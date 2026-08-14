//! Windows capture backend — WGC via windows-capture for video frames and
//! WASAPI via cpal for system audio (loopback) + microphone.

use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, bail, Result};
use windows_capture::capture::{Context, CaptureControl, GraphicsCaptureApiHandler};
use windows_capture::frame::Frame;
use windows_capture::graphics_capture_api::InternalCaptureControl;
use windows_capture::monitor::Monitor;
use windows_capture::settings::{
    ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
    MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
};

use crate::core::geometry::Rect;

use super::{CapturedDisplay, PlatformRecorder};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

// ---------------------------------------------------------------------------
// Frozen display capture (xcap / WGC)
// ---------------------------------------------------------------------------

pub fn capture_active_display() -> Result<CapturedDisplay> {
    let (cursor_x, cursor_y) = cursor_position()?;
    let monitors = xcap::Monitor::all()?;
    let monitor = monitors
        .iter()
        .find(|monitor| {
            let (Ok(mx), Ok(my), Ok(mw), Ok(mh)) = (
                monitor.x(),
                monitor.y(),
                monitor.width(),
                monitor.height(),
            ) else {
                return false;
            };
            cursor_x >= mx
                && cursor_x < mx + mw as i32
                && cursor_y >= my
                && cursor_y < my + mh as i32
        })
        .or_else(|| monitors.first())
        .ok_or_else(|| anyhow!("The active display could not be captured."))?;

    let image = monitor.capture_image()?;
    let scale = monitor.scale_factor().unwrap_or(1.0).max(1.0) as f64;
    let width = monitor.width()? as i64;
    let height = monitor.height()? as i64;

    let mut png_bytes = Vec::new();
    image.write_to(
        &mut std::io::Cursor::new(&mut png_bytes),
        image::ImageFormat::Png,
    )?;

    // Enumerate visible windows of other processes, clipped to the monitor,
    // in display-local top-left points (front-to-back order).
    let own_pid = std::process::id();
    let mut window_rects = Vec::new();
    for window in xcap::Window::all()? {
        let (Ok(pid), Ok(x), Ok(y), Ok(w), Ok(h)) = (
            window.pid(),
            window.x(),
            window.y(),
            window.width(),
            window.height(),
        ) else {
            continue;
        };
        if pid == own_pid || window.is_minimized().unwrap_or(false) {
            continue;
        }
        let x = x as f64;
        let y = y as f64;
        let w = w as f64;
        let h = h as f64;
        let (Ok(mx), Ok(my), Ok(mw), Ok(mh)) = (
            monitor.x(),
            monitor.y(),
            monitor.width(),
            monitor.height(),
        ) else {
            continue;
        };
        let mx = mx as f64;
        let my = my as f64;
        let mw = mw as f64;
        let mh = mh as f64;
        let min_x = x.max(mx);
        let min_y = y.max(my);
        let max_x = (x + w).min(mx + mw);
        let max_y = (y + h).min(my + mh);
        let cw = (max_x - min_x) / scale;
        let ch = (max_y - min_y) / scale;
        if cw < 8.0 || ch < 8.0 {
            continue;
        }
        window_rects.push(Rect::new(
            (min_x - mx) / scale,
            (min_y - my) / scale,
            cw,
            ch,
        ));
    }

    Ok(CapturedDisplay {
        png_data: png_bytes,
        pixel_width: width,
        pixel_height: height,
        screen_frame: Rect::new(0.0, 0.0, width as f64 / scale, height as f64 / scale),
        window_rects,
        display_id: monitor_index(monitor)?,
        backing_scale: scale,
    })
}

fn cursor_position() -> Result<(i32, i32)> {
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
    use windows::Win32::Foundation::POINT;
    let mut point = POINT { x: 0, y: 0 };
    unsafe { GetCursorPos(&mut point) }.map_err(|e| anyhow!("GetCursorPos failed: {e}"))?;
    Ok((point.x, point.y))
}

fn monitor_index(monitor: &xcap::Monitor) -> Result<u32> {
    // xcap monitors order matches EnumDisplayMonitors ordering; reuse the
    // enumeration index for the WGC Monitor::from_index lookup.
    let monitors = xcap::Monitor::all()?;
    let target_id = monitor.id().map_err(|e| anyhow!("{e}"))?;
    monitors
        .iter()
        .position(|m| m.id().map(|id| id == target_id).unwrap_or(false))
        .map(|i| i as u32)
        .ok_or_else(|| anyhow!("display unavailable"))
}

