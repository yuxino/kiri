//! Windows platform helpers — hotkey, focus, click monitoring, and capture
//! exclusion via Win32.

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;

use anyhow::{anyhow, bail, Result};
use tauri::Manager;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, EnumWindows, GetForegroundWindow, GetMessageW,
    GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible, PeekMessageW,
    PostThreadMessageW, SetForegroundWindow, SetWindowDisplayAffinity, SetWindowsHookExW,
    ShowWindow, TranslateMessage, UnhookWindowsHookEx, MSG, MSLLHOOKSTRUCT, PM_NOREMOVE,
    SW_RESTORE, SW_SHOWNOACTIVATE, WDA_EXCLUDEFROMCAPTURE, WDA_NONE, WH_MOUSE_LL, WM_LBUTTONDOWN,
    WM_QUIT, WM_RBUTTONDOWN,
};

use super::{ClickMonitorHandle, MicrophoneAccess};

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

pub fn show_window_without_activation(app: &tauri::AppHandle, label: &str) {
    let Some(window) = app.get_webview_window(label) else {
        return;
    };
    let Ok(hwnd) = window.hwnd() else {
        let _ = window.show();
        return;
    };
    unsafe {
        let _ = ShowWindow(HWND(hwnd.0 as *mut _), SW_SHOWNOACTIVATE);
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

pub fn activate_self() {
    // set_focus on the overlay window already foregrounds it.
}

pub fn mic_supported() -> bool {
    true
}

pub fn request_microphone_access() -> Result<MicrophoneAccess> {
    Ok(MicrophoneAccess::Authorized)
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
        let _ = SetWindowLongPtrW(
            hwnd,
            GWL_EXSTYLE,
            style | WS_EX_LAYERED.0 as isize | WS_EX_TRANSPARENT.0 as isize,
        );
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
        let affinity = if excluded {
            WDA_EXCLUDEFROMCAPTURE
        } else {
            WDA_NONE
        };
        let _ = SetWindowDisplayAffinity(HWND(hwnd.0 as *mut _), affinity);
    }
}

// ---------------------------------------------------------------------------
// Global click monitor (WH_MOUSE_LL)
// ---------------------------------------------------------------------------

type ClickCallback = Arc<dyn Fn(f64, f64) + Send + Sync>;

static CLICK_CALLBACK: Mutex<Option<ClickCallback>> = Mutex::new(None);

struct MessageLoopWorker {
    thread_id: Option<u32>,
    thread: Option<thread::JoinHandle<()>>,
}

impl MessageLoopWorker {
    fn shutdown(&mut self) {
        let Some(thread) = self.thread.take() else {
            self.thread_id = None;
            return;
        };

        if let Some(thread_id) = self.thread_id.take() {
            if let Err(error) = post_thread_quit(thread_id) {
                // A finished message loop has no queue, so a failed post is
                // harmless when the worker already exited. Joining below
                // still reaps it and makes shutdown deterministic.
                if !thread.is_finished() {
                    log::warn!("Windows click monitor could not request shutdown: {error}");
                }
            }
        }

        // A callback could theoretically own the final handle. Never join the
        // current thread; WM_QUIT will be consumed after that callback returns.
        if thread.thread().id() != thread::current().id() && thread.join().is_err() {
            log::warn!("Windows click monitor thread panicked during shutdown");
        }
    }
}

impl Drop for MessageLoopWorker {
    fn drop(&mut self) {
        self.shutdown();
    }
}

pub struct ClickMonitor {
    worker: MessageLoopWorker,
}

impl ClickMonitor {
    fn shutdown(&mut self) {
        self.worker.shutdown();
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
    {
        let mut slot = CLICK_CALLBACK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if slot.is_some() {
            bail!("The Windows click monitor is already running.");
        }
        *slot = Some(callback);
    }

    let (ready_tx, ready_rx) = mpsc::sync_channel::<std::result::Result<u32, String>>(1);
    let installing_thread_id = Arc::new(AtomicU32::new(0));
    let thread_id_slot = Arc::clone(&installing_thread_id);
    let installation_cancelled = Arc::new(AtomicBool::new(false));
    let cancelled_slot = Arc::clone(&installation_cancelled);
    let thread = match thread::Builder::new()
        .name("kiri-click-monitor".into())
        .spawn(move || run_click_monitor_loop(ready_tx, &thread_id_slot, &cancelled_slot))
    {
        Ok(thread) => thread,
        Err(error) => {
            clear_click_callback();
            return Err(error.into());
        }
    };

    let thread_id = match ready_rx.recv_timeout(Duration::from_secs(5)) {
        Ok(Ok(thread_id)) => thread_id,
        Ok(Err(error)) => {
            let _ = thread.join();
            return Err(anyhow!(error));
        }
        Err(error) => {
            installation_cancelled.store(true, Ordering::Release);
            let thread_id = installing_thread_id.load(Ordering::Acquire);
            let mut worker = MessageLoopWorker {
                thread_id: (thread_id != 0).then_some(thread_id),
                thread: Some(thread),
            };
            worker.shutdown();
            clear_click_callback();
            return Err(anyhow!(
                "Windows click monitor did not become ready: {error}"
            ));
        }
    };

    Ok(Box::new(ClickMonitor {
        worker: MessageLoopWorker {
            thread_id: Some(thread_id),
            thread: Some(thread),
        },
    }))
}

fn run_click_monitor_loop(
    ready_tx: mpsc::SyncSender<std::result::Result<u32, String>>,
    installing_thread_id: &AtomicU32,
    installation_cancelled: &AtomicBool,
) {
    unsafe {
        // Thread messages are delivered only after the receiver owns a message
        // queue. Create it before publishing the thread id so WM_QUIT cannot
        // race startup and disappear.
        let mut message = MSG::default();
        let _ = PeekMessageW(&mut message, None, 0, 0, PM_NOREMOVE);
        let thread_id = GetCurrentThreadId();
        installing_thread_id.store(thread_id, Ordering::Release);

        let hook = match SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook_proc), None, 0) {
            Ok(hook) => hook,
            Err(error) => {
                let _ = ready_tx.send(Err(format!(
                    "The Windows click monitor hook could not be installed: {error}"
                )));
                clear_click_callback();
                return;
            }
        };

        if installation_cancelled.load(Ordering::Acquire) {
            let _ = UnhookWindowsHookEx(hook);
            clear_click_callback();
            return;
        }

        if ready_tx.send(Ok(thread_id)).is_err() {
            let _ = UnhookWindowsHookEx(hook);
            clear_click_callback();
            return;
        }

        loop {
            let status = GetMessageW(&mut message, None, 0, 0).0;
            if status == 0 {
                break;
            }
            if status == -1 {
                log::error!("Windows click monitor message loop failed");
                break;
            }
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }

        if let Err(error) = UnhookWindowsHookEx(hook) {
            log::warn!("Windows click monitor hook could not be removed: {error}");
        }
        clear_click_callback();
    }
}

