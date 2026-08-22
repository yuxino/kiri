//! macOS capture backend — ScreenCaptureKit via objc2, mirroring
//! CaptureCoordinator.swift / RegionRecorder.swift (legacy backend).

use std::sync::mpsc;
use std::sync::OnceLock;

use anyhow::{anyhow, bail, Result};
use block2::RcBlock;
use objc2::msg_send;
use objc2::rc::Retained;
use objc2::runtime::{NSObjectProtocol, ProtocolObject};
use objc2::{AnyThread, DefinedClass};
use objc2_app_kit::{NSBitmapImageRep, NSEvent, NSScreen};
use objc2_core_foundation::{CFRetained, CGPoint, CGRect, CGSize};
use objc2_core_graphics::{CGDirectDisplayID, CGDisplayBounds, CGImage};
use objc2_core_media::{
    CMAudioFormatDescriptionGetStreamBasicDescription, CMBlockBuffer, CMFormatDescription,
    CMSampleBuffer, CMTime,
};
use objc2_core_video::{
    CVPixelBuffer, CVPixelBufferGetBaseAddress, CVPixelBufferGetBytesPerRow,
    CVPixelBufferGetHeight, CVPixelBufferGetWidth, CVPixelBufferLockBaseAddress,
    CVPixelBufferLockFlags, CVPixelBufferUnlockBaseAddress,
};
use objc2_foundation::{NSArray, NSDictionary, NSError, NSNumber, NSObject, NSString};
use objc2_screen_capture_kit::{
    SCContentFilter, SCDisplay, SCScreenshotManager, SCShareableContent, SCStream,
    SCStreamConfiguration, SCStreamDelegate, SCStreamOutput, SCStreamOutputType, SCWindow,
};

use crate::core::geometry::Rect;
use crate::core::policy::RecordingOptions;

use super::CapturedDisplay;

// ---------------------------------------------------------------------------
// Permissions
// ---------------------------------------------------------------------------

