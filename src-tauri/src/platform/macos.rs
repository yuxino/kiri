//! macOS platform helpers (AppKit / CoreGraphics via objc2).

use std::path::Path;
use std::sync::Arc;
use std::thread;

use anyhow::Result;
use block2::RcBlock;
use objc2_core_foundation::CFRunLoop;
use tauri::{AppHandle, Manager};

use objc2_app_kit::{NSEvent, NSEventMask, NSRunningApplication, NSWindow, NSWorkspace};
use objc2_core_foundation::kCFRunLoopDefaultMode;
use objc2_foundation::{NSArray, NSString, NSURL};

use super::ClickMonitorHandle;

pub fn activate_application(pid: u32) {
    if let Some(application) =
        NSRunningApplication::runningApplicationWithProcessIdentifier(pid as i32)
    {
        application
            .activateWithOptions(objc2_app_kit::NSApplicationActivationOptions::ActivateAllWindows);
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

pub fn set_window_click_through(app: &tauri::AppHandle, label: &str) {
    if let Some(window) = app.get_webview_window(label) {
        let _ = window.set_ignore_cursor_events(true);
    }
}

/// The CGWindowID for one of Kiri's windows (NSWindow.windowNumber).
pub fn window_capture_id(app: &AppHandle, label: &str) -> Option<u32> {
    let window = app.get_webview_window(label)?;
    let ns_window = window.ns_window().ok()? as *mut NSWindow;
    let ns_window = unsafe { &*ns_window };
    Some(ns_window.windowNumber() as u32)
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
