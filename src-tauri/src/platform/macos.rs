//! macOS platform helpers (AppKit / CoreGraphics via objc2).

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::thread;

use anyhow::Result;
use block2::RcBlock;
use objc2::{rc::Retained, MainThreadOnly};
use objc2_core_foundation::CFRunLoop;
use tauri::{AppHandle, Manager};

use objc2_app_kit::{
    NSBackingStoreType, NSColor, NSEvent, NSEventMask, NSPanel, NSRunningApplication,
    NSScreenSaverWindowLevel, NSWindow, NSWindowCollectionBehavior, NSWindowOrderingMode,
    NSWindowStyleMask, NSWorkspace,
};
use objc2_core_foundation::kCFRunLoopDefaultMode;
use objc2_foundation::{MainThreadMarker, NSArray, NSRect, NSString, NSURL};

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

thread_local! {
    // A regular NSWindow cannot join another application's native full-screen
    // Space. An invisible NSPanel parent gives the existing Tao window that
    // membership without replacing its class, delegate, content view or input
    // handling. Parents are removed when their child window is destroyed.
    static SPACE_PARENTS: RefCell<HashMap<isize, Retained<NSPanel>>> = RefCell::new(HashMap::new());
}

fn attach_space_parent(window: &tauri::WebviewWindow, ns_window: &NSWindow) {
    let id = ns_window.windowNumber();
    let existing = SPACE_PARENTS.with_borrow(|parents| parents.get(&id).cloned());
    let created = existing.is_none();
    let parent = existing.unwrap_or_else(|| {
        let parent = NSPanel::initWithContentRect_styleMask_backing_defer(
            NSPanel::alloc(MainThreadMarker::new().expect("AppKit main thread")),
            NSRect::ZERO,
            NSWindowStyleMask::NonactivatingPanel,
            NSBackingStoreType::Buffered,
            false,
        );
        // The registry owns the panel; close must not consume its retain.
        unsafe { parent.setReleasedWhenClosed(false) };
        parent.setOpaque(false);
        parent.setBackgroundColor(Some(&NSColor::clearColor()));
        parent.setHasShadow(false);
        parent.setIgnoresMouseEvents(true);
        parent.setHidesOnDeactivate(false);
        SPACE_PARENTS.with_borrow_mut(|parents| parents.insert(id, parent.clone()));
        parent
    });
    // Do not hold a registry borrow across AppKit calls: native notifications
    // can re-enter window lifecycle callbacks on this same thread.
    // Moving a parent also moves its child. A resident toast may already
    // have been repositioned for another display: detach it first so that
    // updating the parent does not apply the same displacement twice.
    if !created {
        parent.removeChildWindow(ns_window);
    }
    parent.setFrame_display(ns_window.frame(), false);
    parent.setCollectionBehavior(ns_window.collectionBehavior());
    parent.setLevel(ns_window.level());
    parent.orderFrontRegardless();
    unsafe { parent.addChildWindow_ordered(ns_window, NSWindowOrderingMode::Above) };
    if created {
        window.on_window_event(move |event| {
            if matches!(event, tauri::WindowEvent::Destroyed) {
                dispatch2::run_on_main(move |_| {
                    let parent = SPACE_PARENTS.with_borrow_mut(|parents| parents.remove(&id));
                    if let Some(parent) = parent {
                        if let Some(children) = parent.childWindows() {
                            for child in &children {
                                parent.removeChildWindow(&child);
                            }
                        }
                        parent.close();
                    }
                });
            }
        });
    }
}

fn apply_transient_window_policy(
    window: &tauri::WebviewWindow,
    ns_window: &NSWindow,
    policy: TransientWindowPolicy,
) {
    ns_window.setCollectionBehavior(transient_window_behavior(
        ns_window.collectionBehavior(),
        policy,
    ));
    if policy.screen_saver_level {
        ns_window.setLevel(NSScreenSaverWindowLevel);
    }
    // A shortcut may be pressed while another application owns a native
    // full-screen Space. Changing collectionBehavior makes this window
    // eligible for that Space, but a window that was created before the
    // policy change can remain behind the full-screen owner until it is
    // explicitly reordered. orderFrontRegardless is intentionally
    // non-activating: it keeps the video/app in its full-screen Space instead
    // of switching the user back to Kiri's ordinary Space.
    if policy.full_screen_auxiliary {
        attach_space_parent(window, ns_window);
        ns_window.orderFrontRegardless();
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
        apply_transient_window_policy(&window, ns_window, policy);
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
        apply_transient_window_policy(&window, ns_window, policy);
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
        assert!(behavior.contains(NSWindowCollectionBehavior::CanJoinAllApplications));
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
