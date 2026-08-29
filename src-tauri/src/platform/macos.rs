//! macOS platform helpers (AppKit / CoreGraphics via objc2).

use std::path::Path;
use std::sync::Arc;
use std::thread;

use anyhow::Result;
use block2::RcBlock;
use objc2_core_foundation::CFRunLoop;
use tauri::{AppHandle, Manager};

use objc2_app_kit::{
    NSEvent, NSEventMask, NSRunningApplication, NSScreenSaverWindowLevel, NSWindow,
    NSWindowCollectionBehavior, NSWorkspace,
};
use objc2_core_foundation::kCFRunLoopDefaultMode;
use objc2_foundation::{NSArray, NSString, NSURL};

use super::{ClickMonitorHandle, MicrophoneAccess, TransientWindowPolicy};

pub fn activate_application(pid: u32) {
    // Focus restoration is also reached from async recording finalization.
    // Keep the AppKit call on the main thread on every path.
    dispatch2::run_on_main(move |_main_thread| {
        if let Some(application) =
            NSRunningApplication::runningApplicationWithProcessIdentifier(pid as i32)
        {
            application.activateWithOptions(
                objc2_app_kit::NSApplicationActivationOptions::ActivateAllWindows,
            );
        }
    });
}

fn apply_transient_window_policy(ns_window: &NSWindow, policy: TransientWindowPolicy) {
    ns_window.setCollectionBehavior(transient_window_behavior(
        ns_window.collectionBehavior(),
        policy,
    ));
    if policy.screen_saver_level {
        ns_window.setLevel(NSScreenSaverWindowLevel);
    }
}

fn transient_window_behavior(
    current: NSWindowCollectionBehavior,
    policy: TransientWindowPolicy,
) -> NSWindowCollectionBehavior {
    // AppKit permits only one member from each group below. Remove defaults
    // that can keep a window attached to Kiri's ordinary Space, then declare
    // it eligible to accompany another application's fullscreen window.
    let incompatible = NSWindowCollectionBehavior::CanJoinAllSpaces
        | NSWindowCollectionBehavior::MoveToActiveSpace
        | NSWindowCollectionBehavior::Managed
        | NSWindowCollectionBehavior::Transient
        | NSWindowCollectionBehavior::Stationary
        | NSWindowCollectionBehavior::ParticipatesInCycle
        | NSWindowCollectionBehavior::IgnoresCycle
        | NSWindowCollectionBehavior::Primary
        | NSWindowCollectionBehavior::Auxiliary
        | NSWindowCollectionBehavior::CanJoinAllApplications
        | NSWindowCollectionBehavior::FullScreenPrimary
        | NSWindowCollectionBehavior::FullScreenAuxiliary
        | NSWindowCollectionBehavior::FullScreenNone;
    let mut behavior = current & !incompatible;
    if policy.can_join_all_spaces {
        behavior |= NSWindowCollectionBehavior::CanJoinAllSpaces;
    }
    if policy.full_screen_auxiliary {
        behavior |= NSWindowCollectionBehavior::CanJoinAllApplications
            | NSWindowCollectionBehavior::FullScreenAuxiliary;
    }
    if policy.stationary {
        behavior |= NSWindowCollectionBehavior::Stationary;
    }
    behavior
}

pub(super) fn configure_transient_window(
    window: &tauri::WebviewWindow,
    policy: TransientWindowPolicy,
) {
    // Recording commands may reach this helper from a Tokio worker. AppKit
    // traps off-main NSWindow mutation, so every native policy change stays on
    // the application thread.
    let window = window.clone();
    dispatch2::run_on_main(move |_main_thread| {
        let Ok(ns_window) = window.ns_window() else {
            return;
        };
        let Some(ns_window) = (unsafe { (ns_window as *mut NSWindow).as_ref() }) else {
            return;
        };
        apply_transient_window_policy(ns_window, policy);
    });
}

pub(super) fn show_window_without_activation(
    app: &AppHandle,
    label: &str,
    policy: TransientWindowPolicy,
) {
    let Some(window) = app.get_webview_window(label) else {
        return;
    };
    let fallback = window.clone();
    let shown_natively = dispatch2::run_on_main(move |_main_thread| {
        let Ok(ns_window) = window.ns_window() else {
            return false;
        };
        let Some(ns_window) = (unsafe { (ns_window as *mut NSWindow).as_ref() }) else {
            return false;
        };
        // Apply the same full-screen Space policy used by capture windows
        // before ordering passive feedback front. Otherwise the toast can be
        // created successfully but remain on another Space.
        apply_transient_window_policy(ns_window, policy);
        ns_window.orderFrontRegardless();
        true
    });
    if !shown_natively {
        let _ = fallback.show();
    }
}

