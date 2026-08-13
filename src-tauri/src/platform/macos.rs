//! macOS platform helpers (AppKit / CoreGraphics via objc2).

use std::path::Path;
use std::sync::Arc;
use std::thread;

use anyhow::{anyhow, Result};
use block2::RcBlock;
use objc2_core_foundation::CFRetained;
use objc2_app_kit::{NSEvent, NSEventMask, NSRunningApplication, NSWindow, NSWorkspace};
use objc2_core_foundation::{CFMachPort, CFRunLoop, CFRunLoopSource, kCFRunLoopDefaultMode};
use objc2_core_graphics::{
    CGEvent, CGEventField, CGEventFlags, CGEventMask, CGEventTapLocation, CGEventTapOptions,
    CGEventTapPlacement, CGEventTapProxy, CGEventType,
};
use objc2_foundation::{NSArray, NSString, NSURL};
use tauri::{AppHandle, Manager};

use super::ClickMonitorHandle;

pub fn activate_application(pid: u32) {
    if let Some(application) =
        NSRunningApplication::runningApplicationWithProcessIdentifier(pid as i32)
    {
        unsafe {
            application.activateWithOptions(objc2_app_kit::NSApplicationActivationOptions::ActivateAllWindows)
        };
    }
}

pub fn reveal_path(path: &Path) {
    let url = NSURL::fileURLWithPath(&NSString::from_str(&path.display().to_string()));
    let workspace = NSWorkspace::sharedWorkspace();
    unsafe { workspace.activateFileViewerSelectingURLs(&NSArray::from_slice(&[&*url])) };
}

pub fn frontmost_application() -> Option<(u32, Option<String>)> {
    let workspace = NSWorkspace::sharedWorkspace();
    let app = workspace.frontmostApplication()?;
    let pid = app.processIdentifier() as u32;
    let name = app.localizedName().map(|name| name.to_string());
    Some((pid, name))
}

