//! Linux platform helpers: focus restoration, file reveal, click monitoring
//! stubs, and capture-exclusion stubs for the screenshot MVP.

use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use anyhow::{anyhow, Result};

use super::{ClickMonitorHandle, MicrophoneAccess};

/// Focus restoration is best-effort on Wayland; many compositors deny
/// application-driven activation. A no-op keeps the capture contract intact.
pub fn activate_application(_pid: u32) {}

pub fn reveal_path(path: &Path) {
    let target = if path.is_dir() {
        path.to_path_buf()
    } else {
        path.parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| path.to_path_buf())
    };
    let _ = Command::new("xdg-open").arg(&target).spawn();
}

/// Wayland does not expose a portable frontmost-application API to sandboxed
/// apps. Returning `None` skips focus restoration metadata without failing capture.
pub fn frontmost_application() -> Option<(u32, Option<String>)> {
    None
}

pub fn mic_supported() -> bool {
    false
}

pub fn request_microphone_access() -> Result<MicrophoneAccess> {
    Ok(MicrophoneAccess::Unsupported)
}

pub fn set_window_click_through(_app: &tauri::AppHandle, _label: &str) {
    // GTK/WebKit click-through is not required for the screenshot MVP.
}

pub fn set_window_capture_excluded(_app: &tauri::AppHandle, _label: &str, _excluded: bool) {
    // Portal ScreenCast cannot reliably exclude sibling windows yet.
}

/// Global click monitoring for the recorded ripple requires compositor support
/// that is not portable across Wayland sessions; refuse rather than half-work.
pub fn start_click_monitor(
    _callback: Arc<dyn Fn(f64, f64) + Send + Sync>,
) -> Result<Box<dyn ClickMonitorHandle + Send>> {
    Err(anyhow!(
        "Global click highlights are not available on this Linux session yet."
    ))
}
