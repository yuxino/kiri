//! Windows platform helpers — to be completed in the Windows milestone.
//! Kept compilable so CI can validate the macOS implementation in parallel.

use std::path::Path;
use std::sync::Arc;

use anyhow::Result;

use super::ClickMonitorHandle;

pub fn activate_application(_pid: u32) {
    // TODO(win): SetForegroundWindow for the process's main window.
}

pub fn reveal_path(path: &Path) {
    let _ = std::process::Command::new("explorer")
        .arg(format!("/select,{}", path.display()))
        .spawn();
}

pub fn frontmost_application() -> Option<(u32, Option<String>)> {
    None
}

pub fn ensure_permissions() -> Result<()> {
    Ok(())
}

pub fn set_window_capture_excluded(_app: &tauri::AppHandle, _label: &str, _excluded: bool) {
    // TODO(win): SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE).
}

pub struct ClickMonitor;

impl ClickMonitorHandle for ClickMonitor {
    fn stop(self: Box<Self>) {}
}

pub fn start_click_monitor(
    _callback: Arc<dyn Fn(f64, f64) + Send + Sync>,
) -> Result<Box<dyn ClickMonitorHandle>> {
    // TODO(win): WH_MOUSE_LL hook on a dedicated message-pump thread.
    Ok(Box::new(ClickMonitor))
}
