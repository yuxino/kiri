//! AppModel equivalent — coordinates capture, library operations, recording
//! state, and transient feedback across the Tauri windows.

use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Instant;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::capture::{CapturedDisplay, PlatformRecorder};
use crate::core::asset::CaptureAsset;
use crate::core::library::AssetLibrary;
use crate::core::policy::RecordingOptions;
use crate::record::SegmentEncoder;

pub struct AppState {
    pub library: std::sync::Mutex<AssetLibrary>,
    pub capture: std::sync::Mutex<CaptureFlow>,
    pub recording: std::sync::Mutex<RecordingFlow>,
    pub ffmpeg_path: std::sync::OnceLock<PathBuf>,
    pub saved_recording_options: std::sync::Mutex<RecordingOptions>,
    pub library_root: PathBuf,
    pub gif_conversion_ids: std::sync::Mutex<HashSet<uuid::Uuid>>,
    /// App-lifetime global click monitor (ripple source), installed once.
    pub click_monitor: std::sync::Mutex<Option<Box<dyn crate::platform::ClickMonitorHandle + Send>>>,
}

#[derive(Default)]
pub struct CaptureFlow {
    pub session: Option<CaptureSession>,
}

pub struct CaptureSession {
    pub display: CapturedDisplay,
    pub source_application: Option<String>,
    /// PID of the application that was frontmost before capture started.
    pub return_pid: Option<u32>,
    /// True when Kiri itself was frontmost at capture start.
    pub was_kiri_frontmost: bool,
    /// Library windows hidden for the capture session.
    pub hidden_windows: Vec<String>,
    pub overlay_labels: Vec<String>,
}

impl Default for RecordingFlow {
    fn default() -> Self {
        Self {
            return_pid: None,
            was_kiri_frontmost: false,
            is_starting: false,
            is_recording: false,
            is_paused: false,
            is_transitioning: false,
            is_finalizing: false,
            elapsed_before_segment: 0.0,
            started_at: None,
            segments: Vec::new(),
            configuration: None,
            active: None,
        }
    }
}

pub struct RecordingFlow {
    /// PID of the app that was frontmost before capture (focus restoration).
    pub return_pid: Option<u32>,
    pub was_kiri_frontmost: bool,
    pub is_starting: bool,
    pub is_recording: bool,
    pub is_paused: bool,
    pub is_transitioning: bool,
    pub is_finalizing: bool,
    pub elapsed_before_segment: f64,
    pub started_at: Option<Instant>,
    pub segments: Vec<PathBuf>,
    pub configuration: Option<RecordingConfiguration>,
    pub active: Option<ActiveRecording>,
}

#[derive(Debug, Clone)]
pub struct RecordingConfiguration {
    pub display_id: u32,
    /// Display-local, top-left orientation, in points.
    pub region: crate::core::geometry::Rect,
    /// Global display frame in points (top-left orientation).
    pub screen_frame: crate::core::geometry::Rect,
    pub backing_scale: f64,
    pub options: RecordingOptions,
}

pub struct ActiveRecording {
    pub encoder: Option<SegmentEncoder>,
    pub recorder: Option<Box<dyn PlatformRecorder + Send>>,
}