pub fn ensure_permissions() -> Result<()> {
    Ok(())
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
// Global shortcut (⇧⌘A) — CGEventTap that consumes the event, mirroring
// GlobalShortcutMonitor in AppModel.swift.
// ---------------------------------------------------------------------------

struct ShortcutState {
    action: Box<dyn Fn() + Send>,
}

pub struct ShortcutHandle {
    event_tap: CFRetained<CFMachPort>,
    _source: CFRetained<CFRunLoopSource>,
    state_ptr: usize,
    _thread: thread::JoinHandle<()>,
}

impl ShortcutHandle {
    pub fn stop(self) {
        unsafe { CGEvent::tap_enable(&self.event_tap, false) };
        if self.state_ptr != 0 {
            unsafe {
                let _ = Box::from_raw(self.state_ptr as *mut ShortcutState);
            }
        }
    }
}

unsafe extern "C-unwind" fn shortcut_callback(
    _proxy: CGEventTapProxy,
    event_type: CGEventType,
    event: std::ptr::NonNull<CGEvent>,
    user_info: *mut std::ffi::c_void,
) -> *mut CGEvent {
    // The tap callback returns the event to pass through, or null to
    // consume it (⇧⌘A is reserved exclusively).
    if event_type == CGEventType::TapDisabledByTimeout
        || event_type == CGEventType::TapDisabledByUserInput
    {
        return event.as_ptr();
    }
    let state = &*(user_info as *const ShortcutState);
    let event_ref = &*event.as_ptr();
    if is_capture_event(event_ref) {
        if event_type == CGEventType::KeyDown {
            (state.action)();
        }
        return std::ptr::null_mut();
    }
    event.as_ptr()
}

fn is_capture_event(event: &CGEvent) -> bool {
    // keycode 0 = kVK_ANSI_A
    if CGEvent::integer_value_field(Some(event), CGEventField::KeyboardEventKeycode) != 0 {
        return false;
    }
    let flags = CGEvent::flags(Some(event));
    let modifiers = flags
        & (CGEventFlags::MaskCommand
            | CGEventFlags::MaskShift
            | CGEventFlags::MaskControl
            | CGEventFlags::MaskAlternate);
    modifiers == (CGEventFlags::MaskCommand | CGEventFlags::MaskShift)
}

pub fn start_shortcut(action: Box<dyn Fn() + Send>) -> Result<ShortcutHandle> {
    // Input Monitoring permission gate.
    extern "C" {
        fn CGPreflightListenEventAccess() -> bool;
        fn CGRequestListenEventAccess() -> bool;
    }
    let permitted = unsafe { CGPreflightListenEventAccess() || CGRequestListenEventAccess() };
    if !permitted {
        return Err(anyhow!(
            "Enable Kiri in Input Monitoring settings, then quit and reopen it to reserve ⇧⌘A exclusively."
        ));
    }

    let state = ShortcutState { action };
    let state_ptr = Box::into_raw(Box::new(state)) as *mut std::ffi::c_void;
    let mask: CGEventMask = (1u64 << CGEventType::KeyDown.0) | (1u64 << CGEventType::KeyUp.0);

    let event_tap = unsafe {
        CGEvent::tap_create(
            CGEventTapLocation::SessionEventTap,
            CGEventTapPlacement::HeadInsertEventTap,
            CGEventTapOptions::Default,
            mask,
            Some(shortcut_callback),
            state_ptr,
        )
    }
    .ok_or_else(|| {
        unsafe {
            let _ = Box::from_raw(state_ptr as *mut ShortcutState);
        }
        anyhow!("Kiri could not create the exclusive ⇧⌘A keyboard filter. Check Input Monitoring and Accessibility, then quit and reopen Kiri.")
    })?;

    let source = CFMachPort::new_run_loop_source(None, Some(&event_tap), 0)
        .ok_or_else(|| anyhow!("could not create run loop source"))?;
    let run_loop = CFRunLoop::current().ok_or_else(|| anyhow!("no run loop"))?;
    unsafe { run_loop.add_source(Some(&source), kCFRunLoopDefaultMode) };
    unsafe { CGEvent::tap_enable(&event_tap, true) };

    let thread = thread::spawn(move || CFRunLoop::run());

    Ok(ShortcutHandle {
        event_tap,
        _source: source,
        state_ptr: state_ptr as usize,
        _thread: thread,
    })
}

// ---------------------------------------------------------------------------
// Global click monitor (for the recording ripple) — NSEvent global monitor
// ---------------------------------------------------------------------------

pub struct ClickMonitor {
    stop_flag: Arc<std::sync::atomic::AtomicBool>,
    _thread: thread::JoinHandle<()>,
}

impl ClickMonitorHandle for ClickMonitor {
    fn stop(self: Box<Self>) {
        self.stop_flag
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }
}

pub fn start_click_monitor(
    callback: Arc<dyn Fn(f64, f64) + Send + Sync>,
) -> Result<Box<dyn ClickMonitorHandle>> {
    let stop_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = stop_flag.clone();
    let thread = thread::spawn(move || {
        // The global monitor dispatches on the thread that installs it, which
        // must run its own run loop.
        let block = RcBlock::new(move |event: std::ptr::NonNull<NSEvent>| {
            let event = unsafe { &*event.as_ptr() };
            callback(event.locationInWindow().x, event.locationInWindow().y);
        });
        NSEvent::addGlobalMonitorForEventsMatchingMask_handler(
            NSEventMask::LeftMouseDown | NSEventMask::RightMouseDown,
            &block,
        );
        let run_loop = CFRunLoop::current().expect("no run loop");
        while !flag.load(std::sync::atomic::Ordering::SeqCst) {
            unsafe { CFRunLoop::run_in_mode(kCFRunLoopDefaultMode, 0.1, true) };
        }
    });
    Ok(Box::new(ClickMonitor {
        stop_flag,
        _thread: thread,
    }))
}