extern "C" {
    fn CGPreflightScreenCaptureAccess() -> bool;
    fn CGRequestScreenCaptureAccess() -> bool;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionState {
    Authorized,
    RestartRequired,
    SettingsRequired,
}

/// Process-scoped permission gate. The closures keep the state machine
/// independently testable without invoking macOS privacy APIs from tests.
struct ScreenCapturePermissionGate {
    cached: std::sync::Mutex<Option<PermissionState>>,
}

impl ScreenCapturePermissionGate {
    const fn new() -> Self {
        Self {
            cached: std::sync::Mutex::new(None),
        }
    }

    fn check(
        &self,
        preflight: impl FnOnce() -> bool,
        request: impl FnOnce() -> bool,
    ) -> PermissionState {
        // Hold the gate while checking and requesting so concurrent capture
        // triggers cannot both reach the system request API.
        let mut cached = self.cached.lock().unwrap();
        // Authorized wins unconditionally: if the user grants access in
        // System Settings, preflight overrides any stale denial cache.
        if preflight() {
            *cached = None;
            return PermissionState::Authorized;
        }
        if let Some(state) = *cached {
            return state;
        }
        let state = if request() {
            PermissionState::RestartRequired
        } else {
            PermissionState::SettingsRequired
        };
        *cached = Some(state);
        state
    }
}

// Caches the `CGRequestScreenCaptureAccess` outcome so the system prompt is
// shown at most once per launch (mirrors `ScreenCapturePermissionGate`:
// "已有缓存 → 直接返回缓存值,避免重复弹权限框"). Without this, every
// `start_capture` re-invoked the request API and macOS re-prompts even
// though the user already granted (or already declined) access.
static PERMISSION_GATE: ScreenCapturePermissionGate = ScreenCapturePermissionGate::new();

/// Mirrors `ScreenCapturePermissionGate` (preflight → request, cached).
pub fn check_capture_permission() -> PermissionState {
    PERMISSION_GATE.check(
        || unsafe { CGPreflightScreenCaptureAccess() },
        || unsafe { CGRequestScreenCaptureAccess() },
    )
}

#[cfg(test)]
mod permission_tests {
    use std::cell::Cell;

    use super::{PermissionState, ScreenCapturePermissionGate};

    #[test]
    fn authorized_preflight_never_requests_access() {
        let gate = ScreenCapturePermissionGate::new();
        let request_count = Cell::new(0);

        let state = gate.check(
            || true,
            || {
                request_count.set(request_count.get() + 1);
                false
            },
        );

        assert_eq!(state, PermissionState::Authorized);
        assert_eq!(request_count.get(), 0);
    }

    #[test]
    fn denied_request_is_cached_for_the_process() {
        let gate = ScreenCapturePermissionGate::new();
        let request_count = Cell::new(0);

        for _ in 0..2 {
            let state = gate.check(
                || false,
                || {
                    request_count.set(request_count.get() + 1);
                    false
                },
            );
            assert_eq!(state, PermissionState::SettingsRequired);
        }

        assert_eq!(request_count.get(), 1);
    }

    #[test]
    fn granted_request_is_cached_until_preflight_authorizes() {
        let gate = ScreenCapturePermissionGate::new();
        let request_count = Cell::new(0);

        let first = gate.check(
            || false,
            || {
                request_count.set(request_count.get() + 1);
                true
            },
        );
        let cached = gate.check(
            || false,
            || {
                request_count.set(request_count.get() + 1);
                false
            },
        );
        let authorized = gate.check(|| true, || false);

        assert_eq!(first, PermissionState::RestartRequired);
        assert_eq!(cached, PermissionState::RestartRequired);
        assert_eq!(authorized, PermissionState::Authorized);
        assert_eq!(request_count.get(), 1);
    }
}

// ---------------------------------------------------------------------------
// Frozen display capture (CaptureCoordinator equivalent)
// ---------------------------------------------------------------------------

struct ActiveScreen {
    frame: Rect,
    display_id: CGDirectDisplayID,
    backing_scale: f64,
}

fn make_fixture() -> Result<CapturedDisplay> {
    let screen = active_screen()?;
    let scale = screen.backing_scale;
    let width = (screen.frame.width * scale).round() as u32;
    let height = (screen.frame.height * scale).round() as u32;

    // Two "windows" in display-local points (like the Swift fixture).
    let window_one = Rect::new(
        90.0,
        75.0,
        (620.0_f64).min(screen.frame.width - 180.0),
        (420.0_f64).min(screen.frame.height - 180.0),
    );
    let window_two = Rect::new(
        (240.0_f64).min(screen.frame.width * 0.42),
        155.0,
        (520.0_f64).min(screen.frame.width * 0.48),
        (360.0_f64).min(screen.frame.height - 240.0),
    );

    let mut image = image::RgbaImage::new(width, height);
    // Dark desktop background (Swift fixture: 0.12, 0.14, 0.19).
    for pixel in image.pixels_mut() {
        *pixel = image::Rgba([31, 36, 48, 255]);
    }
    let draw_window = |image: &mut image::RgbaImage, rect: &Rect, color: [u8; 3]| {
        let x0 = (rect.x * scale) as u32;
        let y0 = (rect.y * scale) as u32;
        let w = (rect.width * scale) as u32;
        let h = (rect.height * scale) as u32;
        for y in y0..(y0 + h).min(height) {
            for x in x0..(x0 + w).min(width) {
                image.put_pixel(x, y, image::Rgba([color[0], color[1], color[2], 255]));
            }
        }
    };
    draw_window(&mut image, &window_two, [43, 56, 79]);
    draw_window(&mut image, &window_one, [242, 242, 247]);

    let mut png_bytes = Vec::new();
    image
        .write_to(
            &mut std::io::Cursor::new(&mut png_bytes),
            image::ImageFormat::Png,
        )
        .map_err(|e| anyhow!("{e}"))?;

    Ok(CapturedDisplay {
        png_data: png_bytes,
        pixel_width: width as i64,
        pixel_height: height as i64,
        screen_frame: screen.frame,
        window_rects: vec![window_two, window_one],
        display_id: screen.display_id,
        backing_scale: scale,
    })
}

/// Average brightness (0-255) of a PNG buffer; None if undecodable.
fn png_average(png: &[u8]) -> Option<(u32, u32, f64)> {
    let image = image::load_from_memory(png).ok()?;
    let rgba = image.to_rgba8();
    let (w, h) = rgba.dimensions();
    if w == 0 || h == 0 {
        return None;
    }
    let mut sum = 0.0f64;
    let mut count = 0.0f64;
    let step = ((w * h) / 4000).max(1);
    for (i, pixel) in rgba.pixels().enumerate() {
        if i % step as usize != 0 {
            continue;
        }
        sum += (pixel[0] as f64 + pixel[1] as f64 + pixel[2] as f64) / 3.0;
        count += 1.0;
    }
    Some((w, h, sum / count.max(1.0)))
}

fn active_screen() -> Result<ActiveScreen> {
    let mtm = objc2::MainThreadMarker::new().unwrap();
    let mouse = NSEvent::mouseLocation();
    let point = CGPoint {
        x: mouse.x,
        y: mouse.y,
    };
    let screens = NSScreen::screens(mtm);
    let screen = screens
        .iter()
        .find(|screen| {
            let frame = screen.frame();
            point.x >= frame.origin.x
                && point.x <= frame.origin.x + frame.size.width
                && point.y >= frame.origin.y
                && point.y <= frame.origin.y + frame.size.height
        })
        .or_else(|| screens.firstObject())
        .ok_or_else(|| anyhow!("display unavailable"))?;

    let frame = screen.frame();
    let display_id = screen
        .deviceDescription()
        .objectForKey(&NSString::from_str("NSScreenNumber"))
        .and_then(|value| value.downcast::<NSNumber>().ok())
        .map(|number| number.unsignedIntValue() as CGDirectDisplayID)
        .ok_or_else(|| anyhow!("display unavailable"))?;
    let backing_scale = screen.backingScaleFactor().max(1.0);

    let main_height = NSScreen::mainScreen(mtm)
        .map(|s| s.frame().origin.y + s.frame().size.height)
        .unwrap_or(0.0);
    let top_left = Rect::new(
        frame.origin.x,
        main_height - (frame.origin.y + frame.size.height),
        frame.size.width,
        frame.size.height,
    );
    Ok(ActiveScreen {
        frame: top_left,
        display_id,
        backing_scale,
    })
}

fn shareable_content() -> Result<Retained<SCShareableContent>> {
    // SCShareableContent is MainThreadOnly: the query must run on the main
    // thread even when this is called from a background thread (recording
    // starts off the UI thread). dispatch2 lets us hop to the main queue and
    // wait for the completion handler synchronously.
    let (tx, rx) = mpsc::channel::<std::result::Result<Retained<SCShareableContent>, String>>();
    let block = RcBlock::new(
        move |content: *mut SCShareableContent, error: *mut NSError| {
            let result = if error.is_null() && !content.is_null() {
                // Completion handlers deliver autoreleased (+0) objects; retain.
                Ok(unsafe { Retained::retain(content).unwrap() })
            } else if !error.is_null() {
                let error = unsafe { Retained::retain(error).unwrap() };
                Err(error.localizedDescription().to_string())
            } else {
                Err("shareable content failed".to_string())
            };
            let _ = tx.send(result);
        },
    );
    unsafe {
        SCShareableContent::getShareableContentExcludingDesktopWindows_onScreenWindowsOnly_completionHandler(
            false,
            true,
            &block,
        );
    }
    log::info!("shareable_content: waiting for SCK callback…");
    let result = rx
        .recv_timeout(std::time::Duration::from_secs(8))
        .map_err(|_| anyhow!("shareable content timed out after 8s"))?;
    log::info!("shareable_content: got SCK callback");
    result.map_err(|message| anyhow!(message))
}

fn make_filter(
    content: &SCShareableContent,
    display: &SCDisplay,
    own_process_id: i32,
    excepted_window_ids: &[u32],
) -> Retained<SCContentFilter> {
    let applications = unsafe { content.applications() };
    let kiri_application = applications
        .iter()
        .find(|app| unsafe { app.processID() } == own_process_id);
    match kiri_application {
        Some(application) => {
            let windows = unsafe { content.windows() };
            let excepted: Vec<Retained<SCWindow>> = windows
                .iter()
                .filter(|window| {
                    unsafe { window.owningApplication() }.map(|app| unsafe { app.processID() })
                        == Some(own_process_id)
                        && excepted_window_ids.contains(&unsafe { window.windowID() })
                })
                .collect();
            unsafe {
                let applications = NSArray::from_slice(&[&*application]);
                let excepted_refs: Vec<&SCWindow> = excepted.iter().map(|w| &**w).collect();
                SCContentFilter::initWithDisplay_excludingApplications_exceptingWindows(
                    SCContentFilter::alloc(),
                    display,
                    &applications,
                    &NSArray::from_slice(&excepted_refs),
                )
            }
        }
        None => {
            let windows = unsafe { content.windows() };
            let excluded: Vec<Retained<SCWindow>> = windows
                .iter()
                .filter(|window| {
                    unsafe { window.owningApplication() }.map(|app| unsafe { app.processID() })
                        == Some(own_process_id)
                        && !excepted_window_ids.contains(&unsafe { window.windowID() })
                })
                .collect();
            unsafe {
                let excluded_refs: Vec<&SCWindow> = excluded.iter().map(|w| &**w).collect();
                SCContentFilter::initWithDisplay_excludingWindows(
                    SCContentFilter::alloc(),
                    display,
                    &NSArray::from_slice(&excluded_refs),
                )
            }
        }
    }
}

fn collect_window_rects(content: &SCShareableContent, display_id: CGDirectDisplayID) -> Vec<Rect> {
    let display_bounds = CGDisplayBounds(display_id);
    let own_process_id = std::process::id() as i32;
    let mut rects = Vec::new();
    let windows = unsafe { content.windows() };
    for window in windows.iter() {
        let is_on_screen = unsafe { window.isOnScreen() };
        let layer = unsafe { window.windowLayer() };
        if !is_on_screen || layer != 0 {
            continue;
        }
        let owner_pid = unsafe { window.owningApplication() }
            .map(|app| unsafe { app.processID() })
            .unwrap_or(0);
        if owner_pid == own_process_id {
            continue;
        }
        let frame = unsafe { window.frame() };
        let min_x = frame.origin.x.max(display_bounds.origin.x);
        let min_y = frame.origin.y.max(display_bounds.origin.y);
        let max_x = (frame.origin.x + frame.size.width)
            .min(display_bounds.origin.x + display_bounds.size.width);
        let max_y = (frame.origin.y + frame.size.height)
            .min(display_bounds.origin.y + display_bounds.size.height);
        let width = max_x - min_x;
        let height = max_y - min_y;
        if width < 8.0 || height < 8.0 {
            continue;
        }
        // SCWindow.frame and CGDisplayBounds share the same coordinate space
        // (top-left origin, y down — matching CGWindow, not NSScreen). The
        // Swift original (CaptureCoordinator.swift) translates by the display
        // origin WITHOUT flipping y, and the overlay's flipped view consumes
        // these rects directly. Any y-flip here mirrors the hover/selection
        // rectangles vertically.
        rects.push(Rect::new(
            min_x - display_bounds.origin.x,
            min_y - display_bounds.origin.y,
            width,
            height,
        ));
    }
    rects
}

fn cgimage_to_png(image: &CFRetained<CGImage>) -> Result<(Vec<u8>, i64, i64)> {
    let image_ref: &CGImage = image;
    let rep = NSBitmapImageRep::initWithCGImage(NSBitmapImageRep::alloc(), image_ref);
    let width = rep.pixelsWide() as i64;
    let height = rep.pixelsHigh() as i64;
    let data = unsafe {
        rep.representationUsingType_properties(
            objc2_app_kit::NSBitmapImageFileType::PNG,
            &NSDictionary::new(),
        )
    }
    .ok_or_else(|| anyhow!("could not encode capture as PNG"))?;
    let mut bytes = vec![0u8; data.len()];
    unsafe {
        data.getBytes_length(
            std::ptr::NonNull::new_unchecked(bytes.as_mut_ptr() as *mut std::ffi::c_void),
            data.len(),
        );
    }
    Ok((bytes, width, height))
}

pub fn capture_active_display() -> Result<CapturedDisplay> {
    // Debug/testing fixture (mirrors the Swift original's KIRI_CAPTURE_FIXTURE):
    // synthesizes a frozen screen so the full capture flow can be exercised
    // without Screen Recording permission.
    if std::env::var("KIRI_CAPTURE_FIXTURE").as_deref() == Ok("1") {
        return make_fixture();
    }
    match check_capture_permission() {
        PermissionState::Authorized => {}
        PermissionState::RestartRequired => {
            bail!("Screen Recording access was granted. Quit and reopen Kiri once to finish enabling capture.")
        }
        PermissionState::SettingsRequired => {
            bail!("Screen Recording is off. Enable Kiri in System Settings, then quit and reopen it once.")
        }
    }

    let screen = active_screen()?;
    let content = shareable_content()?;
    let displays = unsafe { content.displays() };
    let display = displays
        .iter()
        .find(|display| unsafe { display.displayID() } == screen.display_id)
        .ok_or_else(|| anyhow!("the active display could not be captured"))?;

    let window_rects = collect_window_rects(&content, screen.display_id);
    let own_process_id = std::process::id() as i32;
    let filter = make_filter(&content, &display, own_process_id, &[]);

    let configuration = unsafe {
        let configuration = SCStreamConfiguration::new();
        let width = ((display.width() as f64) * screen.backing_scale)
            .round()
            .max(1.0) as i64;
        let height = ((display.height() as f64) * screen.backing_scale)
            .round()
            .max(1.0) as i64;
        configuration.setWidth(width as usize);
        configuration.setHeight(height as usize);
        configuration.setShowsCursor(false);
        configuration
    };

    // CGImage is a CoreFoundation object: retain with CF semantics (the
    // handler delivers a borrowed reference).
    let (tx, rx) = mpsc::channel::<std::result::Result<CFRetained<CGImage>, String>>();
    let block = RcBlock::new(move |image: *mut CGImage, error: *mut NSError| {
        let result = if error.is_null() && !image.is_null() {
            Ok(unsafe { CFRetained::retain(std::ptr::NonNull::new_unchecked(image)) })
        } else if !error.is_null() {
            let error = unsafe { Retained::retain(error).unwrap() };
            Err(error.localizedDescription().to_string())
        } else {
            Err("screenshot failed".to_string())
        };
        let _ = tx.send(result);
    });
    let block_ref: &block2::Block<dyn Fn(*mut CGImage, *mut NSError)> = &block;
    unsafe {
        SCScreenshotManager::captureImageWithFilter_configuration_completionHandler(
            &filter,
            &configuration,
            Some(block_ref),
        );
    }
    let image = rx
        .recv()
        .map_err(|_| anyhow!("screenshot callback dropped"))?
        .map_err(|message| anyhow!(message))?;

    let (png_data, pixel_width, pixel_height) = cgimage_to_png(&image)?;
    if let Some((w, h, avg)) = png_average(&png_data) {
        log::info!("capture_active_display: frozen {w}x{h} avg-brightness={avg:.1}");
    }

    Ok(CapturedDisplay {
        png_data,
        pixel_width,
        pixel_height,
        screen_frame: screen.frame,
        window_rects,
        display_id: screen.display_id,
        backing_scale: screen.backing_scale,
    })
}

// ---------------------------------------------------------------------------
// Recording (RegionRecorder legacy-backend equivalent)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
pub struct AudioFormat {
    pub channels: u32,
    pub is_float: bool,
    pub is_non_interleaved: bool,
}

struct DelegateState {
    video_tx: mpsc::Sender<Vec<u8>>,
    audio_tx: Option<mpsc::Sender<Vec<u8>>>,
    mic_tx: Option<mpsc::Sender<Vec<u8>>>,
    audio_format: OnceLock<AudioFormat>,
    mic_format: OnceLock<AudioFormat>,
    /// Set before the stream stops so late frame callbacks (already queued
    /// on the SCK queue) become no-ops instead of sending into a channel
    /// whose receiver is gone.
    stopped: std::sync::atomic::AtomicBool,
    /// Total frames forwarded to the encoder (debug counter).
    frames: std::sync::atomic::AtomicU64,
}

/// A recording session running the SCK stream on a dedicated thread.
/// SCStream is `AnyThread` in objc2 (not Send/Sync), so the stream never
/// leaves the thread that created it; control flows through channels.
pub struct MacRecordingSession {
    stop_tx: mpsc::Sender<()>,
    /// Receives the stream-thread's completion notification after a stop.
    stop_done_rx: mpsc::Receiver<()>,
    _thread: std::thread::JoinHandle<()>,
}

impl MacRecordingSession {
    #[allow(clippy::too_many_arguments)]
    pub fn start(
        display_id: u32,
        region: Rect,
        backing_scale: f64,
        options: RecordingOptions,
        excepted_window_ids: &[u32],
        video_tx: mpsc::Sender<Vec<u8>>,
        audio_tx: Option<mpsc::Sender<Vec<u8>>>,
        mic_tx: Option<mpsc::Sender<Vec<u8>>>,
    ) -> Result<MacRecordingSession> {
        if region.width < 2.0 || region.height < 2.0 {
            bail!("The recording region is too small.");
        }

        // SCShareableContent is MainThreadOnly: resolve it on the main
        // thread. The recorder may be started from a background thread (the
        // UI must stay responsive), so hop to the main thread via the
        // process-wide dispatch main queue when needed.
        let content = shareable_content()?;
        let displays = unsafe { content.displays() };
        let display = displays
            .iter()
            .find(|display| unsafe { display.displayID() } == display_id)
            .ok_or_else(|| anyhow!("The selected display is no longer available."))?;
        let own_process_id = std::process::id() as i32;
        let filter = make_filter(&content, &display, own_process_id, excepted_window_ids);
        log::info!("MacRecordingSession: filter created");

        let width =
            crate::core::policy::RecordingPolicy::pixel_dimension(region.width, backing_scale);
        let height =
            crate::core::policy::RecordingPolicy::pixel_dimension(region.height, backing_scale);

        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let (result_tx, result_rx) = mpsc::channel::<Result<()>>();
        let (stop_done_tx, stop_done_rx) = mpsc::channel::<()>();
        // Transfer ownership across the thread boundary via a raw pointer;
        // the recorder thread owns the filter exclusively afterwards.
        let filter_ptr = Retained::into_raw(filter) as usize;

        log::info!("MacRecordingSession: spawning stream thread");
        let thread = std::thread::spawn(move || {
            log::info!("MacRecordingSession: stream thread running");
            let filter = unsafe { Retained::from_raw(filter_ptr as *mut SCContentFilter).unwrap() };
            let configuration = unsafe {
                let configuration = SCStreamConfiguration::new();
                // SCK sourceRect is display-local, top-left origin, in points.
                configuration.setSourceRect(CGRect {
                    origin: CGPoint {
                        x: region.x,
                        y: region.y,
                    },
                    size: CGSize {
                        width: region.width,
                        height: region.height,
                    },
                });
                configuration.setWidth(width as usize);
                configuration.setHeight(height as usize);
                configuration.setMinimumFrameInterval(CMTime {
                    value: 1,
                    timescale: crate::core::policy::RecordingPolicy::FRAMES_PER_SECOND as i32,
                    flags: objc2_core_media::CMTimeFlags(1),
                    epoch: 0,
                });
                configuration.setQueueDepth(6);
                configuration.setPixelFormat(0x4247_5241); // kCVPixelFormatType_32BGRA
                configuration
                    .setCaptureResolution(objc2_screen_capture_kit::SCCaptureResolutionType::Best);
                configuration.setScalesToFit(false);
                configuration.setShowsCursor(options.shows_cursor);
                configuration.setShowMouseClicks(false);
                configuration.setCapturesAudio(options.captures_system_audio);
                configuration.setExcludesCurrentProcessAudio(true);
                configuration.setSampleRate(48_000);
                configuration.setChannelCount(2);
                configuration
            };

            let state = DelegateState {
                video_tx,
                audio_tx,
                mic_tx,
                audio_format: OnceLock::new(),
                mic_format: OnceLock::new(),
                stopped: std::sync::atomic::AtomicBool::new(false),
                frames: std::sync::atomic::AtomicU64::new(0),
            };
            let state_ptr = Box::into_raw(Box::new(state)) as usize;

            let delegate = KiriStreamDelegate::with_state(state_ptr);
            let queue = dispatch2::DispatchQueue::new("io.yuxino.kiri.stream", None);

            let delegate_ref: &KiriStreamDelegate = &delegate;
            let stream = unsafe {
                SCStream::initWithFilter_configuration_delegate(
                    SCStream::alloc(),
                    &filter,
                    &configuration,
                    Some(ProtocolObject::from_ref(delegate_ref)),
                )
            };
            let output: &ProtocolObject<dyn SCStreamOutput> =
                ProtocolObject::from_ref(delegate_ref);
            if let Err(error) = unsafe {
                stream.addStreamOutput_type_sampleHandlerQueue_error(
                    output,
                    SCStreamOutputType::Screen,
                    Some(&queue),
                )
            } {
                let _ = result_tx.send(Err(anyhow!("{error:?}")));
                return;
            }
            if options.captures_system_audio {
                if let Err(error) = unsafe {
                    stream.addStreamOutput_type_sampleHandlerQueue_error(
                        output,
                        SCStreamOutputType::Audio,
                        Some(&queue),
                    )
                } {
                    let _ = result_tx.send(Err(anyhow!("{error:?}")));
                    return;
                }
            }
            if options.captures_microphone {
                if let Err(error) = unsafe {
                    stream.addStreamOutput_type_sampleHandlerQueue_error(
                        output,
                        SCStreamOutputType::Microphone,
                        Some(&queue),
                    )
                } {
                    let _ = result_tx.send(Err(anyhow!("{error:?}")));
                    return;
                }
            }

            log::info!("MacRecordingSession: starting capture…");
            if let Err(error) = start_capture_sync(&stream) {
                log::error!("MacRecordingSession: start_capture_sync failed: {error}");
                let _ = result_tx.send(Err(error));
                return;
            }
            log::info!("MacRecordingSession: capture started");
            let _ = result_tx.send(Ok(()));

            log::info!("MacRecordingSession: stream thread got stop signal");
            let _ = stop_rx.recv();
            unsafe {
                let state = &mut *(state_ptr as *mut DelegateState);
                state
                    .stopped
                    .store(true, std::sync::atomic::Ordering::Relaxed);
                log::info!(
                    "MacRecordingSession: stopping after {} frames",
                    state.frames.load(std::sync::atomic::Ordering::Relaxed)
                );
            }
            // Do NOT call stopCaptureWithCompletionHandler here: it can block
            // (and its completion may be dispatched onto the sample-handler
            // queue, deadlocking this thread). Dropping the stream and the
            // senders (below) signals EOF to ffmpeg and lets SCK tear the
            // stream down on its own.
            log::info!("MacRecordingSession: stopping capture (drop)…");
            // Give SCK a moment to drain any frame callbacks already queued
            // on the stream queue before we free the delegate state; freeing
            // it first lets a late callback touch freed memory (SIGSEGV in
            // stream_didOutputSampleBuffer → Sender::send).
            std::thread::sleep(std::time::Duration::from_millis(300));
            // Delegate (and frame senders) drop here → EOF for ffmpeg.
            unsafe {
                let _ = Box::from_raw(state_ptr as *mut DelegateState);
            }
            // `stream` drops at the end of this scope, releasing the SCK
            // stream (which stops capture). Only after that notify the stop
            // waiter — otherwise the stream would still be capturing when
            // the caller believes recording has stopped.
            drop(stream);
            // Notify the stop waiter that the stream thread has finished
            // tearing down (senders dropped → ffmpeg EOF, stream released).
            let _ = stop_done_tx.send(());
        });

        log::info!("MacRecordingSession: waiting for stream thread result…");
        result_rx
            .recv()
            .map_err(|_| anyhow!("recorder thread exited early"))??;
        log::info!("MacRecordingSession: stream thread ready");
        Ok(MacRecordingSession {
            stop_tx,
            stop_done_rx,
            _thread: thread,
        })
    }

    fn request_stop(&self) -> Result<()> {
        log::info!("MacRecordingSession: request_stop sending…");
        let _ = self.stop_tx.send(());
        // The result_rx is a one-shot used at start; the stop completion is
        // delivered on stop_done_rx. Waiting on result_rx here would
        // immediately error ("recorder thread exited early") because its
        // sender was already dropped after the start handshake.
        self.stop_done_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .map_err(|_| anyhow!("recorder stop timed out"))?;
        log::info!("MacRecordingSession: request_stop done");
        Ok(())
    }
}

impl super::PlatformRecorder for MacRecordingSession {
    fn stop(&mut self) -> Result<()> {
        self.request_stop()
    }
}

fn start_capture_sync(stream: &SCStream) -> Result<()> {
    let (tx, rx) = mpsc::channel::<std::result::Result<(), String>>();
    let block = RcBlock::new(move |error: *mut NSError| {
        let result = if error.is_null() {
            Ok(())
        } else {
            let error = unsafe { Retained::retain(error).unwrap() };
            Err(error.localizedDescription().to_string())
        };
        let _ = tx.send(result);
    });
    let block_ref: &block2::Block<dyn Fn(*mut NSError)> = &block;
    unsafe { stream.startCaptureWithCompletionHandler(Some(block_ref)) };
    rx.recv()
        .map_err(|_| anyhow!("start capture callback dropped"))?
        .map_err(|message| anyhow!(message))
}

// ---------------------------------------------------------------------------
// Stream delegate
// ---------------------------------------------------------------------------

fn audio_format_from(buffer: &CMSampleBuffer) -> Option<AudioFormat> {
    let desc: CFRetained<CMFormatDescription> = unsafe { buffer.format_description() }?;
    let asbd = unsafe { CMAudioFormatDescriptionGetStreamBasicDescription(&desc) };
    if asbd.is_null() {
        return None;
    }
    let asbd = unsafe { &*asbd };
    const IS_FLOAT: u32 = 1 << 0;
    const IS_NON_INTERLEAVED: u32 = 1 << 5;
    Some(AudioFormat {
        channels: asbd.mChannelsPerFrame,
        is_float: asbd.mFormatFlags & IS_FLOAT != 0,
        is_non_interleaved: asbd.mFormatFlags & IS_NON_INTERLEAVED != 0,
    })
}

fn copy_pixel_buffer(buffer: &CMSampleBuffer) -> Option<Vec<u8>> {
    let pixel_buffer: CFRetained<CVPixelBuffer> = unsafe { buffer.image_buffer() }?;
    let pixel_buffer_ref: &CVPixelBuffer = &pixel_buffer;
    unsafe { CVPixelBufferLockBaseAddress(pixel_buffer_ref, CVPixelBufferLockFlags::ReadOnly) };
    let result = unsafe {
        let base = CVPixelBufferGetBaseAddress(pixel_buffer_ref);
        if base.is_null() {
            CVPixelBufferUnlockBaseAddress(pixel_buffer_ref, CVPixelBufferLockFlags::ReadOnly);
            return None;
        }
        let bytes_per_row = CVPixelBufferGetBytesPerRow(pixel_buffer_ref);
        let height = CVPixelBufferGetHeight(pixel_buffer_ref);
        let row_bytes = CVPixelBufferGetWidth(pixel_buffer_ref) * 4;
        let mut out = Vec::with_capacity(row_bytes * height);
        for row in 0..height {
            let src = base.add(row * bytes_per_row) as *const u8;
            out.extend_from_slice(std::slice::from_raw_parts(src, row_bytes));
        }
        CVPixelBufferUnlockBaseAddress(pixel_buffer_ref, CVPixelBufferLockFlags::ReadOnly);
        out
    };
    Some(result)
}

fn copy_audio_buffer(buffer: &CMSampleBuffer) -> Option<Vec<u8>> {
    let block: CFRetained<CMBlockBuffer> = unsafe { buffer.data_buffer() }?;
    let mut data_ptr: *mut std::ffi::c_char = std::ptr::null_mut();
    let mut length = 0usize;
    let mut total = 0usize;
    let status = unsafe { block.data_pointer(0, &mut length, &mut total, &mut data_ptr) };
    if status != 0 || data_ptr.is_null() || length == 0 {
        return None;
    }
    Some(unsafe { std::slice::from_raw_parts(data_ptr as *const u8, length) }.to_vec())
}

fn deinterleave_f32(planar: &[u8], channels: u32, _format: AudioFormat) -> Vec<u8> {
    let frames = planar.len() / (4 * channels as usize);
    let mut interleaved = Vec::with_capacity(planar.len());
    for frame in 0..frames {
        for channel in 0..channels as usize {
            let offset = (channel * frames + frame) * 4;
            interleaved.extend_from_slice(&planar[offset..offset + 4]);
        }
    }
    interleaved
}

pub struct KiriStreamDelegateIvars {
    state: usize,
}

objc2::define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = AnyThread]
    #[name = "KiriStreamDelegate"]
    #[ivars = KiriStreamDelegateIvars]
    pub struct KiriStreamDelegate;

    unsafe impl NSObjectProtocol for KiriStreamDelegate {}

    unsafe impl SCStreamOutput for KiriStreamDelegate {
        #[unsafe(method(stream:didOutputSampleBuffer:ofType:))]
        fn stream_didOutputSampleBuffer_ofType(
            &self,
            _stream: &SCStream,
            buffer: &CMSampleBuffer,
            of_type: SCStreamOutputType,
        ) {
            if !unsafe { buffer.data_is_ready() } {
                return;
            }
            let state = unsafe { &*(self.ivars().state as *const DelegateState) };
            // Late callback after stop: ignore to avoid use-after-free of
            // the senders (the receiver is dropped once the stream stops).
            if state.stopped.load(std::sync::atomic::Ordering::Relaxed) {
                return;
            }
            if of_type == SCStreamOutputType::Screen {
                if let Some(frame) = copy_pixel_buffer(buffer) {
                    let count = state
                        .frames
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                        + 1;
                    if count % 30 == 0 {
                        log::info!("MacRecordingSession: {count} frames captured");
                    }
                    let _ = state.video_tx.send(frame);
                }
            } else if of_type == SCStreamOutputType::Microphone {
                if let Some(tx) = &state.mic_tx {
                    let format = state.mic_format.get_or_init(|| {
                        audio_format_from(buffer).unwrap_or(AudioFormat {
                            channels: 2,
                            is_float: true,
                            is_non_interleaved: false,
                        })
                    });
                    if let Some(bytes) = copy_audio_buffer(buffer) {
                        let payload = if format.is_float && format.is_non_interleaved {
                            deinterleave_f32(&bytes, format.channels, *format)
                        } else {
                            bytes
                        };
                        let _ = tx.send(payload);
                    }
                }
            } else if let Some(tx) = &state.audio_tx {
                let format = state.audio_format.get_or_init(|| {
                    audio_format_from(buffer).unwrap_or(AudioFormat {
                        channels: 2,
                        is_float: true,
                        is_non_interleaved: false,
                    })
                });
                if let Some(bytes) = copy_audio_buffer(buffer) {
                    log::info!(
                        "MacRecordingSession: audio {} bytes, format ch={} float={} non_interleaved={}",
                        bytes.len(),
                        format.channels,
                        format.is_float,
                        format.is_non_interleaved,
                    );
                    let payload = if format.is_float && format.is_non_interleaved {
                        deinterleave_f32(&bytes, format.channels, *format)
                    } else {
                        bytes
                    };
                    let _ = tx.send(payload);
                }
            }
        }
    }

    unsafe impl SCStreamDelegate for KiriStreamDelegate {
        #[unsafe(method(stream:didStopWithError:))]
        fn stream_didStopWithError(&self, _stream: &SCStream, _error: &NSError) {
            // Stop is initiated by us; failures surface through stop().
        }
    }
);

impl KiriStreamDelegate {
    pub fn with_state(state_ptr: usize) -> Retained<Self> {
        let this =
            KiriStreamDelegate::alloc().set_ivars(KiriStreamDelegateIvars { state: state_ptr });
        unsafe { msg_send![super(this), init] }
    }
}