pub fn reveal_path(path: &Path) {
    let url = NSURL::fileURLWithPath(&NSString::from_str(&path.display().to_string()));
    let workspace = NSWorkspace::sharedWorkspace();
    workspace.activateFileViewerSelectingURLs(&NSArray::from_slice(&[&*url]));
}

pub fn frontmost_application() -> Option<(u32, Option<String>)> {
    let workspace = NSWorkspace::sharedWorkspace();
    let app = workspace.frontmostApplication()?;
    let pid = app.processIdentifier() as u32;
    let name = app.localizedName().map(|name| name.to_string());
    Some((pid, name))
}

pub fn activate_self() {
    use objc2_app_kit::NSApplication;
    let mtm = objc2::MainThreadMarker::new().unwrap();
    let application = NSApplication::sharedApplication(mtm);
    application.activate();
}

pub fn mic_supported() -> bool {
    use objc2_foundation::NSProcessInfo;
    let version = NSProcessInfo::processInfo().operatingSystemVersion();
    version.majorVersion >= 15
}

pub fn request_microphone_access() -> Result<MicrophoneAccess> {
    use objc2_av_foundation::{AVAuthorizationStatus, AVCaptureDevice, AVMediaTypeAudio};

    if !mic_supported() {
        return Ok(MicrophoneAccess::Unsupported);
    }
    let media_type = unsafe { AVMediaTypeAudio }
        .ok_or_else(|| anyhow::anyhow!("AVFoundation audio media type is unavailable"))?;
    let status = unsafe { AVCaptureDevice::authorizationStatusForMediaType(media_type) };
    match status {
        AVAuthorizationStatus::Authorized => Ok(MicrophoneAccess::Authorized),
        AVAuthorizationStatus::Denied | AVAuthorizationStatus::Restricted => {
            Ok(MicrophoneAccess::Denied)
        }
        AVAuthorizationStatus::NotDetermined => {
            let (tx, rx) = std::sync::mpsc::sync_channel(1);
            let block = RcBlock::new(move |granted: objc2::runtime::Bool| {
                let _ = tx.send(granted.as_bool());
            });
            unsafe {
                AVCaptureDevice::requestAccessForMediaType_completionHandler(media_type, &block)
            };
            Ok(if rx.recv().unwrap_or(false) {
                MicrophoneAccess::Authorized
            } else {
                MicrophoneAccess::Denied
            })
        }
        _ => Ok(MicrophoneAccess::Denied),
    }
}

pub fn set_window_click_through(app: &tauri::AppHandle, label: &str) {
    if let Some(window) = app.get_webview_window(label) {
        let _ = window.set_ignore_cursor_events(true);
    }
}

/// The CGWindowID for one of Kiri's windows (NSWindow.windowNumber).
pub fn window_capture_id(app: &AppHandle, label: &str) -> Option<u32> {
    let window = app.get_webview_window(label)?;
    dispatch2::run_on_main(move |_main_thread| {
        let ns_window = window.ns_window().ok()? as *mut NSWindow;
        let ns_window = unsafe { &*ns_window };
        Some(ns_window.windowNumber() as u32)
    })
}

pub fn set_window_capture_excluded(app: &AppHandle, label: &str, excluded: bool) {
    // macOS exclusions happen through the SCK content filter; nothing to do
    // per-window here.
    let _ = (app, label, excluded);
}

// ---------------------------------------------------------------------------
// Global click monitor (for the recording ripple) — NSEvent global monitor
// ---------------------------------------------------------------------------

