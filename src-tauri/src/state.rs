//! AppModel equivalent — coordinates capture, library operations, recording
//! state, and transient feedback across the Tauri windows.

use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Instant;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::capture::{CapturedDisplay, PlatformRecorder};
use crate::core::asset::{CaptureAsset, CaptureKind};
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
            click_monitor: None,
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
    pub click_monitor: Option<Box<dyn crate::platform::ClickMonitorHandle + Send>>,
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
    pub backing_scale: f64,
    pub options: RecordingOptions,
}

pub struct ActiveRecording {
    pub encoder: Option<SegmentEncoder>,
    pub recorder: Option<Box<dyn PlatformRecorder + Send>>,
}

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
    let _ = app.emit("notice", notice);
}

pub fn emit_error(app: &AppHandle, message: String, recovery: Option<RecoveryAction>) {
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

/// Converts a top-left display-local region (points) to its pixel rect within
/// the frozen capture image.
pub fn pixel_rect_for_region(
    display: &CapturedDisplay,
    region: &crate::core::geometry::Rect,
) -> crate::core::geometry::Rect {
    let scale_x = display.pixel_width as f64 / display.screen_frame.width.max(1.0);
    let scale_y = display.pixel_height as f64 / display.screen_frame.height.max(1.0);
    crate::core::geometry::Rect::new(
        region.x * scale_x,
        region.y * scale_y,
        region.width * scale_x,
        region.height * scale_y,
    )
    .integral()
}

pub fn asset_kind_label(kind: CaptureKind) -> &'static str {
    match kind {
        CaptureKind::Image => "image",
        CaptureKind::Video => "video",
        CaptureKind::Gif => "gif",
    }
}
