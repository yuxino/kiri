//! Cross-platform helpers: clipboard, focus, file reveal, global click
//! monitoring, and capture-exclusion of Kiri's own windows.

pub mod macos;
pub mod windows;

#[cfg(target_os = "macos")]
pub use macos as current;

#[cfg(windows)]
pub use windows as current;

use std::path::Path;
use std::sync::Arc;

use anyhow::{bail, Result};

/// Writes PNG bytes to the system clipboard as an image.
pub fn write_image_to_clipboard(png: &[u8]) -> Result<()> {
    let image = image::load_from_memory(png).map_err(|error| anyhow::anyhow!(error))?;
    let rgba = image.to_rgba8();
    let (width, height) = rgba.dimensions();
    let data = rgba.into_raw();
    let mut clipboard = arboard::Clipboard::new().map_err(|error| anyhow::anyhow!(error))?;
    clipboard
        .set_image(arboard::ImageData {
            width: width as usize,
            height: height as usize,
            bytes: std::borrow::Cow::Owned(data),
        })
        .map_err(|error| anyhow::anyhow!(error))
}

/// Writes plain text to the system clipboard.
pub fn write_text_to_clipboard(text: &str) -> Result<()> {
    let mut clipboard = arboard::Clipboard::new().map_err(|error| anyhow::anyhow!(error))?;
    clipboard
        .set_text(text.to_string())
        .map_err(|error| anyhow::anyhow!(error))
}

/// Activate the application with the given PID (focus restoration).
pub fn activate_application(pid: u32) {
    current::activate_application(pid);
}

/// Reveal a file in Finder / Explorer.
pub fn reveal_path(path: &Path) {
    current::reveal_path(path);
}

/// Returns (pid, name) of the frontmost application.
pub fn frontmost_application() -> Option<(u32, Option<String>)> {
    current::frontmost_application()
}

/// Starts a global click listener (mouse down anywhere on the system).
/// The callback receives the click position in the primary display's
/// top-left-oriented coordinates.
pub fn start_click_monitor(
    callback: Arc<dyn Fn(f64, f64) + Send + Sync>,
) -> Result<Box<dyn ClickMonitorHandle>> {
    current::start_click_monitor(callback)
}

pub trait ClickMonitorHandle: Send {
    fn stop(self: Box<Self>);
}

/// Excludes (or re-includes) one of Kiri's windows from screen capture.
pub fn set_window_capture_excluded(app: &tauri::AppHandle, label: &str, excluded: bool) {
    current::set_window_capture_excluded(app, label, excluded);
}

/// macOS-only: the CGWindowID of a window (used for SCK exceptingWindows).
#[cfg(target_os = "macos")]
pub fn window_capture_id(app: &tauri::AppHandle, label: &str) -> Option<u32> {
    macos::window_capture_id(app, label)
}

#[cfg(windows)]
pub fn window_capture_id(_app: &tauri::AppHandle, _label: &str) -> Option<u32> {
    None
}

pub fn ensure_permissions() -> Result<()> {
    current::ensure_permissions()
}

pub fn dummy_error() -> anyhow::Error {
    anyhow::anyhow!("unimplemented")
}
