//! Windows capture backend — WGC via windows-capture for video frames and
//! WASAPI via cpal for system audio (loopback) + microphone.

use std::sync::mpsc;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Result};
use image::ImageEncoder;
use windows_capture::capture::{CaptureControl, Context, GraphicsCaptureApiHandler};
use windows_capture::frame::Frame;
use windows_capture::graphics_capture_api::{GraphicsCaptureApi, InternalCaptureControl};
use windows_capture::monitor::Monitor;
use windows_capture::settings::{
    ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
    MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
};

use crate::core::geometry::Rect;
use crate::record::{AudioChunkSender, AudioQueueSendError, AudioSampleFormat, AudioSpec};

use super::{
    logical_monitor_frame, unique_display_identity_index, CaptureHealth, CapturedDisplay,
    DisplayIdentity, PlatformRecorder,
};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

// ---------------------------------------------------------------------------
// Frozen display capture (xcap / GDI)
// ---------------------------------------------------------------------------

const FROZEN_NATIVE_FRAME_TIMEOUT: Duration = Duration::from_secs(8);
const FROZEN_POSTPROCESS_SLOW_WARNING: Duration = Duration::from_secs(30);
static FROZEN_CAPTURE_WORKER_ACTIVE: AtomicBool = AtomicBool::new(false);

enum FrozenCaptureMessage<T> {
    NativeFrameReady,
    Finished(Result<T>),
}

struct FrozenCaptureWorkerPermit;

impl Drop for FrozenCaptureWorkerPermit {
    fn drop(&mut self) {
        FROZEN_CAPTURE_WORKER_ACTIVE.store(false, Ordering::Release);
    }
}

pub fn capture_active_display() -> Result<CapturedDisplay> {
    if FROZEN_CAPTURE_WORKER_ACTIVE
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        bail!(
            "A previous Windows screen capture is still waiting for the operating system. Restart Kiri if capture remains unavailable."
        );
    }

    let (sender, receiver) = mpsc::channel();
    let worker = std::thread::Builder::new()
        .name("kiri-frozen-capture".into())
        .spawn(move || {
            let result = {
                let _permit = FrozenCaptureWorkerPermit;
                capture_active_display_inner(&sender)
            };
            if sender.send(FrozenCaptureMessage::Finished(result)).is_err() {
                log::warn!("Windows frozen capture finished after its caller stopped waiting");
            }
        });
    if let Err(error) = worker {
        FROZEN_CAPTURE_WORKER_ACTIVE.store(false, Ordering::Release);
        return Err(anyhow!(
            "Could not start the Windows capture worker: {error}"
        ));
    }

    receive_frozen_capture(
        receiver,
        FROZEN_NATIVE_FRAME_TIMEOUT,
        FROZEN_POSTPROCESS_SLOW_WARNING,
    )
}

fn receive_frozen_capture<T>(
    receiver: mpsc::Receiver<FrozenCaptureMessage<T>>,
    native_frame_timeout: Duration,
    postprocess_slow_warning: Duration,
) -> Result<T> {
    match receiver.recv_timeout(native_frame_timeout) {
        Ok(FrozenCaptureMessage::Finished(result)) => return result,
        Ok(FrozenCaptureMessage::NativeFrameReady) => {}
        Err(mpsc::RecvTimeoutError::Timeout) => bail!(
            "Windows screen capture did not return a frame within {} seconds. Restart Kiri if capture remains unavailable.",
            native_frame_timeout.as_secs()
        ),
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            bail!("The Windows screen capture worker stopped unexpectedly.")
        }
    }

    match receiver.recv_timeout(postprocess_slow_warning) {
        Ok(FrozenCaptureMessage::Finished(result)) => result,
        Ok(FrozenCaptureMessage::NativeFrameReady) => {
            bail!("The Windows screen capture worker reported its first frame more than once.")
        }
        Err(mpsc::RecvTimeoutError::Timeout) => {
            log::warn!(
                "Windows frozen capture: post-processing is still running after {} seconds; waiting for completion",
                postprocess_slow_warning.as_secs()
            );
            match receiver.recv() {
                Ok(FrozenCaptureMessage::Finished(result)) => result,
                Ok(FrozenCaptureMessage::NativeFrameReady) => bail!(
                    "The Windows screen capture worker reported its first frame more than once."
                ),
                Err(_) => bail!("The Windows screen capture worker stopped unexpectedly."),
            }
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            bail!("The Windows screen capture worker stopped unexpectedly.")
        }
    }
}