fn post_thread_quit(thread_id: u32) -> windows::core::Result<()> {
    unsafe { PostThreadMessageW(thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) }
}

fn clear_click_callback() {
    *CLICK_CALLBACK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
}

unsafe extern "system" fn mouse_hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let message = wparam.0 as u32;
        if message == WM_LBUTTONDOWN || message == WM_RBUTTONDOWN {
            let data = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
            let callback = CLICK_CALLBACK
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone();
            if let Some(callback) = callback {
                callback(data.pt.x as f64, data.pt.y as f64);
            }
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_loop_shutdown_wakes_joins_and_is_idempotent() {
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let (stopped_tx, stopped_rx) = mpsc::sync_channel(1);
        let thread = thread::spawn(move || unsafe {
            let mut message = MSG::default();
            let _ = PeekMessageW(&mut message, None, 0, 0, PM_NOREMOVE);
            ready_tx.send(GetCurrentThreadId()).unwrap();
            let stopped_by_quit = GetMessageW(&mut message, None, 0, 0).0 == 0;
            stopped_tx.send(stopped_by_quit).unwrap();
        });
        let thread_id = ready_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("test message loop should become ready");
        let mut worker = MessageLoopWorker {
            thread_id: Some(thread_id),
            thread: Some(thread),
        };

        worker.shutdown();
        worker.shutdown();

        assert!(stopped_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("WM_QUIT should wake the test message loop"));
        assert!(worker.thread_id.is_none());
        assert!(worker.thread.is_none());
    }
}
