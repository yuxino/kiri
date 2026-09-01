//! AppModel equivalent — coordinates capture, library operations, recording
//! state, and transient feedback across the Tauri windows.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use serde::Serialize;
use tauri::{
    AppHandle, Emitter, LogicalSize, Manager, Monitor, PhysicalPosition, PhysicalSize, WebviewUrl,
    WebviewWindow, WebviewWindowBuilder,
};

use crate::capture::{CapturedDisplay, PlatformRecorder};
use crate::core::annotation::{AnnotationAppearance, AnnotationDocument};
use crate::core::asset::CaptureAsset;
use crate::core::geometry::Rect;
use crate::core::library::AssetLibrary;
use crate::core::library_location::LibraryContext;
use crate::core::policy::RecordingOptions;
use crate::core::recording_recovery::RecordingRecoveryStore;
use crate::record::SegmentEncoder;

pub struct AppState {
    pub library: std::sync::Mutex<LibraryContext>,
    pub library_transition: std::sync::Mutex<()>,
    pub(crate) capture_start: CaptureStartGate,
    pub capture: std::sync::Mutex<CaptureFlow>,
    /// Successful capture feedback waits here until the owner overlay has
    /// actually been destroyed. Creating another WebView from the synchronous
    /// confirmation IPC can block WebView2 before its response is delivered.
    pub pending_capture_completion: std::sync::Mutex<Option<PendingCaptureCompletion>>,
    pub recording: std::sync::Mutex<RecordingFlow>,
    pub ffmpeg_path: std::sync::OnceLock<PathBuf>,
    pub saved_annotation_appearance: std::sync::Mutex<AnnotationAppearance>,
    pub saved_recording_options: std::sync::Mutex<RecordingOptions>,
    pub recording_recovery: std::sync::Mutex<RecordingRecoveryStore>,
    pub recording_recovery_transition: std::sync::Mutex<()>,
    pub gif_conversion_ids: std::sync::Mutex<HashSet<uuid::Uuid>>,
    /// Active click monitor (ripple source), installed only for recordings
    /// that explicitly enable click highlights and removed when they finish.
    pub click_monitor:
        std::sync::Mutex<Option<Box<dyn crate::platform::ClickMonitorHandle + Send>>>,
    pub ocr_providers: Arc<crate::ocr_controller::OcrProviderManager>,
    pub ocr_requests: Arc<crate::ocr_controller::OcrRequestController>,
    /// One bounded annotation snapshot per live editor window. A save consumes
    /// the snapshot; destroying the window discards it.
    pub editor_annotations: std::sync::Mutex<HashMap<String, StagedEditorAnnotation>>,
    /// One native-save-panel destination per live editor window. The renderer
    /// receives only the opaque token; a save consumes both token and path.
    pub editor_save_destinations: std::sync::Mutex<HashMap<String, ApprovedEditorSave>>,
    remote_ocr: std::sync::OnceLock<Option<crate::remote_ocr::RemoteOcrClient>>,
}

#[derive(Default)]
pub(crate) struct CaptureStartGate(std::sync::atomic::AtomicBool);

impl CaptureStartGate {
    /// Allows only one native display freeze to be in flight. Windows Graphics
    /// Capture can pump another shortcut event before its first frame arrives;
    /// a second `start_capture` must fail fast instead of waiting on a mutex
    /// already held by the first invocation.
    pub(crate) fn try_begin(&self) -> Option<CaptureStartPermit<'_>> {
        self.0
            .compare_exchange(
                false,
                true,
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Acquire,
            )
            .ok()
            .map(|_| CaptureStartPermit { gate: self })
    }
}

pub(crate) struct CaptureStartPermit<'a> {
    gate: &'a CaptureStartGate,
}

impl Drop for CaptureStartPermit<'_> {
    fn drop(&mut self) {
        self.gate
            .0
            .store(false, std::sync::atomic::Ordering::Release);
    }
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
    /// Present only after an overlay with committed marks stages the matching
    /// selection/document pair. Confirmation consumes the whole session.
    pub annotation: Option<StagedCaptureAnnotation>,
}

