//! Windows capture backend — WGC via windows-capture for video frames and
//! WASAPI via cpal for system audio (loopback) + microphone.

use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, bail, Result};
use windows_capture::capture::{CaptureControl, Context, GraphicsCaptureApiHandler};
use windows_capture::frame::Frame;
use windows_capture::graphics_capture_api::InternalCaptureControl;
use windows_capture::monitor::Monitor;
use windows_capture::settings::{
    ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
    MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
};

use crate::core::geometry::Rect;
use crate::record::{AudioSampleFormat, AudioSpec};

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
            let (Ok(mx), Ok(my), Ok(mw), Ok(mh)) =
                (monitor.x(), monitor.y(), monitor.width(), monitor.height())
            else {
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
        let (Ok(mx), Ok(my), Ok(mw), Ok(mh)) =
            (monitor.x(), monitor.y(), monitor.width(), monitor.height())
        else {
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
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
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
    region_px: PixelRegion,
}

#[derive(Clone, Copy)]
struct PixelRegion {
    x: usize,
    y: usize,
    width: usize,
    height: usize,
}

struct WinCaptureHandler {
    slot: Arc<Mutex<WinHandlerState>>,
}

impl GraphicsCaptureApiHandler for WinCaptureHandler {
    type Flags = Arc<Mutex<WinHandlerState>>;
    type Error = Box<dyn std::error::Error + Send + Sync>;

    fn new(ctx: Context<Self::Flags>) -> std::result::Result<Self, Self::Error> {
        Ok(Self { slot: ctx.flags })
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
        let out = crop_rgba_to_bgra(&raw, width, height, state.region_px);
        let _ = tx.send(out);
        Ok(())
    }

    fn on_closed(&mut self) -> std::result::Result<(), Self::Error> {
        Ok(())
    }
}

fn crop_rgba_to_bgra(
    raw: &[u8],
    frame_width: usize,
    frame_height: usize,
    region: PixelRegion,
) -> Vec<u8> {
    // Always return one complete encoder frame. A display-size transition or
    // an edge-rounding pixel must not shift every subsequent rawvideo frame.
    let mut out = vec![0; region.width.saturating_mul(region.height).saturating_mul(4)];
    if region.x >= frame_width || region.y >= frame_height {
        return out;
    }

    let copy_width = region.width.min(frame_width - region.x);
    let copy_height = region.height.min(frame_height - region.y);
    for row in 0..copy_height {
        let source_start = ((region.y + row) * frame_width + region.x).saturating_mul(4);
        if source_start >= raw.len() {
            break;
        }
        let available_pixels = (raw.len() - source_start) / 4;
        let row_pixels = copy_width.min(available_pixels);
        let destination_start = row * region.width * 4;
        for column in 0..row_pixels {
            let source = source_start + column * 4;
            let destination = destination_start + column * 4;
            out[destination..destination + 4].copy_from_slice(&[
                raw[source + 2],
                raw[source + 1],
                raw[source],
                raw[source + 3],
            ]);
        }
    }
    out
}

pub struct WindowsRecorder {
    control: Option<CaptureControl<WinCaptureHandler, Box<dyn std::error::Error + Send + Sync>>>,
    _audio_streams: Vec<cpal::Stream>,
    system_audio_spec: Option<AudioSpec>,
    microphone_spec: Option<AudioSpec>,
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
        let monitor = Monitor::from_index(display_id as usize).map_err(|e| anyhow!("{e}"))?;

        let region_px = PixelRegion {
            x: (region.x * backing_scale).round().max(0.0) as usize,
            y: (region.y * backing_scale).round().max(0.0) as usize,
            width: crate::core::policy::RecordingPolicy::pixel_dimension(
                region.width,
                backing_scale,
            ) as usize,
            height: crate::core::policy::RecordingPolicy::pixel_dimension(
                region.height,
                backing_scale,
            ) as usize,
        };

        // Start audio capture (system loopback + microphone) when requested.
        let audio = start_audio(options, audio_tx, mic_tx)?;

        let cursor = if options.shows_cursor {
            CursorCaptureSettings::Default
        } else {
            CursorCaptureSettings::WithoutCursor
        };

        let state_slot = Arc::new(Mutex::new(WinHandlerState {
            video_tx: Some(video_tx),
            region_px,
        }));

        let settings = Settings::new(
            monitor,
            cursor,
            DrawBorderSettings::WithoutBorder,
            SecondaryWindowSettings::Default,
            MinimumUpdateIntervalSettings::Default,
            DirtyRegionSettings::Default,
            ColorFormat::Rgba8,
            state_slot,
        );

        let control =
            WinCaptureHandler::start_free_threaded(settings).map_err(|e| anyhow!("{e}"))?;

        Ok(WindowsRecorder {
            control: Some(control),
            _audio_streams: audio.streams,
            system_audio_spec: audio.system_audio_spec,
            microphone_spec: audio.microphone_spec,
        })
    }

    pub fn system_audio_spec(&self) -> Option<AudioSpec> {
        self.system_audio_spec
    }

    pub fn microphone_spec(&self) -> Option<AudioSpec> {
        self.microphone_spec
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
) -> Result<StartedAudio> {
    let mut streams = Vec::new();
    let mut system_audio_spec = None;
    let mut microphone_spec = None;
    let host = cpal::default_host();

    if options.captures_system_audio {
        let tx = audio_tx.ok_or_else(|| anyhow!("The system-audio pipe is unavailable."))?;
        let device = host
            .default_output_device()
            .ok_or_else(|| anyhow!("No system-audio output device is available."))?;
        let (stream, spec) = build_audio_stream(&device, AudioDeviceKind::Loopback, tx)?;
        streams.push(stream);
        system_audio_spec = Some(spec);
    }

    if options.captures_microphone {
        let tx = mic_tx.ok_or_else(|| anyhow!("The microphone pipe is unavailable."))?;
        let device = host
            .default_input_device()
            .ok_or_else(|| anyhow!("No microphone input device is available."))?;
        let (stream, spec) = build_audio_stream(&device, AudioDeviceKind::Microphone, tx)?;
        streams.push(stream);
        microphone_spec = Some(spec);
    }

    Ok(StartedAudio {
        streams,
        system_audio_spec,
        microphone_spec,
    })
}

struct StartedAudio {
    streams: Vec<cpal::Stream>,
    system_audio_spec: Option<AudioSpec>,
    microphone_spec: Option<AudioSpec>,
}

#[derive(Clone, Copy)]
enum AudioDeviceKind {
    Loopback,
    Microphone,
}

fn build_audio_stream(
    device: &cpal::Device,
    kind: AudioDeviceKind,
    tx: mpsc::Sender<Vec<u8>>,
) -> Result<(cpal::Stream, AudioSpec)> {
    let config = preferred_audio_config(device, kind)?;
    let format = match config.sample_format() {
        cpal::SampleFormat::F32 => AudioSampleFormat::F32,
        cpal::SampleFormat::I16 => AudioSampleFormat::I16,
        cpal::SampleFormat::U16 => AudioSampleFormat::U16,
        other => bail!("Unsupported audio sample format: {other}"),
    };
    let spec = AudioSpec {
        sample_rate: config.sample_rate(),
        channels: u32::from(config.channels()),
        format,
    };
    let stream = device
        .build_input_stream_raw(
            config.config(),
            config.sample_format(),
            move |data, _| {
                let _ = tx.send(data.bytes().to_vec());
            },
            |_| {},
            None,
        )
        .map_err(|error| anyhow!("Could not start the audio input stream: {error}"))?;
    stream
        .play()
        .map_err(|error| anyhow!("Could not activate the audio input stream: {error}"))?;
    Ok((stream, spec))
}

fn preferred_audio_config(
    device: &cpal::Device,
    kind: AudioDeviceKind,
) -> Result<cpal::SupportedStreamConfig> {
    let ranges = match kind {
        AudioDeviceKind::Loopback => device
            .supported_output_configs()
            .map(|configs| configs.collect::<Vec<_>>()),
        AudioDeviceKind::Microphone => device
            .supported_input_configs()
            .map(|configs| configs.collect::<Vec<_>>()),
    };
    let selected = ranges.ok().and_then(|ranges| {
        ranges
            .into_iter()
            .filter(|range| supported_sample_format(range.sample_format()))
            .map(|range| {
                let sample_rate = 48_000.clamp(range.min_sample_rate(), range.max_sample_rate());
                range.with_sample_rate(sample_rate)
            })
            .min_by_key(audio_config_rank)
    });
    if let Some(config) = selected {
        return Ok(config);
    }

    let fallback = match kind {
        AudioDeviceKind::Loopback => device.default_output_config(),
        AudioDeviceKind::Microphone => device.default_input_config(),
    }
    .map_err(|error| anyhow!("Could not query the audio device format: {error}"))?;
    if !supported_sample_format(fallback.sample_format()) {
        bail!(
            "Unsupported audio sample format: {}",
            fallback.sample_format()
        );
    }
    Ok(fallback)
}

fn supported_sample_format(format: cpal::SampleFormat) -> bool {
    matches!(
        format,
        cpal::SampleFormat::F32 | cpal::SampleFormat::I16 | cpal::SampleFormat::U16
    )
}

fn audio_config_rank(config: &cpal::SupportedStreamConfig) -> (u8, u8, u32) {
    let channels = match config.channels() {
        2 => 0,
        1 => 1,
        channels => 2u8.saturating_add(channels.min(u8::MAX as u16) as u8),
    };
    let format = match config.sample_format() {
        cpal::SampleFormat::F32 => 0,
        cpal::SampleFormat::I16 => 1,
        cpal::SampleFormat::U16 => 2,
        _ => u8::MAX,
    };
    (channels, format, config.sample_rate().abs_diff(48_000))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_config_rank_prefers_stereo_float_at_48khz() {
        let ideal = cpal::SupportedStreamConfig::new(
            2,
            48_000,
            cpal::SupportedBufferSize::Unknown,
            cpal::SampleFormat::F32,
        );
        let mono = cpal::SupportedStreamConfig::new(
            1,
            48_000,
            cpal::SupportedBufferSize::Unknown,
            cpal::SampleFormat::F32,
        );
        let integer = cpal::SupportedStreamConfig::new(
            2,
            48_000,
            cpal::SupportedBufferSize::Unknown,
            cpal::SampleFormat::I16,
        );
        let wrong_rate = cpal::SupportedStreamConfig::new(
            2,
            44_100,
            cpal::SupportedBufferSize::Unknown,
            cpal::SampleFormat::F32,
        );

        assert!(audio_config_rank(&ideal) < audio_config_rank(&mono));
        assert!(audio_config_rank(&ideal) < audio_config_rank(&integer));
        assert!(audio_config_rank(&ideal) < audio_config_rank(&wrong_rate));
    }

    #[test]
    fn crop_produces_exact_bgra_frame_and_pads_display_edges() {
        let rgba = [
            1, 2, 3, 255, 4, 5, 6, 255, // row 0
            7, 8, 9, 255, 10, 11, 12, 255, // row 1
        ];
        let region = PixelRegion {
            x: 1,
            y: 0,
            width: 2,
            height: 2,
        };

        let cropped = crop_rgba_to_bgra(&rgba, 2, 2, region);
        assert_eq!(cropped.len(), 2 * 2 * 4);
        assert_eq!(&cropped[0..4], &[6, 5, 4, 255]);
        assert_eq!(&cropped[4..8], &[0, 0, 0, 0]);
        assert_eq!(&cropped[8..12], &[12, 11, 10, 255]);
        assert_eq!(&cropped[12..16], &[0, 0, 0, 0]);
    }
}
