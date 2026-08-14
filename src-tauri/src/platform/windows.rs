//! Windows platform helpers — hotkey, focus, click monitoring, and capture
//! exclusion via Win32.

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread;

use anyhow::Result;
use tauri::Manager;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, EnumWindows, GetForegroundWindow, GetMessageW,
    GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
    SetForegroundWindow, SetWindowDisplayAffinity, SetWindowsHookExW, ShowWindow, TranslateMessage,
    MSG, MSLLHOOKSTRUCT, SW_RESTORE, WH_MOUSE_LL, WM_LBUTTONDOWN, WM_RBUTTONDOWN,
    WDA_EXCLUDEFROMCAPTURE, WDA_NONE,
};

use super::ClickMonitorHandle;

// ---------------------------------------------------------------------------
// Focus / reveal
// ---------------------------------------------------------------------------

pub fn activate_application(pid: u32) {
    if let Some(hwnd) = find_main_window(pid) {
        unsafe {
            let _ = ShowWindow(hwnd, SW_RESTORE);
            let _ = SetForegroundWindow(hwnd);
        }
    }
}

fn find_main_window(pid: u32) -> Option<HWND> {
    struct Search {
        pid: u32,
        found: Option<HWND>,
    }
    unsafe extern "system" fn callback(hwnd: HWND, lparam: LPARAM) -> windows::core::BOOL {
        let search = unsafe { &mut *(lparam.0 as *mut Search) };
        if search.found.is_some() {
            return windows::core::BOOL(0);
        }
        let mut window_pid = 0u32;
        unsafe {
            GetWindowThreadProcessId(hwnd, Some(&mut window_pid));
        }
        if window_pid == search.pid && unsafe { IsWindowVisible(hwnd).as_bool() } {
            search.found = Some(hwnd);
            return windows::core::BOOL(0);
        }
        windows::core::BOOL(1)
    }
    let mut search = Search { pid, found: None };
    unsafe {
        let _ = EnumWindows(Some(callback), LPARAM(&mut search as *mut Search as isize));
    }
    search.found
}

pub fn reveal_path(path: &Path) {
    let _ = std::process::Command::new("explorer")
        .arg(format!("/select,{}", path.display()))
        .spawn();
}

pub fn frontmost_application() -> Option<(u32, Option<String>)> {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return None;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let name = window_title(hwnd);
        Some((pid, name))
    }
}

fn window_title(hwnd: HWND) -> Option<String> {
    unsafe {
        let length = GetWindowTextLengthW(hwnd) as usize;
        if length == 0 {
            return None;
        }
        let mut buffer = vec![0u16; length + 1];
        let written = GetWindowTextW(hwnd, &mut buffer);
        Some(String::from_utf16_lossy(&buffer[..written as usize]))
    }
}

pub fn ensure_permissions() -> Result<()> {
    Ok(())
}

pub fn activate_self() {
    // set_focus on the overlay window already foregrounds it.
}

pub fn mic_supported() -> bool {
    true
}

pub fn set_window_click_through(app: &tauri::AppHandle, label: &str) {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE, WS_EX_LAYERED, WS_EX_TRANSPARENT,
    };
    let Some(window) = app.get_webview_window(label) else {
        return;
    };
    let Ok(hwnd) = window.hwnd() else {
        return;
    };
    unsafe {
        let hwnd = HWND(hwnd.0 as *mut _);
        let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let _ = SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style | WS_EX_LAYERED.0 as isize | WS_EX_TRANSPARENT.0 as isize);
    }
}

pub fn set_window_capture_excluded(app: &tauri::AppHandle, label: &str, excluded: bool) {
    let Some(window) = app.get_webview_window(label) else {
        return;
    };
    let Ok(hwnd) = window.hwnd() else {
        return;
    };
    unsafe {
        let affinity = if excluded { WDA_EXCLUDEFROMCAPTURE } else { WDA_NONE };
        let _ = SetWindowDisplayAffinity(HWND(hwnd.0 as *mut _), affinity);
    }
}

// ---------------------------------------------------------------------------
// Global click monitor (WH_MOUSE_LL)
// ---------------------------------------------------------------------------

static CLICK_CALLBACK: Mutex<Option<Arc<dyn Fn(f64, f64) + Send + Sync>>> = Mutex::new(None);

pub struct ClickMonitor {
    stop_flag: Arc<std::sync::atomic::AtomicBool>,
    _thread: thread::JoinHandle<()>,
}

impl ClickMonitorHandle for ClickMonitor {
    fn stop(self: Box<Self>) {
        self.stop_flag
            .store(true, std::sync::atomic::Ordering::SeqCst);
        *CLICK_CALLBACK.lock().unwrap() = None;
    }
}

pub fn start_click_monitor(
    callback: Arc<dyn Fn(f64, f64) + Send + Sync>,
) -> Result<Box<dyn ClickMonitorHandle + Send>> {
    *CLICK_CALLBACK.lock().unwrap() = Some(callback);
    let stop_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = stop_flag.clone();
    let thread = thread::spawn(move || unsafe {
        let hook = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook_proc), None, 0).ok();
        if hook.is_none() {
            *CLICK_CALLBACK.lock().unwrap() = None;
            return;
        }
        let mut message = MSG::default();
        while !flag.load(std::sync::atomic::Ordering::SeqCst)
            && GetMessageW(&mut message, None, 0, 0).as_bool()
        {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    });
    Ok(Box::new(ClickMonitor {
        stop_flag,
        _thread: thread,
    }))
}

unsafe extern "system" fn mouse_hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let message = wparam.0 as u32;
        if message == WM_LBUTTONDOWN || message == WM_RBUTTONDOWN {
            let data = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
            let callback = CLICK_CALLBACK.lock().unwrap();
            if let Some(callback) = callback.as_ref() {
                callback(data.pt.x as f64, data.pt.y as f64);
            }
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}