pub struct PendingCaptureCompletion {
    pub session: CaptureSession,
    pub preview: CompletionPreviewDto,
    pub monitor: Option<Monitor>,
}

#[derive(Debug, Clone)]
pub struct StagedCaptureAnnotation {
    pub token: uuid::Uuid,
    pub selection: Rect,
    pub document: AnnotationDocument,
}

#[derive(Debug, Clone)]
pub struct StagedEditorAnnotation {
    pub token: uuid::Uuid,
    pub document: AnnotationDocument,
    /// Backend-derived crop of the exact content-addressed clean source.
    pub replacement_source_png: Option<Vec<u8>>,
    pub output_size: (i64, i64),
    /// Content-addressed baseline returned when the editor loaded its source.
    pub revision_sha256: String,
}

#[derive(Debug)]
pub struct ApprovedEditorSave {
    pub token: uuid::Uuid,
    pub path: PathBuf,
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
            completion_monitor: None,
            session_id: uuid::Uuid::nil(),
        }
    }
}

pub struct RecordingFlow {
    /// Stable identity for timer/finalization work owned by this session.
    pub session_id: uuid::Uuid,
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
    /// Display where the recording selection originated. The control panel is
    /// draggable, so its final display is not a reliable completion target.
    pub completion_monitor: Option<Monitor>,
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