// ---------------------------------------------------------------------------
// Recording (WGC frames + WASAPI audio)
// ---------------------------------------------------------------------------

struct WinHandlerState {
    video_tx: Option<mpsc::Sender<Vec<u8>>>,
    region_px: Rect,
}

struct WinCaptureHandler {
    slot: Arc<Mutex<WinHandlerState>>,
    _audio_streams: Vec<cpal::Stream>,
}

impl GraphicsCaptureApiHandler for WinCaptureHandler {
    type Flags = (Arc<Mutex<WinHandlerState>>, u32);
    type Error = Box<dyn std::error::Error + Send + Sync>;

    fn new(ctx: Context<Self::Flags>) -> std::result::Result<Self, Self::Error> {
        Ok(Self {
            slot: ctx.flags.0,
            _audio_streams: Vec::new(),
        })
    }

    fn on_frame_arrived(
        &mut self,
        frame: &mut Frame,
        _capture_control: InternalCaptureControl,
    ) -> std::result::Result<(), Self::Error> {
        let state = self.slot.lock().unwrap();
        let Some(tx) = state.video_tx.as_ref() else {
            return Ok(());
        };
        let mut buffer = Vec::new();
        let raw = frame.buffer()?.as_nopadding_buffer(&mut buffer).to_vec();
        let width = frame.width() as usize;
        let height = frame.height() as usize;
        let region = state.region_px;
        let region_w = region.width as usize;
        let region_h = region.height as usize;
        let mut out = Vec::with_capacity(region_w * region_h * 4);
        for row in 0..region_h {
            let src_row = row + region.y as usize;
            if src_row >= height {
                break;
            }
            let src_start = (src_row * width + region.x as usize) * 4;
            let row_bytes = region_w * 4;
            let end = src_start.saturating_add(row_bytes).min(raw.len());
            if src_start >= raw.len() {
                break;
            }
            let src = &raw[src_start..end];
            for pixel in src.chunks_exact(4) {
                out.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
            }
        }
        let _ = tx.send(out);
        Ok(())
    }

    fn on_closed(&mut self) -> std::result::Result<(), Self::Error> {
        Ok(())
    }
}

pub struct WindowsRecorder {
    control: Option<CaptureControl<WinCaptureHandler, Box<dyn std::error::Error + Send + Sync>>>,
}

impl WindowsRecorder {
    pub fn start(
        display_id: u32,
        region: Rect,
        backing_scale: f64,
        options: crate::core::policy::RecordingOptions,
        video_tx: mpsc::Sender<Vec<u8>>,
        audio_tx: Option<mpsc::Sender<Vec<u8>>>,
        mic_tx: Option<mpsc::Sender<Vec<u8>>>,
    ) -> Result<WindowsRecorder> {
        if region.width < 2.0 || region.height < 2.0 {
            bail!("The recording region is too small.");
        }
        let monitor = Monitor::from_index(display_id as usize)
            .map_err(|e| anyhow!("{e}"))?;
        let monitor_width = monitor.width()?;
        let monitor_height = monitor.height()?;
        let _ = monitor_height;

        let region_px = Rect::new(
            region.x * backing_scale,
            region.y * backing_scale,
            region.width * backing_scale,
            region.height * backing_scale,
        );

        let cursor = if options.shows_cursor {
            CursorCaptureSettings::Default
        } else {
            CursorCaptureSettings::WithoutCursor
        };

        // Start audio capture (system loopback + microphone) when requested.
        let _audio_streams = start_audio(options, audio_tx, mic_tx);

        let cursor = if options.shows_cursor {
            CursorCaptureSettings::Default
        } else {
            CursorCaptureSettings::WithoutCursor
        };

        let slot = Arc::new(Mutex::new(WinHandlerState {
            video_tx: Some(video_tx),
            region_px,
        }));
        let state_slot = slot.clone();

        let settings = Settings::new(
            monitor,
            cursor,
            DrawBorderSettings::WithoutBorder,
            SecondaryWindowSettings::Default,
            MinimumUpdateIntervalSettings::Default,
            DirtyRegionSettings::Default,
            ColorFormat::Rgba8,
            (state_slot, monitor_width),
        );

        let control = WinCaptureHandler::start_free_threaded(settings)
            .map_err(|e| anyhow!("{e}"))?;
        let _ = slot;

        Ok(WindowsRecorder {
            control: Some(control),
        })
    }
}

