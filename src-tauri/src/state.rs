//! AppModel equivalent — coordinates capture, library operations, recording
//! state, and transient feedback across the Tauri windows.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
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
    /// Active click monitor (ripple source), installed only for recordings
    /// that explicitly enable click highlights and removed when they finish.
    pub click_monitor:
        std::sync::Mutex<Option<Box<dyn crate::platform::ClickMonitorHandle + Send>>>,
    pub ocr_providers: Arc<crate::ocr_controller::OcrProviderManager>,
    pub ocr_requests: Arc<crate::ocr_controller::OcrRequestController>,
    pub remote_ocr: Option<crate::remote_ocr::RemoteOcrClient>,
}

#[derive(Default)]
pub struct CaptureFlow {
    pub session: Option<CaptureSession>,
}

impl CaptureFlow {
    /// Invalidates a destroyed overlay owner. The final owner atomically
    /// consumes the session so a later `start_capture` cannot reuse a frozen
    /// context with no live overlay.
    pub fn destroy_overlay(&mut self, label: &str) -> Option<(uuid::Uuid, Option<CaptureSession>)> {
        let session = self.session.as_mut()?;
        if !session.overlay_labels.iter().any(|owner| owner == label) {
            return None;
        }
        let capture_id = session.capture_id;
        session.overlay_labels.retain(|owner| owner != label);
        let ended_session = if session.overlay_labels.is_empty() {
            self.session.take()
        } else {
            None
        };
        Some((capture_id, ended_session))
    }
}

pub struct CaptureSession {
    pub capture_id: uuid::Uuid,
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
            startup_token: None,
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
    /// Identifies the one asynchronous encoder-preparation task allowed to
    /// attach capture resources to this session. Resetting the flow clears
    /// the token, so a cancelled or superseded task cannot start recording.
    pub(crate) startup_token: Option<uuid::Uuid>,
    pub active: Option<ActiveRecording>,
}

impl RecordingFlow {
    /// Claims the pending recording session for asynchronous preparation.
    /// A second begin request is ignored while the first claim is live.
    pub fn claim_startup(&mut self) -> Option<uuid::Uuid> {
        if self.configuration.is_none()
            || self.startup_token.is_some()
            || self.is_recording
            || self.is_paused
            || self.is_transitioning
            || self.is_finalizing
        {
            return None;
        }
        let token = uuid::Uuid::new_v4();
        self.startup_token = Some(token);
        self.is_starting = true;
        Some(token)
    }

    pub fn startup_is_current(&self, token: uuid::Uuid) -> bool {
        self.startup_token == Some(token)
    }

    /// Publishes a fully started recorder only if its preparation task still
    /// owns this session. The caller receives stale resources back to stop.
    pub fn complete_startup(
        &mut self,
        token: uuid::Uuid,
        active: ActiveRecording,
    ) -> Result<(), ActiveRecording> {
        if !self.startup_is_current(token) {
            return Err(active);
        }
        self.startup_token = None;
        self.is_starting = false;
        self.is_recording = true;
        self.is_paused = false;
        self.started_at = Some(Instant::now());
        self.active = Some(active);
        Ok(())
    }

    /// Resets only when the named asynchronous startup still owns the flow.
    /// This protects a newer session from a late failure in an older task.
    pub fn take_if_startup_is_current(&mut self, token: uuid::Uuid) -> Option<RecordingFlow> {
        self.startup_is_current(token)
            .then(|| self.take_and_reset())
    }

    /// Restores the stable paused state after a resume attempt fails. Prior
    /// segments and configuration remain available so the user can retry or
    /// stop and save what was already recorded.
    pub fn recover_failed_resume(&mut self) -> Option<ActiveRecording> {
        self.is_starting = false;
        self.is_recording = false;
        self.is_paused = true;
        self.is_transitioning = false;
        self.is_finalizing = false;
        self.started_at = None;
        self.active.take()
    }