pub struct ClickMonitor {
    stop_flag: Arc<std::sync::atomic::AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

impl ClickMonitor {
    fn shutdown(&mut self) {
        self.stop_flag
            .store(true, std::sync::atomic::Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            // Joining ensures the installer thread removes the native NSEvent
            // monitor before the Rust handle is considered stopped.
            if thread.thread().id() != std::thread::current().id() {
                let _ = thread.join();
            }
        }
    }
}

impl ClickMonitorHandle for ClickMonitor {
    fn stop(mut self: Box<Self>) {
        self.shutdown();
    }
}

impl Drop for ClickMonitor {
    fn drop(&mut self) {
        self.shutdown();
    }
}

pub fn start_click_monitor(
    callback: Arc<dyn Fn(f64, f64) + Send + Sync>,
) -> Result<Box<dyn ClickMonitorHandle + Send>> {
    static INPUT_MONITORING_AUTHORIZED: std::sync::atomic::AtomicBool =
        std::sync::atomic::AtomicBool::new(false);
    if !INPUT_MONITORING_AUTHORIZED.load(std::sync::atomic::Ordering::Acquire) {
        use objc2_core_graphics::{CGPreflightListenEventAccess, CGRequestListenEventAccess};
        let authorized = CGPreflightListenEventAccess() || CGRequestListenEventAccess();
        if !authorized {
            return Err(anyhow::anyhow!(
                "macOS Input Monitoring permission is required for click highlights"
            ));
        }
        INPUT_MONITORING_AUTHORIZED.store(true, std::sync::atomic::Ordering::Release);
    }

    let stop_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = stop_flag.clone();
    let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
    let thread = thread::spawn(move || {
        // The global monitor dispatches on the thread that installs it, which
        // must run its own run loop.
        let Some(_run_loop) = CFRunLoop::current() else {
            let _ = ready_tx.send(false);
            return;
        };
        let block = RcBlock::new(move |event: std::ptr::NonNull<NSEvent>| {
            let event = unsafe { &*event.as_ptr() };
            callback(event.locationInWindow().x, event.locationInWindow().y);
        });
        let Some(monitor) = NSEvent::addGlobalMonitorForEventsMatchingMask_handler(
            NSEventMask::LeftMouseDown | NSEventMask::RightMouseDown,
            &block,
        ) else {
            let _ = ready_tx.send(false);
            return;
        };
        if ready_tx.send(true).is_err() {
            unsafe { NSEvent::removeMonitor(&monitor) };
            return;
        }
        while !flag.load(std::sync::atomic::Ordering::SeqCst) {
            unsafe { CFRunLoop::run_in_mode(kCFRunLoopDefaultMode, 0.1, true) };
        }
        unsafe { NSEvent::removeMonitor(&monitor) };
    });
    match ready_rx.recv() {
        Ok(true) => Ok(Box::new(ClickMonitor {
            stop_flag,
            thread: Some(thread),
        })),
        Ok(false) => {
            let _ = thread.join();
            Err(anyhow::anyhow!(
                "macOS did not install the global click monitor"
            ))
        }
        Err(error) => {
            let _ = thread.join();
            Err(anyhow::anyhow!(
                "global click monitor setup ended before reporting readiness: {error}"
            ))
        }
    }
}

#[cfg(test)]
mod transient_window_tests {
    use objc2_app_kit::NSWindowCollectionBehavior;

    use super::{transient_window_behavior, TransientWindowPolicy};

    #[test]
    fn fullscreen_policy_replaces_incompatible_space_flags() {
        let current = NSWindowCollectionBehavior::MoveToActiveSpace
            | NSWindowCollectionBehavior::Managed
            | NSWindowCollectionBehavior::ParticipatesInCycle
            | NSWindowCollectionBehavior::Primary
            | NSWindowCollectionBehavior::FullScreenPrimary;
        let policy = TransientWindowPolicy {
            can_join_all_spaces: true,
            full_screen_auxiliary: true,
            stationary: false,
            screen_saver_level: true,
        };

        let behavior = transient_window_behavior(current, policy);

        assert!(behavior.contains(NSWindowCollectionBehavior::CanJoinAllSpaces));
        assert!(behavior.contains(NSWindowCollectionBehavior::CanJoinAllApplications));
        assert!(behavior.contains(NSWindowCollectionBehavior::FullScreenAuxiliary));
        assert!(!behavior.contains(NSWindowCollectionBehavior::MoveToActiveSpace));
        assert!(!behavior.contains(NSWindowCollectionBehavior::Managed));
        assert!(!behavior.contains(NSWindowCollectionBehavior::Primary));
        assert!(!behavior.contains(NSWindowCollectionBehavior::FullScreenPrimary));
    }

    #[test]
    fn click_ripple_is_the_only_stationary_role() {
        let behavior = transient_window_behavior(
            NSWindowCollectionBehavior::Transient,
            TransientWindowPolicy {
                can_join_all_spaces: true,
                full_screen_auxiliary: true,
                stationary: true,
                screen_saver_level: false,
            },
        );

        assert!(behavior.contains(NSWindowCollectionBehavior::Stationary));
        assert!(!behavior.contains(NSWindowCollectionBehavior::Transient));
    }
}
