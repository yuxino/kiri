//! macOS capture backend — ScreenCaptureKit via objc2, mirroring
//! CaptureCoordinator.swift / RegionRecorder.swift (legacy backend).

use std::sync::mpsc;
use std::sync::OnceLock;

use anyhow::{anyhow, bail, Context, Result};
use block2::RcBlock;
use objc2::msg_send;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObjectProtocol, ProtocolObject};
use objc2::{AnyThread, DefinedClass};
use objc2_app_kit::{NSEvent, NSScreen, NSBitmapImageRep};
use objc2_core_foundation::{CFRetained, CGPoint, CGRect, CGSize};
use objc2_core_graphics::{CGDirectDisplayID, CGDisplayBounds, CGImage};
use objc2_core_media::{
    CMAudioFormatDescriptionGetStreamBasicDescription, CMBlockBuffer, CMFormatDescription,
    CMSampleBuffer, CMTime,
};
use objc2_core_video::{
    CVPixelBuffer, CVPixelBufferGetBaseAddress, CVPixelBufferGetBytesPerRow, CVPixelBufferGetHeight,
    CVPixelBufferGetWidth, CVPixelBufferLockBaseAddress, CVPixelBufferLockFlags,
    CVPixelBufferUnlockBaseAddress,
};
use objc2_foundation::{NSArray, NSData, NSDictionary, NSNumber, NSString, NSError, NSObject};
use objc2_screen_capture_kit::{
    SCContentFilter, SCDisplay, SCScreenshotConfiguration, SCScreenshotManager, SCShareableContent,
    SCStream, SCStreamConfiguration, SCStreamDelegate, SCStreamOutput, SCStreamOutputType,
    SCWindow,
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

/// Mirrors `ScreenCapturePermissionGate` (preflight → request).
pub fn check_capture_permission() -> PermissionState {
    if unsafe { CGPreflightScreenCaptureAccess() } {
        return PermissionState::Authorized;
    }
    if unsafe { CGRequestScreenCaptureAccess() } {
        return PermissionState::RestartRequired;
    }
    PermissionState::SettingsRequired
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

fn shareable_content() -> Result<Retained<SCShareableContent>> {
    let (tx, rx) = mpsc::channel::<std::result::Result<Retained<SCShareableContent>, String>>();
    let block = RcBlock::new(move |content: *mut SCShareableContent, error: *mut NSError| {
        let result = if error.is_null() && !content.is_null() {
            Ok(unsafe { Retained::from_raw(content).unwrap() })
        } else if !error.is_null() {
            let error = unsafe { Retained::from_raw(error).unwrap() };
            Err(error.localizedDescription().to_string())
        } else {
            Err("shareable content failed".to_string())
        };
        let _ = tx.send(result);
    });
    unsafe {
        SCShareableContent::getShareableContentExcludingDesktopWindows_onScreenWindowsOnly_completionHandler(
            false,
            true,
            &block,
        );
    }
    rx.recv()
        .map_err(|_| anyhow!("shareable content callback dropped"))?
        .map_err(|message| anyhow!(message))
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
                    unsafe { window.owningApplication() }
                        .map(|app| unsafe { app.processID() })
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
                    unsafe { window.owningApplication() }
                        .map(|app| unsafe { app.processID() })
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
    let display_bounds = unsafe { CGDisplayBounds(display_id) };
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
        let top_left_y = display_bounds.size.height - (min_y - display_bounds.origin.y + height);
        rects.push(Rect::new(
            min_x - display_bounds.origin.x,
            top_left_y,
            width,
            height,
        ));
    }
    rects
}

fn cgimage_to_png(image: &Retained<CGImage>) -> Result<(Vec<u8>, i64, i64)> {
    let rep = unsafe { NSBitmapImageRep::initWithCGImage(NSBitmapImageRep::alloc(), image) };
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
        data.getBytes_length(std::ptr::NonNull::new_unchecked(bytes.as_mut_ptr() as *mut std::ffi::c_void), data.len());
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

    let configuration = SCStreamConfiguration::new();
    let width = ((unsafe { display.width() } as f64) * screen.backing_scale).round().max(1.0) as i64;
    let height = ((unsafe { display.height() } as f64) * screen.backing_scale).round().max(1.0) as i64;
    configuration.setWidth(width as usize);
    configuration.setHeight(height as usize);
    configuration.setShowsCursor(false);

    let (tx, rx) = mpsc::channel::<std::result::Result<Retained<CGImage>, String>>();
    let block = RcBlock::new(move |image: *mut CGImage, error: *mut NSError| {
        let result = if error.is_null() && !image.is_null() {
            Ok(unsafe { Retained::from_raw(image).unwrap() })
        } else if !error.is_null() {
            let error = unsafe { Retained::from_raw(error).unwrap() };
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
    pub sample_rate: f64,
    pub channels: u32,
    pub bits_per_channel: u32,
    pub is_float: bool,
    pub is_non_interleaved: bool,
}

struct DelegateState {
    video_tx: mpsc::Sender<Vec<u8>>,
    audio_tx: Option<mpsc::Sender<Vec<u8>>>,
    video_size: OnceLock<(usize, usize)>,
    audio_format: OnceLock<AudioFormat>,
}

/// A recording session running the SCK stream on a dedicated thread.
/// SCStream is `AnyThread` in objc2 (not Send/Sync), so the stream never
/// leaves the thread that created it; control flows through channels.
pub struct MacRecordingSession {
    stop_tx: mpsc::Sender<()>,
    result_rx: mpsc::Receiver<Result<()>>,
    _thread: std::thread::JoinHandle<()>,
}

impl MacRecordingSession {
    pub fn start(
        display_id: u32,
        region: Rect,
        backing_scale: f64,
        options: RecordingOptions,
        excepted_window_ids: &[u32],
        video_tx: mpsc::Sender<Vec<u8>>,
        audio_tx: Option<mpsc::Sender<Vec<u8>>>,
    ) -> Result<MacRecordingSession> {
        if region.width < 2.0 || region.height < 2.0 {
            bail!("The recording region is too small.");
        }

        // SCShareableContent is MainThreadOnly: resolve it on the main thread.
        let content = shareable_content()?;
        let displays = unsafe { content.displays() };
        let display = displays
            .iter()
            .find(|display| unsafe { display.displayID() } == display_id)
            .ok_or_else(|| anyhow!("The selected display is no longer available."))?;
        let own_process_id = std::process::id() as i32;
        let filter = make_filter(&content, &display, own_process_id, excepted_window_ids);

        let width = crate::core::policy::RecordingPolicy::pixel_dimension(
            region.width,
            backing_scale,
        );
        let height = crate::core::policy::RecordingPolicy::pixel_dimension(
            region.height,
            backing_scale,
        );

        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let (result_tx, result_rx) = mpsc::channel::<Result<()>>();
        // Transfer ownership across the thread boundary via a raw pointer;
        // the recorder thread owns the filter exclusively afterwards.
        let filter_ptr = Retained::into_raw(filter);

        let thread = std::thread::spawn(move || {
            let filter = unsafe { Retained::from_raw(filter_ptr) };
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
            configuration.setCaptureResolution(objc2_screen_capture_kit::SCCaptureResolutionType::Best);
            configuration.setScalesToFit(false);
            configuration.setShowsCursor(options.shows_cursor);
            configuration.setShowMouseClicks(false);
            configuration.setCapturesAudio(options.captures_system_audio);
            configuration.setExcludesCurrentProcessAudio(true);
            configuration.setSampleRate(48_000);
            configuration.setChannelCount(2);

            let state = DelegateState {
                video_tx,
                audio_tx,
                video_size: OnceLock::new(),
                audio_format: OnceLock::new(),
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

            if let Err(error) = start_capture_sync(&stream) {
                let _ = result_tx.send(Err(error));
                return;
            }
            let _ = result_tx.send(Ok(()));

            let _ = stop_rx.recv();
            let _ = stop_capture_sync(&stream);
            // Delegate (and frame senders) drop here → EOF for ffmpeg.
            unsafe {
                let _ = Box::from_raw(state_ptr as *mut DelegateState);
            }
        });

        result_rx
            .recv()
            .map_err(|_| anyhow!("recorder thread exited early"))??;
        Ok(MacRecordingSession {
            stop_tx,
            result_rx,
            _thread: thread,
        })
    }

    fn request_stop(&self) -> Result<()> {
        let _ = self.stop_tx.send(());
        self.result_rx
            .recv()
            .map_err(|_| anyhow!("recorder thread exited early"))?
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
    stream.startCaptureWithCompletionHandler(Some(block_ref));
    rx.recv()
        .map_err(|_| anyhow!("start capture callback dropped"))?
        .map_err(|message| anyhow!(message))
}

fn stop_capture_sync(stream: &SCStream) -> Result<()> {
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
    stream.stopCaptureWithCompletionHandler(Some(block_ref));
    rx.recv()
        .map_err(|_| anyhow!("stop capture callback dropped"))?
        .map_err(|message| anyhow!(message))
}

// ---------------------------------------------------------------------------
// Stream delegate
// ---------------------------------------------------------------------------

fn audio_format_from(buffer: &CMSampleBuffer) -> Option<AudioFormat> {
    let desc: CFRetained<CMFormatDescription> = buffer.format_description()?;
    let asbd = unsafe { CMAudioFormatDescriptionGetStreamBasicDescription(&desc) };
    if asbd.is_null() {
        return None;
    }
    let asbd = unsafe { &*asbd };
    const IS_FLOAT: u32 = 1 << 0;
    const IS_NON_INTERLEAVED: u32 = 1 << 5;
    Some(AudioFormat {
        sample_rate: asbd.mSampleRate,
        channels: asbd.mChannelsPerFrame,
        bits_per_channel: asbd.mBitsPerChannel,
        is_float: asbd.mFormatFlags & IS_FLOAT != 0,
        is_non_interleaved: asbd.mFormatFlags & IS_NON_INTERLEAVED != 0,
    })
}

fn copy_pixel_buffer(buffer: &CMSampleBuffer) -> Option<Vec<u8>> {
    let pixel_buffer: CFRetained<CVPixelBuffer> = buffer.image_buffer()?;
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
    let block: CFRetained<CMBlockBuffer> = buffer.data_buffer()?;
    let mut data_ptr: *mut std::ffi::c_char = std::ptr::null_mut();
    let mut length = 0usize;
    let mut total = 0usize;
    let status = unsafe {
        block.data_pointer(0, &mut length, &mut total, &mut data_ptr)
    };
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

struct KiriStreamDelegateIvars {
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
            if !buffer.data_is_ready() {
                return;
            }
            let state = unsafe { &*(self.ivars().state as *const DelegateState) };
            if of_type == SCStreamOutputType::Screen {
                if let Some(frame) = copy_pixel_buffer(buffer) {
                    let _ = state.video_tx.send(frame);
                }
            } else if let Some(tx) = &state.audio_tx {
                let format = state.audio_format.get_or_init(|| {
                    audio_format_from(buffer).unwrap_or(AudioFormat {
                        sample_rate: 48_000.0,
                        channels: 2,
                        bits_per_channel: 32,
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
        let this = KiriStreamDelegate::alloc().set_ivars(KiriStreamDelegateIvars { state: state_ptr });
        unsafe { msg_send![super(this), init] }
    }
}
