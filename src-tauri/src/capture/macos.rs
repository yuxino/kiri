//! macOS capture backend — ScreenCaptureKit via objc2, mirroring
//! CaptureCoordinator.swift / RegionRecorder.swift (legacy backend).

use std::sync::mpsc;
use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::Weak;

use anyhow::{anyhow, bail, Result};
use block2::RcBlock;
use objc2::msg_send;
use objc2::rc::Retained;
use objc2::runtime::{NSObjectProtocol, ProtocolObject};
use objc2::{AnyThread, DefinedClass, MainThreadMarker};
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

fn shareable_content(_main_thread: MainThreadMarker) -> Result<Retained<SCShareableContent>> {
    // The marker makes it impossible for call sites to initiate this
    // MainThreadOnly request from a Tokio worker.
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

/// Sendable ownership wrapper for a retained SCContentFilter pointer. If a
/// timed-out receiver drops a queued completion, Drop still balances the
/// native retain instead of leaking it at the timeout boundary.
struct RecordingFilterPointer(Option<usize>);

impl RecordingFilterPointer {
    fn into_raw(mut self) -> usize {
        self.0
            .take()
            .expect("recording filter pointer consumed once")
    }
}

impl Drop for RecordingFilterPointer {
    fn drop(&mut self) {
        if let Some(pointer) = self.0.take() {
            unsafe {
                drop(Retained::from_raw(pointer as *mut SCContentFilter));
            }
        }
    }
}

fn recording_filter_pointer(
    display_id: u32,
    own_process_id: i32,
    excepted_window_ids: Vec<u32>,
) -> Result<usize> {
    if MainThreadMarker::new().is_some() {
        bail!("recording filter preparation must not block the main thread");
    }

    let (tx, rx) = mpsc::channel::<std::result::Result<RecordingFilterPointer, String>>();
    // Only registration happens synchronously on main. Waiting for SCK stays
    // on the async command's worker thread below.
    dispatch2::run_on_main(move |_main_thread| {
        let block = RcBlock::new(
            move |content: *mut SCShareableContent, error: *mut NSError| {
                if !error.is_null() {
                    let error = unsafe { Retained::retain(error).unwrap() };
                    let _ = tx.send(Err(error.localizedDescription().to_string()));
                    return;
                }
                if content.is_null() {
                    let _ = tx.send(Err("shareable content failed".to_string()));
                    return;
                }

                // Retain on the callback queue, then consume and inspect the
                // MainThreadOnly object inside a main-thread closure.
                let content = unsafe { Retained::retain(content).unwrap() };
                let content_pointer = Retained::into_raw(content) as usize;
                let completion_tx = tx.clone();
                let excepted_window_ids = excepted_window_ids.clone();
                dispatch2::run_on_main(move |_main_thread| {
                    let result = (|| -> std::result::Result<RecordingFilterPointer, String> {
                        let content = unsafe {
                            Retained::from_raw(content_pointer as *mut SCShareableContent)
                        }
                        .ok_or_else(|| {
                            "shareable content callback returned an invalid object".to_string()
                        })?;
                        let displays = unsafe { content.displays() };
                        let display = displays
                            .iter()
                            .find(|display| unsafe { display.displayID() } == display_id)
                            .ok_or_else(|| {
                                "The selected display is no longer available.".to_string()
                            })?;
                        let filter =
                            make_filter(&content, &display, own_process_id, &excepted_window_ids);
                        Ok(RecordingFilterPointer(Some(
                            Retained::into_raw(filter) as usize
                        )))
                    })();
                    // SendError owns and drops a successful pointer guard, so
                    // a completion racing a timeout cannot leak the filter.
                    let _ = completion_tx.send(result);
                });
            },
        );
        unsafe {
            SCShareableContent::getShareableContentExcludingDesktopWindows_onScreenWindowsOnly_completionHandler(
                false,
                true,
                &block,
            );
        }
    });

    let pointer = rx
        .recv_timeout(std::time::Duration::from_secs(8))
        .map_err(|_| anyhow!("shareable content timed out after 8s"))?
        .map_err(|message| anyhow!(message))?;
    Ok(pointer.into_raw())
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
        // SCWindow.frame and CGDisplayBounds share the same top-left, y-down
        // coordinate space. Translate by the display origin without flipping
        // y; a flip here would mirror hover and selection rectangles.
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
    match check_capture_permission() {
        PermissionState::Authorized => {}
        PermissionState::RestartRequired => {
            bail!("Screen Recording access was granted. Quit and reopen Kiri once to finish enabling capture.")
        }
        PermissionState::SettingsRequired => {
            bail!("Screen Recording is off. Enable Kiri in System Settings, then quit and reopen it once.")
        }
    }

    let main_thread = MainThreadMarker::new()
        .ok_or_else(|| anyhow!("display capture must start on the main thread"))?;
    let screen = active_screen()?;
    let content = shareable_content(main_thread)?;
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

struct DelegateSenders {
    video_tx: mpsc::Sender<Vec<u8>>,
    audio_tx: Option<mpsc::Sender<Vec<u8>>>,
    mic_tx: Option<mpsc::Sender<Vec<u8>>>,
}

struct DelegateState {
    senders: Weak<DelegateSenders>,
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
    stop_done_rx: mpsc::Receiver<std::result::Result<(), String>>,
    thread: Option<std::thread::JoinHandle<()>>,
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
        if options.captures_microphone && !crate::platform::mic_supported() {
            bail!("Microphone recording requires macOS 15 or later.");
        }

        // SCShareableContent and the objects obtained from it are
        // MainThreadOnly. Build the filter on the main thread, then transfer
        // its retained ownership to the dedicated stream thread as a raw
        // pointer. No SCK content object is inspected on the Tokio worker.
        let own_process_id = std::process::id() as i32;
        let filter_ptr =
            recording_filter_pointer(display_id, own_process_id, excepted_window_ids.to_vec())?;
        log::info!("MacRecordingSession: filter created");

        let width =
            crate::core::policy::RecordingPolicy::pixel_dimension(region.width, backing_scale);
        let height =
            crate::core::policy::RecordingPolicy::pixel_dimension(region.height, backing_scale);

        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let (result_tx, result_rx) = mpsc::channel::<Result<()>>();
        let (stop_done_tx, stop_done_rx) = mpsc::channel::<std::result::Result<(), String>>();
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
                if options.captures_microphone {
                    // This selector is available on macOS 15+. The backend
                    // rejects microphone capture on older systems before the
                    // stream thread starts, so it is never sent there.
                    configuration.setCaptureMicrophone(true);
                }
                configuration.setExcludesCurrentProcessAudio(true);
                configuration.setSampleRate(48_000);
                configuration.setChannelCount(2);
                configuration
            };

            let senders = Arc::new(DelegateSenders {
                video_tx,
                audio_tx,
                mic_tx,
            });
            let state = Arc::new(DelegateState {
                senders: Arc::downgrade(&senders),
                audio_format: OnceLock::new(),
                mic_format: OnceLock::new(),
                stopped: std::sync::atomic::AtomicBool::new(false),
                frames: std::sync::atomic::AtomicU64::new(0),
            });

            let delegate = KiriStreamDelegate::with_state(state);
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
            let mut registered_outputs = Vec::new();
            if let Err(error) = unsafe {
                stream.addStreamOutput_type_sampleHandlerQueue_error(
                    output,
                    SCStreamOutputType::Screen,
                    Some(&queue),
                )
            } {
                let _ = remove_stream_outputs(&stream, output, &queue, &registered_outputs);
                let _ = result_tx.send(Err(anyhow!("{error:?}")));
                return;
            }
            registered_outputs.push(SCStreamOutputType::Screen);
            if options.captures_system_audio {
                if let Err(error) = unsafe {
                    stream.addStreamOutput_type_sampleHandlerQueue_error(
                        output,
                        SCStreamOutputType::Audio,
                        Some(&queue),
                    )
                } {
                    let _ = remove_stream_outputs(&stream, output, &queue, &registered_outputs);
                    let _ = result_tx.send(Err(anyhow!("{error:?}")));
                    return;
                }
                registered_outputs.push(SCStreamOutputType::Audio);
            }
            if options.captures_microphone {
                if let Err(error) = unsafe {
                    stream.addStreamOutput_type_sampleHandlerQueue_error(
                        output,
                        SCStreamOutputType::Microphone,
                        Some(&queue),
                    )
                } {
                    let _ = remove_stream_outputs(&stream, output, &queue, &registered_outputs);
                    let _ = result_tx.send(Err(anyhow!("{error:?}")));
                    return;
                }
                registered_outputs.push(SCStreamOutputType::Microphone);
            }

            log::info!("MacRecordingSession: starting capture…");
            if let Err(error) = start_capture_sync(&stream) {
                log::error!("MacRecordingSession: start_capture_sync failed: {error}");
                let _ = remove_stream_outputs(&stream, output, &queue, &registered_outputs);
                let _ = result_tx.send(Err(error));
                return;
            }
            log::info!("MacRecordingSession: capture started");
            let _ = result_tx.send(Ok(()));

            log::info!("MacRecordingSession: stream thread got stop signal");
            let _ = stop_rx.recv();
            let state = &delegate.ivars().state;
            state
                .stopped
                .store(true, std::sync::atomic::Ordering::Release);
            log::info!(
                "MacRecordingSession: stopping after {} frames",
                state.frames.load(std::sync::atomic::Ordering::Relaxed)
            );
            // Removing outputs prevents new sample delivery; the empty
            // synchronous task is then a FIFO drain barrier on the serial
            // sample queue. Do not call stopCaptureWithCompletionHandler
            // here: on affected macOS versions that call can block before a
            // completion is scheduled (the reason Kiri previously removed
            // it in the recording-stop deadlock fix).
            let stop_result = remove_stream_outputs(&stream, output, &queue, &registered_outputs);
            // The delegate intentionally holds only a Weak reference. Queue
            // drain guarantees no callback owns a temporary strong Arc, so
            // this closes every frame channel before stop_done and gives
            // ffmpeg deterministic EOF even if SCK retains the delegate.
            drop(senders);
            log::info!("MacRecordingSession: capture stopped; releasing stream…");
            // DelegateState is owned by the Objective-C delegate through an
            // Arc. Any callback already retained by SCK therefore keeps the
            // state alive as a final safety net; the queue drain above is the
            // synchronization that lets normal shutdown release it now.
            drop(stream);
            drop(delegate);
            drop(queue);
            // Notify the stop waiter that the stream thread has finished
            // releasing stream/delegate ownership and all frame senders have
            // closed, so ffmpeg has observed EOF.
            let _ = stop_done_tx.send(stop_result.map_err(|error| error.to_string()));
        });

        log::info!("MacRecordingSession: waiting for stream thread result…");
        result_rx
            .recv()
            .map_err(|_| anyhow!("recorder thread exited early"))??;
        log::info!("MacRecordingSession: stream thread ready");
        Ok(MacRecordingSession {
            stop_tx,
            stop_done_rx,
            thread: Some(thread),
        })
    }

    fn request_stop(&mut self) -> Result<()> {
        if self.thread.is_none() {
            return Ok(());
        }
        log::info!("MacRecordingSession: request_stop sending…");
        let _ = self.stop_tx.send(());
        // The result_rx is a one-shot used at start; the stop completion is
        // delivered on stop_done_rx. Waiting on result_rx here would
        // immediately error ("recorder thread exited early") because its
        // sender was already dropped after the start handshake.
        let stop_result = match self
            .stop_done_rx
            .recv_timeout(std::time::Duration::from_secs(8))
        {
            Ok(result) => result.map_err(|error| anyhow!(error)),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // The thread still owns its stream, delegate, and Arc state.
                // Detach rather than defeating the timeout with an unbounded
                // join; late cleanup remains memory-safe.
                let _ = self.thread.take();
                return Err(anyhow!("recorder stop timed out"));
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                Err(anyhow!("recorder thread exited during shutdown"))
            }
        };
        let join_result = if let Some(thread) = self.thread.take() {
            thread
                .join()
                .map_err(|_| anyhow!("recorder thread panicked during shutdown"))
        } else {
            Ok(())
        };
        // Always join, even when native stop reported an error. Otherwise
        // Drop would retry a consumed one-shot and detach the cleanup thread.
        join_result?;
        stop_result?;
        log::info!("MacRecordingSession: request_stop done");
        Ok(())
    }
}