impl PlatformRecorder for WindowsRecorder {
    fn stop(&mut self) -> Result<()> {
        // `CaptureControl::stop` consumes itself, so take it out of the slot
        // and drop it after the capture thread has been joined.
        if let Some(control) = self.control.take() {
            control.stop().map_err(|e| anyhow!("{e}"))?;
        }
        Ok(())
    }
}

fn start_audio(
    options: crate::core::policy::RecordingOptions,
    audio_tx: Option<mpsc::Sender<Vec<u8>>>,
    mic_tx: Option<mpsc::Sender<Vec<u8>>>,
) -> Vec<cpal::Stream> {
    let mut streams = Vec::new();
    let Some(tx) = audio_tx else {
        return streams;
    };
    let host = cpal::default_host();
    if options.captures_system_audio {
        if let Some(device) = host.default_output_device() {
            // Prefer 48 kHz stereo f32 to match the ffmpeg pipe contract.
            let chosen = device
                .supported_input_configs()
                .ok()
                .and_then(|configs| {
                    configs
                        .filter(|c| c.channels() == 2)
                        .map(|c| c.with_sample_rate(48_000))
                        .find(|c| c.sample_format() == cpal::SampleFormat::F32)
                })
                .or_else(|| device.default_input_config().ok());
            if let Some(config) = chosen {
                let tx_clone = tx.clone();
                match config.sample_format() {
                    cpal::SampleFormat::F32 => {
                        if let Ok(stream) = device.build_input_stream(
                            config.into(),
                            move |data: &[f32], _| {
                                let _ = tx_clone.send(to_bytes(data));
                            },
                            |_| {},
                            None,
                        ) {
                            let _ = stream.play();
                            streams.push(stream);
                        }
                    }
                    cpal::SampleFormat::I16 | cpal::SampleFormat::U16 => {
                        if let Ok(stream) = device.build_input_stream(
                            config.into(),
                            move |data: &[i16], _| {
                                let _ = tx_clone.send(to_bytes_i16(data));
                            },
                            |_| {},
                            None,
                        ) {
                            let _ = stream.play();
                            streams.push(stream);
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    if options.captures_microphone {
        if let Some(tx) = mic_tx {
            if let Some(device) = host.default_input_device() {
                // Prefer a 48 kHz stereo f32 stream so the ffmpeg pipe
                // contract (48k/2ch/f32) always matches; fall back to the
                // device default config otherwise.
                let chosen = device
                    .supported_input_configs()
                    .ok()
                    .and_then(|configs| {
                        configs
                            .filter(|c| c.channels() == 2)
                            .map(|c| c.with_sample_rate(48_000))
                            .find(|c| c.sample_format() == cpal::SampleFormat::F32)
                    })
                    .or_else(|| device.default_input_config().ok());
                if let Some(config) = chosen {
                    match config.sample_format() {
                        cpal::SampleFormat::F32 => {
                            if let Ok(stream) = device.build_input_stream(
                                config.into(),
                                move |data: &[f32], _| {
                                    let _ = tx.send(to_bytes(data));
                                },
                                |_| {},
                                None,
                            ) {
                                let _ = stream.play();
                                streams.push(stream);
                            }
                        }
                        cpal::SampleFormat::I16 | cpal::SampleFormat::U16 => {
                            if let Ok(stream) = device.build_input_stream(
                                config.into(),
                                move |data: &[i16], _| {
                                    let _ = tx.send(to_bytes_i16(data));
                                },
                                |_| {},
                                None,
                            ) {
                                let _ = stream.play();
                                streams.push(stream);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    streams
}

fn to_bytes(samples: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(samples.len() * 4);
    for sample in samples {
        out.extend_from_slice(&sample.to_le_bytes());
    }
    out
}

fn to_bytes_i16(samples: &[i16]) -> Vec<u8> {
    let mut out = Vec::with_capacity(samples.len() * 2);
    for sample in samples {
        out.extend_from_slice(&sample.to_le_bytes());
    }
    out
}