    /// Transfers finalized segment files to the caller before a failed live
    /// segment resets the session. Keeping these paths out of the abandoned
    /// flow prevents generic cleanup from deleting already-valid recording
    /// data while it is being imported as a partial recording.
    pub fn take_completed_segments(&mut self) -> Vec<PathBuf> {
        std::mem::take(&mut self.segments)
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
    #[cfg_attr(windows, allow(dead_code))]
    pub display_id: u32,
    #[cfg_attr(not(windows), allow(dead_code))]
    pub display_identity: Option<crate::capture::DisplayIdentity>,
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
pub struct CompletionPreviewDto {
    pub id: String,
    pub phase: String,
    pub asset_id: Option<String>,
    pub kind: String,
    pub title: String,
    pub detail: String,
    pub gif_eligible: bool,
    pub copied: bool,
}

impl CompletionPreviewDto {
    pub fn processing(id: String, kind: &str, title: &str, detail: &str) -> Self {
        Self {
            id,
            phase: "processing".into(),
            asset_id: None,
            kind: kind.into(),
            title: title.into(),
            detail: detail.into(),
            gif_eligible: false,
            copied: false,
        }
    }

    pub fn ready(
        id: String,
        asset: &CaptureAsset,
        title: &str,
        detail: String,
        copied: bool,
    ) -> Self {
        Self {
            id,
            phase: "ready".into(),
            asset_id: Some(asset.id.to_string()),
            kind: asset.kind.as_str().into(),
            title: title.into(),
            detail,
            gif_eligible: asset.kind == crate::core::asset::CaptureKind::Video
                && crate::core::policy::RecordingPolicy::is_gif_eligible(asset.duration),
            copied,
        }
    }
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
        let config_dir = app.path().app_config_dir()?;
        let default_root = AssetLibrary::default_root_url()
            .or_else(|| dirs::data_local_dir().map(|dir| dir.join("kiri")))
            .ok_or_else(|| anyhow::anyhow!("the default Kiri library path is unavailable"))?;
        let library = LibraryContext::open(default_root, config_dir.clone())?;
        let recording_recovery = RecordingRecoveryStore::new(
            app.path().app_local_data_dir()?.join("Recording Recovery"),
        );
        let ocr_providers = Arc::new(crate::ocr_controller::OcrProviderManager::open(&config_dir));
        Ok(Self {
            library: std::sync::Mutex::new(library),
            library_transition: std::sync::Mutex::new(()),
            capture_start: Default::default(),
            capture: Default::default(),
            pending_capture_completion: Default::default(),
            recording: Default::default(),
            ffmpeg_path: std::sync::OnceLock::new(),
            saved_annotation_appearance: std::sync::Mutex::new(AnnotationAppearance::default()),
            saved_recording_options: std::sync::Mutex::new(RecordingOptions::default()),
            recording_recovery: std::sync::Mutex::new(recording_recovery),
            recording_recovery_transition: std::sync::Mutex::new(()),
            gif_conversion_ids: Default::default(),
            click_monitor: std::sync::Mutex::new(None),
            ocr_providers,
            ocr_requests: Arc::new(crate::ocr_controller::OcrRequestController::default()),
            editor_annotations: Default::default(),
            editor_save_destinations: Default::default(),
            remote_ocr: std::sync::OnceLock::new(),
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

    /// Remote OCR is opt-in, so avoid constructing TLS/proxy connection pools
    /// for the common local-only path. A failed initialization is cached too,
    /// preventing repeated setup work during one app session.
    pub fn remote_ocr(&self) -> Option<crate::remote_ocr::RemoteOcrClient> {
        self.remote_ocr
            .get_or_init(|| crate::remote_ocr::RemoteOcrClient::new().ok())
            .clone()
    }
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
    show_completion_toast(app, &notice, None);
}

/// Completion notice pinned to the monitor where the originating Kiri window
/// was shown. Capture and recording windows may be closed before the work
/// finishes, so their monitor must be retained by the caller.
pub fn emit_notice_on_monitor(
    app: &AppHandle,
    title: String,
    symbol: String,
    monitor: Option<Monitor>,
) {
    let notice = NoticeDto {
        id: uuid::Uuid::new_v4().to_string(),
        title,
        symbol,
    };
    show_completion_toast(app, &notice, monitor);
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
            let window = match builder.build() {
                Ok(window) => window,
                Err(error) => {
                    log::error!("[confirm] window creation failed: {error}");
                    return;
                }
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

/// Shows the notice as a borderless always-on-top toast near the top-center of
/// the display where the operation happened. Screenshots and recordings
/// return focus to the source application, so the library-window notice alone
/// is easy to miss. One resident window is reused and repositioned before
/// every notice, including after Kiri moves between displays.
fn show_completion_toast(app: &AppHandle, notice: &NoticeDto, monitor: Option<Monitor>) {
    let label = "toast";
    let window = match app.get_webview_window(label) {
        Some(window) => window,
        None => {
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
            .content_protected(true)
            .always_on_top(true)
            .skip_taskbar(true)
            .resizable(false)
            .focused(false)
            .visible(false)
            .inner_size(360.0, 60.0)
            .position(0.0, 0.0);
            let window = match builder.build() {
                Ok(window) => window,
                Err(error) => {
                    log::error!("[toast] passive window creation failed: {error}");
                    return;
                }
            };
            window
        }
    };

    let target_monitor = monitor
        .or_else(|| focused_kiri_monitor(app))
        .or_else(|| {
            let cursor = app.cursor_position().ok()?;
            app.monitor_from_point(cursor.x, cursor.y).ok().flatten()
        })
        .or_else(|| app.primary_monitor().ok().flatten());
    if let Some(monitor) = target_monitor {
        position_completion_toast(&window, &monitor, 60.0);
    }

    // Content is delivered via event so the resident window can update in
    // place (initial render reads the URL params above).
    let _ = window.set_ignore_cursor_events(true);
    let _ = window.set_content_protected(true);
    crate::platform::set_window_capture_excluded(app, label, true);
    let _ = window.emit("toast", notice.clone());
    crate::platform::show_window_without_activation(
        app,
        label,
        crate::platform::TransientWindowRole::CompletionFeedback,
    );
    log::info!("[toast] passive notice presented");
}

/// Shows an interactive completion card in the same resident global-feedback
/// window used by passive notices. The window is visible without taking focus;
/// it accepts pointer input only while the completion card is active.
pub fn show_completion_preview(
    app: &AppHandle,
    preview: &CompletionPreviewDto,
    monitor: Option<Monitor>,
) {
    let label = "toast";
    let asset_id = preview.asset_id.as_deref().unwrap_or_default();
    let initial_url = format!(
        "index.html?window=toast&mode=completion&id={}&phase={}&assetId={}&kind={}&title={}&detail={}&gifEligible={}&copied={}",
        urlencode(&preview.id),
        urlencode(&preview.phase),
        urlencode(asset_id),
        urlencode(&preview.kind),
        urlencode(&preview.title),
        urlencode(&preview.detail),
        preview.gif_eligible,
        preview.copied,
    );
    let window = match app.get_webview_window(label) {
        Some(window) => window,
        None => {
            let builder =
                WebviewWindowBuilder::new(app, label, WebviewUrl::App(initial_url.into()))
                    .title("kiri")
                    .decorations(false)
                    .transparent(true)
                    .shadow(false)
                    .content_protected(true)
                    .always_on_top(true)
                    .skip_taskbar(true)
                    .resizable(false)
                    .focused(false)
                    .visible(false)
                    .inner_size(360.0, 124.0)
                    .position(0.0, 0.0);
            let window = match builder.build() {
                Ok(window) => window,
                Err(error) => {
                    log::error!("[toast] completion window creation failed: {error}");
                    return;
                }
            };
            window
        }
    };

    let target_monitor = monitor
        .or_else(|| focused_kiri_monitor(app))
        .or_else(|| {
            let cursor = app.cursor_position().ok()?;
            app.monitor_from_point(cursor.x, cursor.y).ok().flatten()
        })
        .or_else(|| app.primary_monitor().ok().flatten());
    if let Some(monitor) = target_monitor {
        position_completion_toast(&window, &monitor, 124.0);
    }

    let _ = window.set_ignore_cursor_events(preview.phase == "processing");
    let _ = window.set_content_protected(true);
    crate::platform::set_window_capture_excluded(app, label, true);
    let _ = window.emit("completion-preview", preview.clone());
    crate::platform::show_window_without_activation(
        app,
        label,
        crate::platform::TransientWindowRole::CompletionFeedback,
    );
    log::info!(
        "[toast] completion preview presented phase={} kind={}",
        preview.phase,
        preview.kind
    );
}

fn focused_kiri_monitor(app: &AppHandle) -> Option<Monitor> {
    app.webview_windows()
        .into_iter()
        .filter(|(label, _)| label != "toast" && label != "ripple" && label != "confirm")
        .find_map(|(_, window)| {
            window
                .is_focused()
                .ok()
                .filter(|focused| *focused)
                .and_then(|_| window.current_monitor().ok().flatten())
        })
}

fn position_completion_toast(window: &WebviewWindow, monitor: &Monitor, height: f64) {
    let work_area = monitor.work_area();
    let position = toast_position(work_area.position, work_area.size, monitor.scale_factor());

    // Position first so the following logical size is resolved using the
    // destination display's scale factor on mixed-DPI setups.
    let _ = window.set_position(position);
    let _ = window.set_size(LogicalSize::new(360.0, height));
}

/// Top-center toast position in physical desktop coordinates. Including the
/// work-area origin keeps the toast on secondary displays (including displays
/// arranged left/above the primary), and using the destination scale keeps a
/// 360pt toast centered on Retina/mixed-DPI screens.
fn toast_position(
    work_area_position: PhysicalPosition<i32>,
    work_area_size: PhysicalSize<u32>,
    scale: f64,
) -> PhysicalPosition<i32> {
    let toast_width = (360.0 * scale).round() as i64;
    let available_width = i64::from(work_area_size.width);
    let centered_offset = ((available_width - toast_width) / 2).max(0);
    let x = i64::from(work_area_position.x) + centered_offset;
    let y = i64::from(work_area_position.y) + (20.0 * scale).round() as i64;
    PhysicalPosition::new(x as i32, y as i32)
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

pub fn emit_asset_content_changed(app: &AppHandle, asset_id: &uuid::Uuid) {
    let _ = app.emit("asset-content-changed", asset_id.to_string());
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

// Annotation appearance is shared by the capture overlay and editor. Keep it
// in the native config directory because Tauri WebViews do not share reliable
// localStorage across windows.
pub fn annotation_appearance_path(app: &AppHandle) -> PathBuf {
    app.path()
        .app_config_dir()
        .unwrap_or_else(|_| std::env::temp_dir())
        .join("annotation-appearance.json")
}

pub fn load_annotation_appearance(app: &AppHandle) -> AnnotationAppearance {
    std::fs::read(annotation_appearance_path(app))
        .ok()
        .and_then(|data| serde_json::from_slice::<AnnotationAppearance>(&data).ok())
        .unwrap_or_default()
        .normalized()
}

pub fn save_annotation_appearance(
    app: &AppHandle,
    appearance: &AnnotationAppearance,
) -> std::io::Result<()> {
    let path = annotation_appearance_path(app);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let data =
        serde_json::to_vec_pretty(&appearance.normalized()).map_err(std::io::Error::other)?;
    std::fs::write(path, data)
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
        CaptureStartGate, RecordingConfiguration, RecordingFlow,
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
        // Retina 3024×1964 @2x: the 360pt toast is 720 physical pixels wide.
        let position = toast_position(
            tauri::PhysicalPosition::new(0, 48),
            tauri::PhysicalSize::new(3024, 1916),
            2.0,
        );
        assert_eq!(position.x, (3024 - 720) / 2);
        assert_eq!(position.y, 48 + 40);

        // A secondary display to the left keeps its negative desktop origin.
        let position = toast_position(
            tauri::PhysicalPosition::new(-1920, 0),
            tauri::PhysicalSize::new(1920, 1040),
            1.0,
        );
        assert_eq!(position.x, -1920 + (1920 - 360) / 2);
        assert_eq!(position.y, 20);

        // Small screens clamp the centering offset instead of moving farther
        // left than the display's own origin.
        let position = toast_position(
            tauri::PhysicalPosition::new(200, 100),
            tauri::PhysicalSize::new(320, 240),
            1.0,
        );
        assert_eq!(position.x, 200);
        assert_eq!(position.y, 120);
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
    fn capture_start_gate_rejects_overlap_and_reopens_after_drop() {
        let gate = CaptureStartGate::default();
        let permit = gate.try_begin().expect("first capture start must enter");
        assert!(gate.try_begin().is_none());
        drop(permit);
        assert!(gate.try_begin().is_some());
    }

    #[test]
    fn final_destroyed_overlay_consumes_capture_session() {
        let capture_id = uuid::Uuid::new_v4();
        let mut flow = CaptureFlow {
            session: Some(CaptureSession {
                capture_id,
                display: CapturedDisplay {
                    png_data: vec![1].into(),
                    pixel_width: 1,
                    pixel_height: 1,
                    screen_frame: Rect::new(0.0, 0.0, 1.0, 1.0),
                    window_rects: Vec::new(),
                    display_id: 1,
                    display_identity: None,
                    backing_scale: 1.0,
                },
                source_application: None,
                return_pid: None,
                was_kiri_frontmost: true,
                hidden_windows: Vec::new(),
                overlay_labels: vec!["overlay-a".into(), "overlay-b".into()],
                annotation: None,
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
                display_identity: None,
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
                display_identity: None,
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
    fn completed_segments_can_be_transferred_before_failure_cleanup() {
        let first = std::path::PathBuf::from("completed-1.mp4");
        let second = std::path::PathBuf::from("completed-2.mp4");
        let mut flow = RecordingFlow {
            is_transitioning: true,
            segments: vec![first.clone(), second.clone()],
            active: Some(ActiveRecording::default()),
            ..Default::default()
        };

        let recoverable = flow.take_completed_segments();
        let abandoned = flow.take_and_reset();

        assert_eq!(recoverable, vec![first, second]);
        assert!(abandoned.segments.is_empty());
        assert!(abandoned.active.is_some());
    }

    #[test]
    fn cancelled_startup_cannot_attach_to_a_replacement_session() {
        let configuration = RecordingConfiguration {
            display_id: 11,
            display_identity: None,
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
                display_identity: None,
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