impl super::PlatformRecorder for MacRecordingSession {
    fn stop(&mut self) -> Result<()> {
        self.request_stop()
    }
}

impl Drop for MacRecordingSession {
    fn drop(&mut self) {
        let _ = self.request_stop();
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
    rx.recv_timeout(std::time::Duration::from_secs(8))
        .map_err(|_| anyhow!("start capture timed out"))?
        .map_err(|message| anyhow!(message))
}

fn remove_stream_outputs(
    stream: &SCStream,
    output: &ProtocolObject<dyn SCStreamOutput>,
    queue: &dispatch2::DispatchQueue,
    registered_outputs: &[SCStreamOutputType],
) -> Result<()> {
    let mut first_error = None;
    for output_type in registered_outputs.iter().rev().copied() {
        if let Err(error) = unsafe { stream.removeStreamOutput_type_error(output, output_type) } {
            first_error.get_or_insert_with(|| anyhow!("{error:?}"));
        }
    }
    // DispatchQueue::new creates a serial queue. A synchronous no-op from the
    // recorder thread waits for every previously submitted callback.
    queue.exec_sync(|| {});
    match first_error {
        Some(error) => Err(error),
        None => Ok(()),
    }
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
    state: Arc<DelegateState>,
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
            let state = &self.ivars().state;
            // Late callbacks retain the delegate and its Arc-owned state, but
            // do not forward more data once stop begins.
            if state.stopped.load(std::sync::atomic::Ordering::Acquire) {
                return;
            }
            let Some(senders) = state.senders.upgrade() else {
                return;
            };
            if of_type == SCStreamOutputType::Screen {
                if let Some(frame) = copy_pixel_buffer(buffer) {
                    let count = state
                        .frames
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                        + 1;
                    if count % 30 == 0 {
                        log::info!("MacRecordingSession: {count} frames captured");
                    }
                    let _ = senders.video_tx.send(frame);
                }
            } else if of_type == SCStreamOutputType::Microphone {
                if let Some(tx) = &senders.mic_tx {
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
            } else if let Some(tx) = &senders.audio_tx {
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
    fn with_state(state: Arc<DelegateState>) -> Retained<Self> {
        let this = KiriStreamDelegate::alloc().set_ivars(KiriStreamDelegateIvars { state });
        unsafe { msg_send![super(this), init] }
    }
}

#[cfg(test)]
mod recording_lifetime_tests {
    use super::*;

    #[test]
    fn native_delegate_owns_callback_state_and_frame_senders() {
        let (video_tx, video_rx) = mpsc::channel();
        let senders = Arc::new(DelegateSenders {
            video_tx,
            audio_tx: None,
            mic_tx: None,
        });
        let state = Arc::new(DelegateState {
            senders: Arc::downgrade(&senders),
            audio_format: OnceLock::new(),
            mic_format: OnceLock::new(),
            stopped: std::sync::atomic::AtomicBool::new(false),
            frames: std::sync::atomic::AtomicU64::new(0),
        });
        let weak_state = Arc::downgrade(&state);
        let delegate = KiriStreamDelegate::with_state(state.clone());

        drop(state);
        assert!(weak_state.upgrade().is_some());
        assert!(matches!(
            video_rx.try_recv(),
            Err(mpsc::TryRecvError::Empty)
        ));

        drop(senders);
        assert!(weak_state.upgrade().is_some());
        assert!(matches!(
            video_rx.try_recv(),
            Err(mpsc::TryRecvError::Disconnected)
        ));

        drop(delegate);
        assert!(weak_state.upgrade().is_none());
    }
}