    /// Atomically moves every session-owned resource out and leaves an idle
    /// flow. Callers can then stop native handles and delete temporary files
    /// without holding the recording mutex.
    pub fn take_and_reset(&mut self) -> RecordingFlow {
        std::mem::take(self)
    }
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

#[derive(Default)]
pub struct ActiveRecording {
    pub encoder: Option<SegmentEncoder>,
    pub recorder: Option<Box<dyn PlatformRecorder + Send>>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RecoveryAction {
    OpenSettings,
    QuitKiri,
    OpenInputMonitoringSettings,
    OpenMicrophoneSettings,
}

impl AppState {
    pub fn new(app: &AppHandle) -> anyhow::Result<Self> {
        let (library, root) = open_library()?;
        let ocr_providers = app
            .path()
            .app_config_dir()
            .map(|path| Arc::new(crate::ocr_controller::OcrProviderManager::open(&path)))
            .unwrap_or_else(|_| Arc::new(crate::ocr_controller::OcrProviderManager::unavailable()));
        Ok(Self {
            library: std::sync::Mutex::new(library),
            capture: Default::default(),
            recording: Default::default(),
            ffmpeg_path: std::sync::OnceLock::new(),
            saved_recording_options: std::sync::Mutex::new(RecordingOptions::default()),
            library_root: root,
            gif_conversion_ids: Default::default(),
            click_monitor: std::sync::Mutex::new(None),
            ocr_providers,
            ocr_requests: Arc::new(crate::ocr_controller::OcrRequestController::default()),
            remote_ocr: crate::remote_ocr::RemoteOcrClient::new().ok(),
        })
    }

    pub fn ffmpeg(&self) -> anyhow::Result<PathBuf> {
        if let Some(path) = self.ffmpeg_path.get() {
            return Ok(path.clone());
        }
        let path = crate::record::ensure_ffmpeg()?;
        let _ = self.ffmpeg_path.set(path.clone());
        Ok(path)
    }