fn capture_active_display_inner(
    progress: &mpsc::Sender<FrozenCaptureMessage<CapturedDisplay>>,
) -> Result<CapturedDisplay> {
    let worker_started = Instant::now();
    log::info!("Windows frozen capture: worker started");
    let (cursor_x, cursor_y) = cursor_position()?;
    let monitor_enumeration_started = Instant::now();
    let monitors = xcap::Monitor::all()?;
    log::info!(
        "Windows frozen capture: enumerated {} monitor(s) in {} ms",
        monitors.len(),
        monitor_enumeration_started.elapsed().as_millis()
    );
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

    log::info!("Windows frozen capture: requesting desktop frame through GDI");
    let frame_started = Instant::now();
    let image = monitor.capture_image()?;
    log::info!(
        "Windows frozen capture: GDI desktop frame received in {} ms ({} ms total)",
        frame_started.elapsed().as_millis(),
        worker_started.elapsed().as_millis()
    );
    progress
        .send(FrozenCaptureMessage::NativeFrameReady)
        .map_err(|_| anyhow!("The Windows screen capture caller stopped waiting."))?;

    let metadata_started = Instant::now();
    let scale = monitor.scale_factor().unwrap_or(1.0).max(1.0) as f64;
    let monitor_x = monitor.x()?;
    let monitor_y = monitor.y()?;
    let width = monitor.width()? as i64;
    let height = monitor.height()? as i64;
    let display_identity = DisplayIdentity {
        device_name: monitor.name()?,
        physical_x: monitor_x,
        physical_y: monitor_y,
        physical_width: u32::try_from(width).map_err(|_| anyhow!("Invalid display width."))?,
        physical_height: u32::try_from(height).map_err(|_| anyhow!("Invalid display height."))?,
        scale_factor: scale,
    };
    log::info!(
        "Windows frozen capture: display metadata resolved in {} ms",
        metadata_started.elapsed().as_millis()
    );

    let png_started = Instant::now();
    let png_bytes = encode_frozen_capture_png(&image)?;
    log::info!(
        "Windows frozen capture: PNG encoded in {} ms ({} bytes)",
        png_started.elapsed().as_millis(),
        png_bytes.len()
    );

    // Enumerate visible windows of other processes, clipped to the monitor,
    // in display-local top-left points. EnumWindows already returns them in
    // front-to-back order, which is also the overlay's hit-test order.
    let window_enumeration_started = Instant::now();
    let window_rects =
        collect_frozen_window_rects(monitor_x, monitor_y, width as u32, height as u32, scale);
    log::info!(
        "Windows frozen capture: collected {} window candidate(s) in {} ms",
        window_rects.len(),
        window_enumeration_started.elapsed().as_millis()
    );

    let display_index_started = Instant::now();
    let display_id = monitor_index(monitor)?;
    log::info!(
        "Windows frozen capture: display index resolved in {} ms",
        display_index_started.elapsed().as_millis()
    );
    let captured = CapturedDisplay {
        png_data: png_bytes.into(),
        pixel_width: width,
        pixel_height: height,
        screen_frame: logical_monitor_frame(
            monitor_x,
            monitor_y,
            width as u32,
            height as u32,
            scale,
        ),
        window_rects,
        display_id,
        display_identity: Some(display_identity),
        backing_scale: scale,
    };
    log::info!(
        "Windows frozen capture: worker completed in {} ms",
        worker_started.elapsed().as_millis()
    );
    Ok(captured)
}

