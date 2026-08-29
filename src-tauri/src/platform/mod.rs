//! Cross-platform helpers: clipboard, focus, file reveal, global click
//! monitoring, and capture-exclusion of Kiri's own windows.

#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(windows)]
pub mod windows;

#[cfg(target_os = "macos")]
pub use macos as current;

#[cfg(windows)]
pub use windows as current;

use std::path::Path;
use std::sync::Arc;

use anyhow::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MicrophoneAccess {
    Authorized,
    Unsupported,
    Denied,
}

/// Native presentation roles for Kiri's short-lived capture and feedback
/// windows. The role keeps macOS Space behavior centralized without changing
/// the corresponding Windows window configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransientWindowRole {
    CaptureOverlay,
    RecordingCountdown,
    RecordingControls,
    CompletionFeedback,
    ClickRipple,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct TransientWindowPolicy {
    pub can_join_all_spaces: bool,
    pub full_screen_auxiliary: bool,
    pub stationary: bool,
    pub screen_saver_level: bool,
}

impl TransientWindowRole {
    const fn policy(self) -> TransientWindowPolicy {
        TransientWindowPolicy {
            can_join_all_spaces: true,
            full_screen_auxiliary: true,
            stationary: matches!(self, Self::ClickRipple),
            // Preserve the existing native levels: capture UI and completion
            // feedback use the screen-saver level, while the click ripple
            // continues to rely on its always-on-top window configuration.
            screen_saver_level: !matches!(self, Self::ClickRipple),
        }
    }
}

/// Applies the native policy for a transient Kiri window. macOS requires
/// explicit collection behavior to appear in another app's full-screen Space.
pub fn configure_transient_window(window: &tauri::WebviewWindow, role: TransientWindowRole) {
    #[cfg(target_os = "macos")]
    macos::configure_transient_window(window, role.policy());

    #[cfg(not(target_os = "macos"))]
    let _ = (window, role);
}

#[cfg(any(windows, test))]
fn physical_transient_frame(
    frame: crate::core::geometry::Rect,
    backing_scale: f64,
) -> (i32, i32, u32, u32) {
    let scale = backing_scale.max(1.0);
    (
        (frame.x * scale).round() as i32,
        (frame.y * scale).round() as i32,
        (frame.width * scale).round().max(1.0) as u32,
        (frame.height * scale).round().max(1.0) as u32,
    )
}

/// Corrects transient window geometry after creation. Tao chooses a monitor
/// for logical builder coordinates by trying each monitor's scale factor;
/// that is ambiguous on Windows mixed-DPI desktops. Repositioning with an
/// explicit physical frame keeps overlays on the display that was captured.
pub fn place_transient_window(
    window: &tauri::WebviewWindow,
    frame: crate::core::geometry::Rect,
    backing_scale: f64,
) -> tauri::Result<()> {
    #[cfg(windows)]
    {
        let (x, y, width, height) = physical_transient_frame(frame, backing_scale);
        window.set_position(tauri::PhysicalPosition::new(x, y))?;
        window.set_size(tauri::PhysicalSize::new(width, height))?;
    }

    #[cfg(not(windows))]
    let _ = (window, frame, backing_scale);

    Ok(())
}

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

/// Writes an existing regular file to the system clipboard as a file item.
pub fn write_file_to_clipboard(path: &Path) -> Result<()> {
    let metadata = std::fs::metadata(path).map_err(|error| {
        anyhow::anyhow!(
            "The file could not be accessed for copying ({}): {error}",
            path.display()
        )
    })?;
    if !metadata.is_file() {
        anyhow::bail!(
            "Only an existing regular file can be copied ({}).",
            path.display()
        );
    }

    let mut clipboard = arboard::Clipboard::new()
        .map_err(|error| anyhow::anyhow!("The system clipboard is unavailable: {error}"))?;
    clipboard
        .set()
        .file_list(&[path])
        .map_err(|error| anyhow::anyhow!("The file could not be copied to the clipboard: {error}"))
}

/// Shows a window without activating Kiri or moving keyboard focus to it.
pub fn show_window_without_activation(
    app: &tauri::AppHandle,
    label: &str,
    role: TransientWindowRole,
) {
    #[cfg(target_os = "macos")]
    current::show_window_without_activation(app, label, role.policy());

    #[cfg(windows)]
    {
        let _ = role;
        current::show_window_without_activation(app, label);
    }
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
) -> Result<Box<dyn ClickMonitorHandle + Send>> {
    current::start_click_monitor(callback)
}

pub trait ClickMonitorHandle: Send + Sync {
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

/// Brings the app itself to the foreground (so overlay webviews get keys).
pub fn activate_self() {
    current::activate_self();
}

/// True when microphone capture is available (macOS 15+; Windows: always).
pub fn mic_supported() -> bool {
    current::mic_supported()
}

/// Requests microphone access when the platform supports recording it.
/// A denied or restricted grant is reported explicitly so recording never
/// starts with a silently empty microphone track.
pub fn request_microphone_access() -> Result<MicrophoneAccess> {
    current::request_microphone_access()
}

/// Makes a window click-through so it never blocks the cursor.
pub fn set_window_click_through(app: &tauri::AppHandle, label: &str) {
    current::set_window_click_through(app, label);
}

#[cfg(test)]
mod tests {
    use super::{physical_transient_frame, TransientWindowRole};

    #[test]
    fn transient_windows_join_full_screen_spaces() {
        for role in [
            TransientWindowRole::CaptureOverlay,
            TransientWindowRole::RecordingCountdown,
            TransientWindowRole::RecordingControls,
            TransientWindowRole::CompletionFeedback,
            TransientWindowRole::ClickRipple,
        ] {
            let policy = role.policy();
            assert!(policy.can_join_all_spaces, "{role:?}");
            assert!(policy.full_screen_auxiliary, "{role:?}");
        }
    }

    #[test]
    fn ripple_is_stationary_without_changing_existing_window_levels() {
        for role in [
            TransientWindowRole::CaptureOverlay,
            TransientWindowRole::RecordingCountdown,
            TransientWindowRole::RecordingControls,
            TransientWindowRole::CompletionFeedback,
        ] {
            let policy = role.policy();
            assert!(!policy.stationary, "{role:?}");
            assert!(policy.screen_saver_level, "{role:?}");
        }

        let ripple = TransientWindowRole::ClickRipple.policy();
        assert!(ripple.stationary);
        assert!(!ripple.screen_saver_level);
    }

    #[test]
    fn windows_mixed_dpi_frames_round_trip_to_physical_monitor_bounds() {
        assert_eq!(
            physical_transient_frame(
                crate::core::geometry::Rect::new(960.0, 0.0, 1280.0, 720.0),
                2.0,
            ),
            (1920, 0, 2560, 1440)
        );
        assert_eq!(
            physical_transient_frame(
                crate::core::geometry::Rect::new(-1280.0, 180.0, 1280.0, 720.0),
                2.0,
            ),
            (-2560, 360, 2560, 1440)
        );
    }
}