    pub fn asset_file_url(&self, asset: &CaptureAsset) -> PathBuf {
        self.library_root.join("Assets").join(&asset.filename)
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

/// Opens a FULL-SCREEN destructive-confirmation overlay (borderless,
/// transparent, always-on-top, covering the whole primary display). The
/// ConfirmWindow frontend dims the screen and shows a centered card; on
/// confirm it runs the requested action and closes itself. This makes
/// irreversible operations (empty trash, permanent delete) unmistakable
/// instead of a small in-window modal.
pub fn show_confirm_dialog(
    app: &AppHandle,
    kind: String,
    title: String,
    message: String,
    confirm_label: String,
    ids: Vec<String>,
) {
    let label = "confirm";
    let window = match app.get_webview_window(label) {
        Some(window) => window,
        None => {
            let (win_w, win_h) = match app.primary_monitor() {
                Ok(Some(monitor)) => {
                    let size = *monitor.size();
                    let scale = monitor.scale_factor();
                    (size.width as f64 / scale, size.height as f64 / scale)
                }
                _ => (1440.0, 900.0),
            };
            let ids_query = if ids.is_empty() {
                String::new()
            } else {
                format!("&ids={}", urlencode(&ids.join(",")))
            };
            let builder = WebviewWindowBuilder::new(
                app,
                label,
                WebviewUrl::App(
                    format!(
                        "index.html?window=confirm&kind={}&title={}&message={}&confirmLabel={}{ids_query}",
                        urlencode(&kind),
                        urlencode(&title),
                        urlencode(&message),
                        urlencode(&confirm_label),
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
            .focused(true)
            .inner_size(win_w, win_h)
            .position(0.0, 0.0);
            let Ok(window) = builder.build() else {
                return;
            };
            window
        }
    };
    // If a confirm window already exists (e.g. re-triggered), reload it with
    // the new content rather than stacking dialogs.
    let ids_query = if ids.is_empty() {
        String::new()
    } else {
        format!("&ids={}", urlencode(&ids.join(",")))
    };
    let url = format!(
        "index.html?window=confirm&kind={}&title={}&message={}&confirmLabel={}{ids_query}",
        urlencode(&kind),
        urlencode(&title),
        urlencode(&message),
        urlencode(&confirm_label),
    );
    let _ = window.eval(format!("location.href = '{}'", url.replace('\'', "\\'")));
    let _ = window.show();
    let _ = window.set_focus();
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
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'.' | b'-' | b'_' => out.push(byte as char),
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

// ---------------------------------------------------------------------------
// Language preference (persisted in the app config dir so the choice
// survives relaunches and is shared across all windows/webviews — the
// WebView's localStorage is per-window and unreliable in Tauri).
// ---------------------------------------------------------------------------

fn language_path(app: &AppHandle) -> PathBuf {
    app.path()
        .app_config_dir()
        .unwrap_or_else(|_| std::env::temp_dir())
        .join("language.json")
}

/// Returns the persisted language ("en" | "zh-Hans" | "ja") or empty string when
/// the user has never picked one (then the system locale applies).
pub fn load_language(app: &AppHandle) -> String {
    let path = language_path(app);
    std::fs::read(&path)
        .ok()
        .and_then(|data| serde_json::from_slice::<String>(&data).ok())
        .filter(|lang| lang == "en" || lang == "zh-Hans" || lang == "ja")
        .unwrap_or_default()
}

pub fn save_language(app: &AppHandle, language: &str) {
    let path = language_path(app);
    if language == "en" || language == "zh-Hans" || language == "ja" {
        if let Ok(data) = serde_json::to_vec_pretty(language) {
            let _ = std::fs::write(path, data);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        mark_error_seen, toast_position, urlencode, ActiveRecording, CaptureFlow, CaptureSession,
        RecordingConfiguration, RecordingFlow,
    };
    use crate::capture::CapturedDisplay;
    use crate::core::geometry::Rect;
    use crate::core::policy::RecordingOptions;

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
        assert!((0.0..1512.0).contains(&x), "x={x} must be on-screen");
        assert!((0.0..982.0).contains(&y), "y={y} must be on-screen");

        // Non-retina 1920×1080 @1x stays on-screen too.
        let (x, y) = toast_position(tauri::PhysicalSize::new(1920, 1080), 1.0);
        assert!((0.0..1920.0).contains(&x) && (0.0..1080.0).contains(&y));

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

    #[test]
    fn final_destroyed_overlay_consumes_capture_session() {
        let capture_id = uuid::Uuid::new_v4();
        let mut flow = CaptureFlow {
            session: Some(CaptureSession {
                capture_id,
                display: CapturedDisplay {
                    png_data: vec![1],
                    pixel_width: 1,
                    pixel_height: 1,
                    screen_frame: Rect::new(0.0, 0.0, 1.0, 1.0),
                    window_rects: Vec::new(),
                    display_id: 1,
                    backing_scale: 1.0,
                },
                source_application: None,
                return_pid: None,
                was_kiri_frontmost: true,
                hidden_windows: Vec::new(),
                overlay_labels: vec!["overlay-a".into(), "overlay-b".into()],
            }),
        };

        let (first_id, ended) = flow.destroy_overlay("overlay-a").unwrap();
        assert_eq!(first_id, capture_id);
        assert!(ended.is_none());
        assert!(flow.session.is_some());
        let (_, ended) = flow.destroy_overlay("overlay-b").unwrap();
        let ended = ended.expect("last overlay must consume the session");
        assert_eq!(ended.capture_id, capture_id);
        assert!(flow.session.is_none());
    }

    #[test]
    fn failed_resume_restores_paused_session_without_losing_segments() {
        let segment = std::path::PathBuf::from("existing-segment.mp4");
        let mut flow = RecordingFlow {
            is_paused: false,
            is_transitioning: true,
            segments: vec![segment.clone()],
            elapsed_before_segment: 12.5,
            active: Some(ActiveRecording::default()),
            configuration: Some(RecordingConfiguration {
                display_id: 7,
                region: Rect::new(10.0, 20.0, 640.0, 360.0),
                screen_frame: Rect::new(0.0, 0.0, 1440.0, 900.0),
                backing_scale: 2.0,
                options: RecordingOptions::default(),
            }),
            ..Default::default()
        };

        assert!(flow.recover_failed_resume().is_some());
        assert!(flow.is_paused);
        assert!(!flow.is_recording);
        assert!(!flow.is_transitioning);
        assert!(!flow.is_finalizing);
        assert_eq!(flow.segments, vec![segment]);
        assert_eq!(flow.elapsed_before_segment, 12.5);
        assert_eq!(flow.configuration.as_ref().unwrap().display_id, 7);
        assert!(flow.active.is_none());
    }

    #[test]
    fn failed_stop_moves_resources_out_and_returns_to_idle() {
        let segment = std::path::PathBuf::from("partial-segment.mp4");
        let mut flow = RecordingFlow {
            is_finalizing: true,
            segments: vec![segment.clone()],
            active: Some(ActiveRecording::default()),
            configuration: Some(RecordingConfiguration {
                display_id: 9,
                region: Rect::new(0.0, 0.0, 320.0, 240.0),
                screen_frame: Rect::new(0.0, 0.0, 320.0, 240.0),
                backing_scale: 1.0,
                options: RecordingOptions::default(),
            }),
            ..Default::default()
        };

        let abandoned = flow.take_and_reset();
        assert!(abandoned.is_finalizing);
        assert_eq!(abandoned.segments, vec![segment]);
        assert!(abandoned.active.is_some());
        assert!(flow.configuration.is_none());
        assert!(flow.segments.is_empty());
        assert!(!flow.is_finalizing);
        assert!(!flow.is_recording);
        assert!(!flow.is_paused);
        assert!(flow.active.is_none());
    }

    #[test]
    fn cancelled_startup_cannot_attach_to_a_replacement_session() {
        let configuration = RecordingConfiguration {
            display_id: 11,
            region: Rect::new(0.0, 0.0, 640.0, 480.0),
            screen_frame: Rect::new(0.0, 0.0, 640.0, 480.0),
            backing_scale: 1.0,
            options: RecordingOptions::default(),
        };
        let mut flow = RecordingFlow {
            is_starting: true,
            configuration: Some(configuration.clone()),
            ..Default::default()
        };

        let cancelled = flow.claim_startup().expect("startup should be claimed");
        assert!(flow.claim_startup().is_none(), "duplicate begin is ignored");
        assert!(flow.take_if_startup_is_current(cancelled).is_some());

        flow.is_starting = true;
        flow.configuration = Some(configuration);
        let replacement = flow
            .claim_startup()
            .expect("replacement startup should be claimed");
        assert_ne!(replacement, cancelled);
        assert!(flow.take_if_startup_is_current(cancelled).is_none());
        assert!(flow.startup_is_current(replacement));
    }

    #[test]
    fn only_current_startup_can_publish_active_resources() {
        let mut flow = RecordingFlow {
            is_starting: true,
            configuration: Some(RecordingConfiguration {
                display_id: 12,
                region: Rect::new(0.0, 0.0, 320.0, 240.0),
                screen_frame: Rect::new(0.0, 0.0, 320.0, 240.0),
                backing_scale: 2.0,
                options: RecordingOptions::default(),
            }),
            ..Default::default()
        };
        let current = flow.claim_startup().unwrap();
        let stale = uuid::Uuid::new_v4();

        assert!(flow
            .complete_startup(stale, ActiveRecording::default())
            .is_err());
        assert!(flow.startup_is_current(current));
        assert!(flow
            .complete_startup(current, ActiveRecording::default())
            .is_ok());
        assert!(!flow.is_starting);
        assert!(flow.is_recording);
        assert!(!flow.is_paused);
        assert!(flow.active.is_some());
    }
}