fn cursor_position() -> Result<(i32, i32)> {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
    let mut point = POINT { x: 0, y: 0 };
    unsafe { GetCursorPos(&mut point) }.map_err(|e| anyhow!("GetCursorPos failed: {e}"))?;
    Ok((point.x, point.y))
}

fn encode_frozen_capture_png(image: &image::RgbaImage) -> Result<Vec<u8>> {
    let mut png_bytes = Vec::new();
    image::codecs::png::PngEncoder::new_with_quality(
        &mut png_bytes,
        image::codecs::png::CompressionType::Fast,
        image::codecs::png::FilterType::Sub,
    )
    .write_image(
        image.as_raw(),
        image.width(),
        image.height(),
        image::ExtendedColorType::Rgba8,
    )?;
    Ok(png_bytes)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PhysicalWindowBounds {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

fn clipped_logical_window_rect(
    window: PhysicalWindowBounds,
    monitor_x: i32,
    monitor_y: i32,
    monitor_width: u32,
    monitor_height: u32,
    scale: f64,
) -> Option<Rect> {
    let scale = scale.max(1.0);
    let monitor_left = f64::from(monitor_x);
    let monitor_top = f64::from(monitor_y);
    let monitor_right = monitor_left + f64::from(monitor_width);
    let monitor_bottom = monitor_top + f64::from(monitor_height);
    let clipped_left = f64::from(window.left).max(monitor_left);
    let clipped_top = f64::from(window.top).max(monitor_top);
    let clipped_right = f64::from(window.right).min(monitor_right);
    let clipped_bottom = f64::from(window.bottom).min(monitor_bottom);
    let logical_width = (clipped_right - clipped_left) / scale;
    let logical_height = (clipped_bottom - clipped_top) / scale;
    if logical_width < 8.0 || logical_height < 8.0 {
        return None;
    }
    Some(Rect::new(
        (clipped_left - monitor_left) / scale,
        (clipped_top - monitor_top) / scale,
        logical_width,
        logical_height,
    ))
}

fn collect_frozen_window_rects(
    monitor_x: i32,
    monitor_y: i32,
    monitor_width: u32,
    monitor_height: u32,
    scale: f64,
) -> Vec<Rect> {
    use std::ffi::c_void;
    use windows::core::BOOL;
    use windows::Win32::Foundation::{HWND, LPARAM, RECT};
    use windows::Win32::Graphics::Dwm::{
        DwmGetWindowAttribute, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetShellWindow, GetWindowThreadProcessId, IsIconic, IsWindowVisible,
    };

    struct EnumerationContext {
        own_pid: u32,
        shell_window: HWND,
        monitor_x: i32,
        monitor_y: i32,
        monitor_width: u32,
        monitor_height: u32,
        scale: f64,
        rects: Vec<Rect>,
    }

    extern "system" fn collect_window(hwnd: HWND, state: LPARAM) -> BOOL {
        let context = unsafe { &mut *(state.0 as *mut EnumerationContext) };
        let mut pid = 0_u32;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
        if pid == 0 || pid == context.own_pid || hwnd == context.shell_window {
            return BOOL(1);
        }
        if !unsafe { IsWindowVisible(hwnd) }.as_bool() || unsafe { IsIconic(hwnd) }.as_bool() {
            return BOOL(1);
        }

        let mut cloaked = 0_u32;
        let cloak_query = unsafe {
            DwmGetWindowAttribute(
                hwnd,
                DWMWA_CLOAKED,
                &mut cloaked as *mut u32 as *mut c_void,
                std::mem::size_of::<u32>() as u32,
            )
        };
        if cloak_query.is_ok() && cloaked != 0 {
            return BOOL(1);
        }

        let mut bounds = RECT::default();
        if unsafe {
            DwmGetWindowAttribute(
                hwnd,
                DWMWA_EXTENDED_FRAME_BOUNDS,
                &mut bounds as *mut RECT as *mut c_void,
                std::mem::size_of::<RECT>() as u32,
            )
        }
        .is_err()
        {
            return BOOL(1);
        }

        if let Some(rect) = clipped_logical_window_rect(
            PhysicalWindowBounds {
                left: bounds.left,
                top: bounds.top,
                right: bounds.right,
                bottom: bounds.bottom,
            },
            context.monitor_x,
            context.monitor_y,
            context.monitor_width,
            context.monitor_height,
            context.scale,
        ) {
            context.rects.push(rect);
        }
        BOOL(1)
    }

    let mut context = EnumerationContext {
        own_pid: std::process::id(),
        shell_window: unsafe { GetShellWindow() },
        monitor_x,
        monitor_y,
        monitor_width,
        monitor_height,
        scale,
        rects: Vec::new(),
    };
    let result = unsafe {
        EnumWindows(
            Some(collect_window),
            LPARAM(&mut context as *mut EnumerationContext as isize),
        )
    };
    if let Err(error) = result {
        // Window snapping is best-effort; never discard a valid frozen frame
        // because the desktop changed while its windows were enumerated.
        log::warn!("Windows frozen capture: window enumeration failed: {error}");
    }
    context.rects
}

fn monitor_index(monitor: &xcap::Monitor) -> Result<u32> {
    // Both crates follow EnumDisplayMonitors ordering, but xcap exposes the
    // zero-based Vec position while windows-capture requires a one-based
    // index. Store the one-based value in CapturedDisplay so recording cannot
    // accidentally select the preceding monitor (or reject the primary one).
    let monitors = xcap::Monitor::all()?;
    let target_id = monitor.id().map_err(|e| anyhow!("{e}"))?;
    monitors
        .iter()
        .position(|m| m.id().map(|id| id == target_id).unwrap_or(false))
        .map(windows_capture_monitor_index)
        .transpose()?
        .ok_or_else(|| anyhow!("display unavailable"))
}

fn windows_capture_monitor_index(xcap_position: usize) -> Result<u32> {
    let one_based = xcap_position
        .checked_add(1)
        .ok_or_else(|| anyhow!("The display index is too large."))?;
    u32::try_from(one_based).map_err(|_| anyhow!("The display index is too large."))
}

fn current_display_identities() -> Result<Vec<DisplayIdentity>> {
    xcap::Monitor::all()?
        .into_iter()
        .map(|monitor| {
            Ok(DisplayIdentity {
                device_name: monitor.name()?,
                physical_x: monitor.x()?,
                physical_y: monitor.y()?,
                physical_width: monitor.width()?,
                physical_height: monitor.height()?,
                scale_factor: monitor.scale_factor()?.max(1.0) as f64,
            })
        })
        .collect()
}

fn resolve_recording_monitor(expected: &DisplayIdentity) -> Result<Monitor> {
    let current = current_display_identities()?;
    unique_display_identity_index(expected, &current).ok_or_else(|| {
        anyhow!("The selected display changed or is no longer uniquely available. Select it again.")
    })?;

    let mut matches = Monitor::enumerate()?.into_iter().filter_map(|monitor| {
        monitor
            .device_name()
            .ok()
            .filter(|name| name == &expected.device_name)
            .map(|_| monitor)
    });
    let selected = matches
        .next()
        .ok_or_else(|| anyhow!("The selected display is no longer available. Select it again."))?;
    if matches.next().is_some() {
        bail!("The selected display is ambiguous. Select it again.");
    }
    Ok(selected)
}

// ---------------------------------------------------------------------------
// Recording (WGC frames + WASAPI audio)
// ---------------------------------------------------------------------------

struct WinHandlerState {
    video_tx: Option<super::VideoFrameSender>,
    region_px: PixelRegion,
    dropped_frames: u64,
    capture_health: Arc<CaptureHealth>,
    paused: Arc<std::sync::atomic::AtomicBool>,
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
        let mut state = self.slot.lock().unwrap();
        if state.paused.load(std::sync::atomic::Ordering::Acquire) {
            return Ok(());
        }
        let Some(tx) = state.video_tx.as_ref() else {
            return Ok(());
        };
        let width = frame.width() as usize;
        let height = frame.height() as usize;
        let mut frame_buffer = frame.buffer()?;
        let row_pitch = frame_buffer.row_pitch() as usize;
        let out = crop_rgba_to_bgra(
            frame_buffer.as_raw_buffer(),
            width,
            height,
            row_pitch,
            state.region_px,
        );
        drop(frame_buffer);
        if let Err(mpsc::TrySendError::Full(_)) = tx.try_send(out) {
            state.dropped_frames += 1;
            if state.dropped_frames == 1 || state.dropped_frames.is_multiple_of(30) {
                log::warn!(
                    "WindowsRecorder: encoder backpressure dropped {} frames",
                    state.dropped_frames
                );
            }
        }
        Ok(())
    }

    fn on_closed(&mut self) -> std::result::Result<(), Self::Error> {
        let health = Arc::clone(&self.slot.lock().unwrap().capture_health);
        if health.report_unexpected_stop("Windows Graphics Capture closed the session.".into()) {
            log::error!("WindowsRecorder: graphics capture closed unexpectedly");
        }
        Ok(())
    }
}

fn crop_rgba_to_bgra(
    raw: &[u8],
    frame_width: usize,
    frame_height: usize,
    row_pitch: usize,
    region: PixelRegion,
) -> Vec<u8> {
    // Always return one complete encoder frame. A display-size transition or
    // an edge-rounding pixel must not shift every subsequent rawvideo frame.
    let Some(output_len) = region
        .width
        .checked_mul(region.height)
        .and_then(|pixels| pixels.checked_mul(4))
    else {
        return Vec::new();
    };
    let mut out = vec![0; output_len];
    let Some(source_x) = region.x.checked_mul(4) else {
        return out;
    };
    let Some(logical_row_bytes) = frame_width.checked_mul(4) else {
        return out;
    };
    if region.x >= frame_width || region.y >= frame_height || row_pitch == 0 {
        return out;
    }

    let copy_width = region.width.min(frame_width - region.x);
    let copy_height = region.height.min(frame_height - region.y);
    for row in 0..copy_height {
        let Some(source_row) = (region.y + row).checked_mul(row_pitch) else {
            break;
        };
        let Some(source_start) = source_row.checked_add(source_x) else {
            break;
        };
        let source_end = source_row
            .saturating_add(logical_row_bytes.min(row_pitch))
            .min(raw.len());
        if source_start >= source_end {
            continue;
        }
        let available_pixels = (source_end - source_start) / 4;
        let row_pixels = copy_width.min(available_pixels);
        let destination_start = row * region.width * 4;
        let source = &raw[source_start..source_start + row_pixels * 4];
        let destination = &mut out[destination_start..destination_start + row_pixels * 4];
        for (rgba, bgra) in source.chunks_exact(4).zip(destination.chunks_exact_mut(4)) {
            bgra.copy_from_slice(&[rgba[2], rgba[1], rgba[0], rgba[3]]);
        }
    }
    out
}

fn recording_minimum_update_interval() -> MinimumUpdateIntervalSettings {
    match GraphicsCaptureApi::is_minimum_update_interval_supported() {
        Ok(true) => MinimumUpdateIntervalSettings::Custom(std::time::Duration::from_secs_f64(
            1.0 / f64::from(crate::core::policy::RecordingPolicy::FRAMES_PER_SECOND),
        )),
        Ok(false) => MinimumUpdateIntervalSettings::Default,
        Err(error) => {
            log::warn!(
                "WindowsRecorder: could not query capture frame-rate throttling support: {error}"
            );
            MinimumUpdateIntervalSettings::Default
        }
    }
}

pub struct WindowsRecorder {
    control: Option<CaptureControl<WinCaptureHandler, Box<dyn std::error::Error + Send + Sync>>>,
    _audio_streams: Vec<cpal::Stream>,
    system_audio_spec: Option<AudioSpec>,
    microphone_spec: Option<AudioSpec>,
    capture_health: Arc<CaptureHealth>,
    paused: Arc<std::sync::atomic::AtomicBool>,
}

impl WindowsRecorder {
    pub fn start(
        display_identity: &DisplayIdentity,
        region: Rect,
        backing_scale: f64,
        options: crate::core::policy::RecordingOptions,
        video_tx: super::VideoFrameSender,
        audio_tx: Option<AudioChunkSender>,
        mic_tx: Option<AudioChunkSender>,
    ) -> Result<WindowsRecorder> {
        if region.width < 2.0 || region.height < 2.0 {
            bail!("The recording region is too small.");
        }
        let monitor = resolve_recording_monitor(display_identity)?;

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

        let paused = Arc::new(std::sync::atomic::AtomicBool::new(false));

        // Start audio capture (system loopback + microphone) when requested.
        let audio = start_audio(options, audio_tx, mic_tx, Arc::clone(&paused))?;

        let cursor = if options.shows_cursor {
            CursorCaptureSettings::Default
        } else {
            CursorCaptureSettings::WithoutCursor
        };

        let capture_health = Arc::new(CaptureHealth::default());
        let state_slot = Arc::new(Mutex::new(WinHandlerState {
            video_tx: Some(video_tx),
            region_px,
            dropped_frames: 0,
            capture_health: Arc::clone(&capture_health),
            paused: Arc::clone(&paused),
        }));

        let settings = Settings::new(
            monitor,
            cursor,
            DrawBorderSettings::WithoutBorder,
            SecondaryWindowSettings::Default,
            recording_minimum_update_interval(),
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
            capture_health,
            paused,
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
        self.capture_health.begin_expected_stop();
        if let Some(control) = self.control.take() {
            control.stop().map_err(|e| anyhow!("{e}"))?;
        }
        if let Some(error) = self.capture_health.unexpected_failure() {
            bail!("Screen capture stopped unexpectedly: {error}");
        }
        Ok(())
    }

    fn pause(&mut self) -> Result<()> {
        self.paused
            .store(true, std::sync::atomic::Ordering::Release);
        Ok(())
    }

    fn resume(&mut self) -> Result<()> {
        self.paused
            .store(false, std::sync::atomic::Ordering::Release);
        Ok(())
    }
}

fn start_audio(
    options: crate::core::policy::RecordingOptions,
    audio_tx: Option<AudioChunkSender>,
    mic_tx: Option<AudioChunkSender>,
    paused: Arc<std::sync::atomic::AtomicBool>,
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
        let (stream, spec) =
            build_audio_stream(&device, AudioDeviceKind::Loopback, tx, Arc::clone(&paused))?;
        streams.push(stream);
        system_audio_spec = Some(spec);
    }

    if options.captures_microphone {
        let tx = mic_tx.ok_or_else(|| anyhow!("The microphone pipe is unavailable."))?;
        let device = host
            .default_input_device()
            .ok_or_else(|| anyhow!("No microphone input device is available."))?;
        let (stream, spec) = build_audio_stream(
            &device,
            AudioDeviceKind::Microphone,
            tx,
            Arc::clone(&paused),
        )?;
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
    tx: AudioChunkSender,
    paused: Arc<std::sync::atomic::AtomicBool>,
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
    tx.configure(spec)?;
    let dropped_chunks = std::sync::atomic::AtomicU64::new(0);
    let error_tx = tx.clone();
    let stream = device
        .build_input_stream_raw(
            config.config(),
            config.sample_format(),
            move |data, _| {
                if paused.load(std::sync::atomic::Ordering::Acquire) {
                    return;
                }
                match tx.try_send(data.bytes().to_vec()) {
                    Ok(()) | Err(AudioQueueSendError::Closed) => {}
                    Err(
                        error @ (AudioQueueSendError::Unconfigured
                        | AudioQueueSendError::Overloaded),
                    ) => {
                        let dropped =
                            dropped_chunks.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                        if dropped == 1 || dropped.is_multiple_of(100) {
                            log::warn!(
                            "WindowsRecorder: audio queue rejected {dropped} chunks ({error:?})"
                        );
                        }
                    }
                }
            },
            move |error| {
                log::error!("WindowsRecorder: audio input stream failed: {error}");
                error_tx.report_capture_failure();
            },
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
    fn windows_capture_monitor_indices_are_one_based() {
        assert_eq!(windows_capture_monitor_index(0).unwrap(), 1);
        assert_eq!(windows_capture_monitor_index(1).unwrap(), 2);
    }

    #[test]
    fn windows_capture_monitor_index_rejects_overflow() {
        assert!(windows_capture_monitor_index(usize::MAX).is_err());
    }

    #[test]
    fn frozen_capture_fast_png_round_trips_rgba_pixels() {
        let pixels = vec![
            1, 2, 3, 4, 5, 6, 7, 8, // row 0
            9, 10, 11, 12, 13, 14, 15, 16, // row 1
        ];
        let image = image::RgbaImage::from_raw(2, 2, pixels.clone()).unwrap();

        let encoded = encode_frozen_capture_png(&image).unwrap();
        let decoded = image::load_from_memory_with_format(&encoded, image::ImageFormat::Png)
            .unwrap()
            .into_rgba8();

        assert_eq!(decoded.as_raw(), &pixels);
    }

    #[test]
    fn frozen_window_rect_is_clipped_and_scaled_into_monitor_coordinates() {
        let rect = clipped_logical_window_rect(
            PhysicalWindowBounds {
                left: -2_000,
                top: 40,
                right: -800,
                bottom: 800,
            },
            -1_920,
            100,
            1_920,
            1_080,
            2.0,
        )
        .unwrap();

        assert_eq!(rect, Rect::new(0.0, 0.0, 560.0, 350.0));
    }

    #[test]
    fn frozen_window_rect_rejects_offscreen_and_tiny_candidates() {
        assert!(clipped_logical_window_rect(
            PhysicalWindowBounds {
                left: -400,
                top: 10,
                right: -100,
                bottom: 200,
            },
            0,
            0,
            1_920,
            1_080,
            1.0,
        )
        .is_none());
        assert!(clipped_logical_window_rect(
            PhysicalWindowBounds {
                left: 100,
                top: 100,
                right: 110,
                bottom: 110,
            },
            0,
            0,
            1_920,
            1_080,
            2.0,
        )
        .is_none());
    }

    #[test]
    fn frozen_capture_wait_propagates_success() {
        let (sender, receiver) = mpsc::channel::<FrozenCaptureMessage<u8>>();
        sender.send(FrozenCaptureMessage::NativeFrameReady).unwrap();
        sender
            .send(FrozenCaptureMessage::Finished(Ok(42_u8)))
            .unwrap();
        assert_eq!(
            receive_frozen_capture(
                receiver,
                Duration::from_millis(10),
                Duration::from_millis(10),
            )
            .unwrap(),
            42
        );
    }

    #[test]
    fn frozen_capture_wait_allows_slow_postprocessing_after_first_frame() {
        let (sender, receiver) = mpsc::channel::<FrozenCaptureMessage<u8>>();
        sender.send(FrozenCaptureMessage::NativeFrameReady).unwrap();
        let worker = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(30));
            sender
                .send(FrozenCaptureMessage::Finished(Ok(42_u8)))
                .unwrap();
        });

        let value = receive_frozen_capture(
            receiver,
            Duration::from_millis(1),
            Duration::from_millis(500),
        )
        .unwrap();
        worker.join().unwrap();
        assert_eq!(value, 42);
    }

    #[test]
    fn frozen_capture_wait_times_out_before_first_frame() {
        let (_sender, receiver) = mpsc::channel::<FrozenCaptureMessage<u8>>();
        let error = receive_frozen_capture(
            receiver,
            Duration::from_millis(1),
            Duration::from_millis(10),
        )
        .unwrap_err();
        assert!(error.to_string().contains("did not return a frame"));
    }

    #[test]
    fn frozen_capture_wait_continues_after_a_slow_postprocessing_warning() {
        let (sender, receiver) = mpsc::channel();
        sender.send(FrozenCaptureMessage::NativeFrameReady).unwrap();
        let worker = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(30));
            sender
                .send(FrozenCaptureMessage::Finished(Ok(42_u8)))
                .unwrap();
        });

        let value = receive_frozen_capture(
            receiver,
            Duration::from_millis(10),
            Duration::from_millis(1),
        )
        .unwrap();
        worker.join().unwrap();
        assert_eq!(value, 42);
    }

    #[test]
    fn frozen_capture_wait_propagates_errors_before_and_after_first_frame() {
        let (sender, receiver) = mpsc::channel::<FrozenCaptureMessage<u8>>();
        sender
            .send(FrozenCaptureMessage::Finished(Err(anyhow!(
                "native failed"
            ))))
            .unwrap();
        assert_eq!(
            receive_frozen_capture(
                receiver,
                Duration::from_millis(10),
                Duration::from_millis(10),
            )
            .unwrap_err()
            .to_string(),
            "native failed"
        );

        let (sender, receiver) = mpsc::channel::<FrozenCaptureMessage<u8>>();
        sender.send(FrozenCaptureMessage::NativeFrameReady).unwrap();
        sender
            .send(FrozenCaptureMessage::Finished(Err(anyhow!(
                "post-processing failed"
            ))))
            .unwrap();
        assert_eq!(
            receive_frozen_capture(
                receiver,
                Duration::from_millis(10),
                Duration::from_millis(10),
            )
            .unwrap_err()
            .to_string(),
            "post-processing failed"
        );
    }

    #[test]
    fn frozen_capture_wait_reports_a_disconnected_worker() {
        let (sender, receiver) = mpsc::channel::<FrozenCaptureMessage<u8>>();
        drop(sender);
        let error = receive_frozen_capture(
            receiver,
            Duration::from_millis(10),
            Duration::from_millis(10),
        )
        .unwrap_err();
        assert!(error.to_string().contains("stopped unexpectedly"));
    }

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
    fn crop_honors_row_padding_and_swaps_rgba_to_bgra() {
        let rgba = [
            1, 2, 3, 4, 5, 6, 7, 8, 99, 99, 99, 99, // row 0 + padding
            9, 10, 11, 12, 13, 14, 15, 16, 88, 88, 88, 88, // row 1 + padding
        ];
        let region = PixelRegion {
            x: 0,
            y: 0,
            width: 2,
            height: 2,
        };

        let cropped = crop_rgba_to_bgra(&rgba, 2, 2, 12, region);
        assert_eq!(cropped.len(), 2 * 2 * 4);
        assert_eq!(
            cropped,
            [3, 2, 1, 4, 7, 6, 5, 8, 11, 10, 9, 12, 15, 14, 13, 16]
        );
    }

    #[test]
    fn crop_produces_exact_frame_and_pads_out_of_bounds_pixels() {
        let rgba = [
            1, 2, 3, 255, 4, 5, 6, 255, // row 0
            7, 8, 9, 255, 10, 11, 12, 255, // row 1
        ];
        let region = PixelRegion {
            x: 1,
            y: 1,
            width: 2,
            height: 2,
        };

        let cropped = crop_rgba_to_bgra(&rgba, 2, 2, 8, region);
        assert_eq!(cropped.len(), 2 * 2 * 4);
        assert_eq!(&cropped[0..4], &[12, 11, 10, 255]);
        assert!(cropped[4..].iter().all(|byte| *byte == 0));
    }
}