#[allow(clippy::derivable_impls)]
impl Default for ActiveRecording {
    fn default() -> Self {
        Self {
            encoder: None,
            recorder: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingStateDto {
    pub is_starting: bool,
    pub is_recording: bool,
    pub is_paused: bool,
    pub is_transitioning: bool,
    pub is_finalizing: bool,
    pub elapsed: f64,
    pub elapsed_label: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NoticeDto {
    pub id: String,
    pub title: String,
    pub symbol: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorDto {
    pub message: String,
    pub recovery: Option<String>,
}

// The three *Settings variants are never constructed yet (only OpenSettings /
// QuitKiri are emitted), but they are part of the serialized contract the
// frontend already handles (recoveryLabel / openSettings dispatch).
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RecoveryAction {
    OpenSettings,
    QuitKiri,
    OpenAccessibilitySettings,
    OpenInputMonitoringSettings,
    OpenMicrophoneSettings,
}

impl AppState {
    pub fn new(_app: &AppHandle) -> anyhow::Result<Self> {
        let (library, root) = open_library()?;
        Ok(Self {
            library: std::sync::Mutex::new(library),
            capture: Default::default(),
            recording: Default::default(),
            ffmpeg_path: std::sync::OnceLock::new(),
            saved_recording_options: std::sync::Mutex::new(RecordingOptions::default()),
            library_root: root,
            gif_conversion_ids: Default::default(),
            click_monitor: std::sync::Mutex::new(None),
        })
    }

    pub fn ffmpeg(&self, app: &AppHandle) -> anyhow::Result<PathBuf> {
        if let Some(path) = self.ffmpeg_path.get() {
            return Ok(path.clone());
        }
        let resource_dir = app
            .path()
            .resource_dir()
            .ok();
        let path = crate::record::ensure_ffmpeg(resource_dir)?;
        let _ = self.ffmpeg_path.set(path.clone());
        Ok(path)
    }

    pub fn asset_file_url(&self, asset: &CaptureAsset) -> PathBuf {
        self.library_root
            .join("Assets")
            .join(&asset.filename)
    }
}

fn open_library() -> anyhow::Result<(AssetLibrary, PathBuf)> {
    let root = AssetLibrary::default_root_url()
        .or_else(|| dirs::data_local_dir().map(|dir| dir.join("kiri")))
        .unwrap_or_else(|| std::env::temp_dir().join("kiri-library"));
    let library = AssetLibrary::open(root.clone())?;
    Ok((library, root))
}

// ---------------------------------------------------------------------------
// Frontend notifications
// ---------------------------------------------------------------------------

pub fn emit_notice(app: &AppHandle, title: String, symbol: String) {
    let notice = NoticeDto {
        id: uuid::Uuid::new_v4().to_string(),
        title,
        symbol,
    };
    let _ = app.emit("notice", notice.clone());
    show_completion_toast(app, &notice);
}

/// Library-scoped notice: shown only inside the library window, never as a
/// global toast. Used for in-library feedback (moved to trash, restored,
/// deleted, emptied) where the user is already looking at the library and a
/// floating toast at the screen corner would be noise.
pub fn emit_notice_local(app: &AppHandle, title: String, symbol: String) {
    let notice = NoticeDto {
        id: uuid::Uuid::new_v4().to_string(),
        title,
        symbol,
    };
    let _ = app.emit("notice", notice);
}

/// Shows the notice as a borderless always-on-top toast near the TOP-CENTER
/// of the primary display. Screenshots and recordings return focus to the
/// source application, so the library-window notice alone is easy to miss;
/// the toast keeps completion feedback visible without covering the Dock or
/// the taskbar. It reuses one resident "toast" window and hides itself after
/// 2s (see ToastWindow.tsx).
fn show_completion_toast(app: &AppHandle, notice: &NoticeDto) {
    let label = "toast";
    let window = match app.get_webview_window(label) {
        Some(window) => window,
        None => {
            // Position in LOGICAL coordinates. monitor.size() returns
            // physical pixels (e.g. 3024×1964 on a Retina display) but
            // WebviewWindowBuilder::position takes logical points — feeding
            // the physical value put the toast far off-screen (2652 > 1512
            // logical width) so it was never visible.
            let (win_x, win_y) = match app.primary_monitor() {
                Ok(Some(monitor)) => {
                    let size = *monitor.size();
                    toast_position(size, monitor.scale_factor())
                }
                _ => (540.0, 24.0), // 1440×900 fallback, top-center
            };
            let builder = WebviewWindowBuilder::new(
                app,
                label,
                WebviewUrl::App(
                    format!(
                        "index.html?window=toast&title={}&symbol={}",
                        urlencode(&notice.title),
                        urlencode(&notice.symbol),
                    )
                    .into(),
                ),
            )
            .title("kiri")
            .decorations(false)
            .transparent(true)
            .shadow(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .resizable(false)
            .focused(false)
            .visible(false)
            .inner_size(360.0, 60.0)
            .position(win_x, win_y);
            let Ok(window) = builder.build() else {
                return;
            };
            window
        }
    };
    // Content is delivered via event so the resident window can update in
    // place (initial render reads the URL params above).
    let _ = window.emit("toast", notice.clone());
    let _ = window.show();
}

/// Top-center toast position in LOGICAL points for a monitor of the given
/// PHYSICAL pixel size and scale factor. (WebviewWindowBuilder::position
/// takes logical points; feeding physical pixels put the toast off-screen
/// on Retina displays.)
fn toast_position(size: tauri::PhysicalSize<u32>, scale: f64) -> (f64, f64) {
    let logical_w = size.width as f64 / scale;
    let x = ((logical_w - 360.0) / 2.0).max(0.0);
    (x, 24.0)
}

/// Minimal percent-encoding for query values (notice titles/symbols are
/// ASCII words and dots; spaces and a few characters need escaping).
fn urlencode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'.' | b'-' | b'_' => {
                out.push(byte as char)
            }
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

pub fn emit_error(app: &AppHandle, message: String, recovery: Option<RecoveryAction>) {
    // Persist every error to a log file so repeated failures are recorded
    // even when the UI stops re-prompting (see dedupe below).
    append_error_log(&message, recovery);

    // Show a given error message at most once per launch. Repeated failures
    // (e.g. a denied permission that the user already dismissed) would
    // otherwise re-open the banner on every capture attempt; the first
    // occurrence surfaces it, later ones are only logged.
    if !mark_error_seen(&message) {
        log::info!("[error] suppressed duplicate banner: {message}");
        return;
    }

    let recovery = recovery.map(|action| match action {
        RecoveryAction::OpenSettings => "openSettings",
        RecoveryAction::QuitKiri => "quitKiri",
        RecoveryAction::OpenAccessibilitySettings => "openAccessibilitySettings",
        RecoveryAction::OpenInputMonitoringSettings => "openInputMonitoringSettings",
        RecoveryAction::OpenMicrophoneSettings => "openMicrophoneSettings",
    });
    let _ = app.emit(
        "error",
        ErrorDto {
            message,
            recovery: recovery.map(String::from),
        },
    );
}

// ---------------------------------------------------------------------------
// Error deduplication + persistent log
// ---------------------------------------------------------------------------

static SEEN_ERRORS: std::sync::OnceLock<std::sync::Mutex<std::collections::HashSet<String>>> =
    std::sync::OnceLock::new();

fn seen_errors() -> &'static std::sync::Mutex<std::collections::HashSet<String>> {
    SEEN_ERRORS.get_or_init(|| std::sync::Mutex::new(std::collections::HashSet::new()))
}

/// Returns true if this message has not been shown yet this launch.
fn mark_error_seen(message: &str) -> bool {
    seen_errors().lock().unwrap().insert(message.to_string())
}

/// Appends `[timestamp] message (recovery)` to the per-user error log at
/// `~/Library/Logs/io.yuxino.kiri/errors.log` (or the equivalent platform
/// log dir). Failures are never silently dropped: even deduped repeats are
/// recorded here for later inspection.
fn append_error_log(message: &str, recovery: Option<RecoveryAction>) {
    let Some(log_dir) = log_dir() else {
        return;
    };
    let path = log_dir.join("errors.log");
    let recovery = match recovery {
        Some(RecoveryAction::OpenSettings) => " [openSettings]",
        Some(RecoveryAction::QuitKiri) => " [quitKiri]",
        Some(RecoveryAction::OpenAccessibilitySettings) => " [openAccessibilitySettings]",
        Some(RecoveryAction::OpenInputMonitoringSettings) => " [openInputMonitoringSettings]",
        Some(RecoveryAction::OpenMicrophoneSettings) => " [openMicrophoneSettings]",
        None => "",
    };
    use std::io::Write;
    let line = format!("{} {}{}\n", now_rfc3339(), message, recovery);
    let mut file = match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        Ok(file) => file,
        Err(error) => {
            log::warn!("[error] could not write {path:?}: {error}");
            return;
        }
    };
    if let Err(error) = file.write_all(line.as_bytes()) {
        log::warn!("[error] could not append to {path:?}: {error}");
    }
}

fn log_dir() -> Option<std::path::PathBuf> {
    #[cfg(target_os = "macos")]
    {
        dirs::home_dir().map(|home| home.join("Library/Logs/io.yuxino.kiri"))
    }
    #[cfg(not(target_os = "macos"))]
    {
        dirs::data_local_dir().map(|dir| dir.join("kiri/logs"))
    }
}

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

pub fn emit_library_changed(app: &AppHandle) {
    let _ = app.emit("library-changed", ());
}

pub fn recording_state(state: &RecordingFlow) -> RecordingStateDto {
    let elapsed = state.elapsed_before_segment
        + state
            .started_at
            .map(|start| start.elapsed().as_secs_f64())
            .unwrap_or(0.0);
    RecordingStateDto {
        is_starting: state.is_starting,
        is_recording: state.is_recording,
        is_paused: state.is_paused,
        is_transitioning: state.is_transitioning,
        is_finalizing: state.is_finalizing,
        elapsed,
        elapsed_label: crate::core::policy::RecordingPolicy::elapsed_label(elapsed),
    }
}

pub fn emit_recording_state(app: &AppHandle, state: &RecordingFlow) {
    let _ = app.emit("recording-state", recording_state(state));
}

/// Access the recording options preference storage.
pub fn recording_options_path(app: &AppHandle) -> PathBuf {
    app.path()
        .app_config_dir()
        .unwrap_or_else(|_| std::env::temp_dir())
        .join("recording-options.json")
}

pub fn load_recording_options(app: &AppHandle) -> RecordingOptions {
    let path = recording_options_path(app);
    std::fs::read(&path)
        .ok()
        .and_then(|data| serde_json::from_slice::<RecordingOptions>(&data).ok())
        .unwrap_or_default()
        .normalized()
}

pub fn save_recording_options(app: &AppHandle, options: &RecordingOptions) {
    let path = recording_options_path(app);
    if let Ok(data) = serde_json::to_vec_pretty(&options.normalized()) {
        let _ = std::fs::write(path, data);
    }
}

#[cfg(test)]
mod tests {
    use super::{mark_error_seen, toast_position, urlencode};

    #[test]
    fn urlencode_escapes_spaces_and_keeps_words() {
        assert_eq!(urlencode("Copied to Clipboard"), "Copied%20to%20Clipboard");
        assert_eq!(urlencode("checkmark.circle.fill"), "checkmark.circle.fill");
        assert_eq!(urlencode("Recording Saved"), "Recording%20Saved");
        assert_eq!(urlencode("a/b c"), "a%2Fb%20c");
    }

    #[test]
    fn toast_position_is_top_centered_on_screen() {
        // Retina 3024×1964 @2x: physical pixels must be divided by scale so
        // the toast lands inside the 1512×982 logical screen (regression:
        // using 3024−372 = 2652 put it far off-screen).
        let (x, y) = toast_position(tauri::PhysicalSize::new(3024, 1964), 2.0);
        assert_eq!(x, (3024.0 / 2.0 - 360.0) / 2.0);
        assert_eq!(y, 24.0);
        assert!(x < 1512.0 && x >= 0.0, "x={x} must be on-screen");
        assert!(y < 982.0 && y >= 0.0, "y={y} must be on-screen");

        // Non-retina 1920×1080 @1x stays on-screen too.
        let (x, y) = toast_position(tauri::PhysicalSize::new(1920, 1080), 1.0);
        assert!(x < 1920.0 && x >= 0.0 && y < 1080.0 && y >= 0.0);

        // Small screens must not go negative.
        let (x, _) = toast_position(tauri::PhysicalSize::new(320, 240), 1.0);
        assert!(x >= 0.0);
    }

    #[test]
    fn duplicate_error_message_is_seen_only_once() {
        // First occurrence surfaces; the identical message must not re-open
        // the banner (the dedupe that stops repeated error prompts).
        assert!(mark_error_seen("Screen Recording is off."));
        assert!(!mark_error_seen("Screen Recording is off."));
        assert!(mark_error_seen("A different error."));
        assert!(!mark_error_seen("A different error."));
    }
}
