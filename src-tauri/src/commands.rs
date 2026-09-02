//! Tauri command surface and application orchestration.
//! Synchronous commands run on the main thread; heavy work is spawned onto
//! background threads.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use serde::{Deserialize, Serialize};
use tauri::{
    AppHandle, Emitter, Manager, Monitor, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};

use crate::capture::current as capture_backend;
use crate::core::annotation::{AnnotationAppearance, AnnotationDocument, AnnotationPixelSize};
use crate::core::asset::{CaptureAsset, CaptureKind};
use crate::core::geometry::Rect;
use crate::core::library::{AssetLibraryError, EditorAnnotationState};
use crate::core::library_location::{
    self, LibraryAvailability, LibraryLocationError, LibraryStatusSnapshot,
};
use crate::core::policy::{RecordingOptions, RecordingOutputFormat};
use crate::core::recording_recovery::PendingRecording;
use crate::core::shortcut::KIRI_CAPTURE;
use crate::platform;
#[cfg(target_os = "macos")]
use crate::state::RecoveryAction;
use crate::state::{
    emit_asset_content_changed, emit_error, emit_library_changed, emit_notice, emit_notice_local,
    emit_notice_on_monitor, emit_recording_state, show_completion_preview, ActiveRecording,
    AppState, ApprovedEditorSave, CaptureSession, CompletionPreviewDto, PendingCaptureCompletion,
    RecordingConfiguration, RecordingFlow, StagedCaptureAnnotation, StagedEditorAnnotation,
};

// ---------------------------------------------------------------------------
// DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RectDto {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct GifConversionStateDto {
    id: String,
    is_converting: bool,
}

impl From<&Rect> for RectDto {
    fn from(rect: &Rect) -> Self {
        Self {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: rect.height,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureContextDto {
    pub display_width: f64,
    pub display_height: f64,
    pub scale: f64,
    pub pixel_width: i64,
    pub pixel_height: i64,
    pub window_rects: Vec<RectDto>,
    pub source_application: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetDto {
    pub id: String,
    pub kind: String,
    pub created_at: f64,
    pub filename: String,
    pub title: Option<String>,
    pub tags: Vec<String>,
    pub pixel_width: i64,
    pub pixel_height: i64,
    pub duration: Option<f64>,
    pub source_application: Option<String>,
    pub is_favorite: bool,
    pub trashed_at: Option<f64>,
    pub gif_eligible: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryStatusDto {
    location_label: String,
    is_default: bool,
    availability: LibraryAvailability,
}

impl From<LibraryStatusSnapshot> for LibraryStatusDto {
    fn from(status: LibraryStatusSnapshot) -> Self {
        Self {
            location_label: status.location_label,
            is_default: status.is_default,
            availability: status.availability,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetAvailabilityDto {
    status: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingRecordingDto {
    id: String,
    created_at: f64,
}

fn asset_dto(asset: &CaptureAsset) -> AssetDto {
    AssetDto {
        id: asset.id.to_string(),
        kind: asset.kind.as_str().to_string(),
        created_at: asset.created_at,
        filename: asset.filename.clone(),
        title: asset.title.clone(),
        tags: asset.tags.clone(),
        pixel_width: asset.pixel_width,
        pixel_height: asset.pixel_height,
        duration: asset.duration,
        source_application: asset.source_application.clone(),
        is_favorite: asset.is_favorite,
        trashed_at: asset.trashed_at,
        gif_eligible: asset.kind == CaptureKind::Video
            && crate::core::policy::RecordingPolicy::is_gif_eligible(asset.duration),
    }
}

fn completion_asset_detail(asset: &CaptureAsset) -> String {
    let format = match asset.kind {
        CaptureKind::Image => "PNG",
        CaptureKind::Video => "MP4",
        CaptureKind::Gif => "GIF",
    };
    let mut parts = vec![format.to_string()];
    if let Some(duration) = asset.duration {
        parts.push(crate::core::policy::RecordingPolicy::elapsed_label(
            duration,
        ));
    }
    if asset.pixel_width > 0 && asset.pixel_height > 0 {
        parts.push(format!("{} × {}", asset.pixel_width, asset.pixel_height));
    }
    parts.join(" · ")
}

// ---------------------------------------------------------------------------
// Library commands (main thread)
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn list_assets(
    app: AppHandle,
    query: String,
    showing_trash: bool,
) -> Result<Vec<AssetDto>, String> {
    let state = app.state::<AppState>();
    let mut context = state.library.lock().unwrap();
    let library = context.library().map_err(|error| error.to_string())?;
    let assets = library.search(&query, showing_trash);
    Ok(assets.iter().map(asset_dto).collect())
}

fn with_asset_mutation(
    app: &AppHandle,
    id: &str,
    mutation: impl FnOnce(&mut crate::core::library::AssetLibrary, &uuid::Uuid) -> Result<(), String>,
) -> Result<(), String> {
    let parsed = uuid::Uuid::parse_str(id).map_err(|e| e.to_string())?;
    let state = app.state::<AppState>();
    let mut context = state.library.lock().unwrap();
    let library = context.library_mut().map_err(|error| error.to_string())?;
    mutation(library, &parsed)?;
    drop(context);
    emit_library_changed(app);
    Ok(())
}

#[tauri::command]
pub fn set_favorite(app: AppHandle, id: String, favorite: bool) -> Result<(), String> {
    with_asset_mutation(&app, &id, |library, parsed| {
        library
            .set_favorite(favorite, parsed)
            .map_err(|e| e.to_string())
    })
}

#[tauri::command]
pub fn move_to_trash(app: AppHandle, window: WebviewWindow, id: String) -> Result<(), String> {
    with_asset_mutation(&app, &id, |library, parsed| {
        library.move_to_trash(parsed).map_err(|e| e.to_string())
    })?;
    if window.label() != "toast" {
        emit_notice_local(&app, "Moved to Trash".into(), "trash".into());
    }
    Ok(())
}

#[tauri::command]
pub fn restore_asset(app: AppHandle, window: WebviewWindow, id: String) -> Result<(), String> {
    with_asset_mutation(&app, &id, |library, parsed| {
        library.restore(parsed).map_err(|e| e.to_string())
    })?;
    if window.label() != "toast" {
        emit_notice_local(
            &app,
            "Restored to Library".into(),
            "arrow.uturn.backward".into(),
        );
    }
    Ok(())
}

#[tauri::command]
pub fn permanently_delete(app: AppHandle, id: String) -> Result<(), String> {
    let parsed = uuid::Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    let state = app.state::<AppState>();
    let store = app.state::<crate::protocol::ProtocolStore>();
    let result: Result<(), LibraryLocationError> =
        crate::protocol::with_thumbnail_invalidation(&store, parsed, || {
            let mut context = state.library.lock().unwrap();
            context.library_mut()?.permanently_delete(&parsed)?;
            Ok(())
        });
    if result.is_ok()
        || matches!(
            &result,
            Err(LibraryLocationError::Library(
                AssetLibraryError::CleanupFailed { .. }
            ))
        )
    {
        emit_library_changed(&app);
    }
    result.map_err(|error| error.to_string())?;
    emit_notice_local(&app, "Deleted Permanently".into(), "trash.fill".into());
    Ok(())
}

#[tauri::command]
pub fn empty_trash(app: AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let removed_ids = {
        let mut context = state.library.lock().unwrap();
        context
            .library()
            .map_err(|error| error.to_string())?
            .all_assets(true)
            .into_iter()
            .map(|asset| asset.id)
            .collect::<Vec<_>>()
    };
    let store = app.state::<crate::protocol::ProtocolStore>();
    let result: Result<(), LibraryLocationError> =
        crate::protocol::with_thumbnail_invalidations(&store, &removed_ids, || {
            let mut context = state.library.lock().unwrap();
            context.library_mut()?.empty_trash()?;
            Ok(())
        });
    if result.is_ok()
        || matches!(
            &result,
            Err(LibraryLocationError::Library(
                AssetLibraryError::CleanupFailed { .. }
            ))
        )
    {
        emit_library_changed(&app);
    }
    result.map_err(|error| error.to_string())?;
    emit_notice_local(&app, "Trash Emptied".into(), "trash.slash".into());
    Ok(())
}

/// Parses a batch of asset ids (frontend sends a JSON array of uuids).
fn parse_ids(ids: &[String]) -> Result<Vec<uuid::Uuid>, String> {
    ids.iter()
        .map(|id| uuid::Uuid::parse_str(id).map_err(|e| e.to_string()))
        .collect()
}

#[tauri::command]
pub fn batch_move_to_trash(app: AppHandle, ids: Vec<String>) -> Result<(), String> {
    let parsed = parse_ids(&ids)?;
    {
        let state = app.state::<AppState>();
        let mut context = state.library.lock().unwrap();
        context
            .library_mut()
            .map_err(|error| error.to_string())?
            .batch_move_to_trash(&parsed)
            .map_err(|e| e.to_string())?;
    }
    emit_library_changed(&app);
    Ok(())
}

#[tauri::command]
pub fn batch_restore(app: AppHandle, ids: Vec<String>) -> Result<(), String> {
    let parsed = parse_ids(&ids)?;
    {
        let state = app.state::<AppState>();
        let mut context = state.library.lock().unwrap();
        context
            .library_mut()
            .map_err(|error| error.to_string())?
            .batch_restore(&parsed)
            .map_err(|e| e.to_string())?;
    }
    emit_library_changed(&app);
    Ok(())
}

#[tauri::command]
pub fn batch_permanently_delete(app: AppHandle, ids: Vec<String>) -> Result<(), String> {
    let parsed = parse_ids(&ids)?;
    let state = app.state::<AppState>();
    let store = app.state::<crate::protocol::ProtocolStore>();
    let result: Result<(), LibraryLocationError> =
        crate::protocol::with_thumbnail_invalidations(&store, &parsed, || {
            let mut context = state.library.lock().unwrap();
            context.library_mut()?.batch_permanently_delete(&parsed)?;
            Ok(())
        });
    if result.is_ok()
        || matches!(
            &result,
            Err(LibraryLocationError::Library(
                AssetLibraryError::CleanupFailed { .. }
            ))
        )
    {
        emit_library_changed(&app);
    }
    result.map_err(|error| error.to_string())
}

#[tauri::command]
pub fn batch_set_favorite(app: AppHandle, ids: Vec<String>, favorite: bool) -> Result<(), String> {
    let parsed = parse_ids(&ids)?;
    {
        let state = app.state::<AppState>();
        let mut context = state.library.lock().unwrap();
        context
            .library_mut()
            .map_err(|error| error.to_string())?
            .batch_set_favorite(favorite, &parsed)
            .map_err(|e| e.to_string())?;
    }
    emit_library_changed(&app);
    Ok(())
}

#[tauri::command]
pub fn copy_asset(app: AppHandle, window: WebviewWindow, id: String) -> Result<(), String> {
    let parsed = uuid::Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    let state = app.state::<AppState>();
    let (asset, path) = {
        let mut context = state.library.lock().unwrap();
        let library = context.library().map_err(|error| error.to_string())?;
        let asset = library
            .asset_by_id(&parsed)
            .cloned()
            .ok_or_else(|| "The capture could not be found.".to_string())?;
        let path = library
            .readable_asset_url(&asset)
            .map_err(|error| error.to_string())?;
        (asset, path)
    };
    match asset.kind {
        CaptureKind::Image => {
            let data = std::fs::read(&path).map_err(|e| e.to_string())?;
            platform::write_image_to_clipboard(&data).map_err(|e| e.to_string())?;
        }
        CaptureKind::Video | CaptureKind::Gif => {
            platform::write_file_to_clipboard(&path).map_err(|e| e.to_string())?;
        }
    }
    if window.label() != "toast" {
        emit_notice_local(
            &app,
            "Copied to Clipboard".into(),
            "checkmark.circle.fill".into(),
        );
    }
    Ok(())
}

#[tauri::command]
pub fn open_asset(app: AppHandle, id: String) -> Result<(), String> {
    let parsed = uuid::Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    let state = app.state::<AppState>();
    let mut context = state.library.lock().unwrap();
    let asset = context
        .library()
        .map_err(|error| error.to_string())?
        .asset_by_id(&parsed)
        .cloned()
        .ok_or_else(|| "The capture could not be found.".to_string())?;
    let (width, height) = (asset.pixel_width, asset.pixel_height);
    drop(context);

    // In-app viewer window (image preview / video player), Esc to close.
    let label = format!("viewer-{}", asset.id.to_string().to_lowercase());
    if let Some(window) = app.get_webview_window(&label) {
        let _ = window.show();
        let _ = window.set_focus();
        return Ok(());
    }
    // Size the window to the asset's aspect ratio (clamped), so an image
    // opens close to its natural shape without covering the screen.
    let aspect = if width > 0 && height > 0 {
        (width as f64 / height as f64).clamp(0.4, 2.6)
    } else {
        1.5
    };
    let win_h = 640.0f64;
    let win_w = (win_h * aspect).clamp(360.0, 1200.0);
    let builder = WebviewWindowBuilder::new(
        &app,
        label,
        WebviewUrl::App(format!("index.html?window=viewer&id={}", asset.id).into()),
    )
    .title("kiri")
    .inner_size(win_w, win_h)
    .resizable(true)
    .shadow(true)
    .decorations(true);
    let _ = builder.build();
    Ok(())
}

#[tauri::command]
pub fn open_editor(app: AppHandle, id: String) -> Result<(), String> {
    let parsed =
        uuid::Uuid::parse_str(&id).map_err(|_| "The capture id is invalid.".to_string())?;
    let state = app.state::<AppState>();
    let mut context = state.library.lock().unwrap();
    let asset = context
        .library()
        .map_err(|error| error.to_string())?
        .asset_by_id(&parsed)
        .cloned()
        .ok_or_else(|| "The capture could not be found.".to_string())?;
    if asset.kind != CaptureKind::Image {
        return Err("Only image captures can be edited.".into());
    }
    drop(context);

    let label = format!("editor-{}", asset.id.to_string().to_lowercase());
    if let Some(window) = app.get_webview_window(&label) {
        let _ = window.show();
        let _ = window.set_focus();
        return Ok(());
    }
    WebviewWindowBuilder::new(
        &app,
        label,
        WebviewUrl::App(format!("index.html?window=editor&id={}", asset.id).into()),
    )
    .title("kiri")
    .inner_size(880.0, 620.0)
    .min_inner_size(520.0, 420.0)
    .resizable(true)
    .shadow(true)
    .decorations(true)
    .build()
    .map_err(|error| format!("The screenshot editor could not be opened: {error}"))?;
    Ok(())
}

#[tauri::command]
pub fn get_asset(app: AppHandle, id: String) -> Result<AssetDto, String> {
    let parsed = uuid::Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    let state = app.state::<AppState>();
    let mut context = state.library.lock().unwrap();
    let library = context.library().map_err(|error| error.to_string())?;
    let asset = library
        .asset_by_id(&parsed)
        .cloned()
        .ok_or_else(|| "The capture could not be found.".to_string())?;
    Ok(asset_dto(&asset))
}

#[tauri::command]
pub fn reveal_asset(app: AppHandle, id: String) -> Result<(), String> {
    let parsed = uuid::Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    let state = app.state::<AppState>();
    let path = {
        let mut context = state.library.lock().unwrap();
        let library = context.library().map_err(|error| error.to_string())?;
        let asset = library
            .asset_by_id(&parsed)
            .cloned()
            .ok_or_else(|| "The capture could not be found.".to_string())?;
        library
            .readable_asset_url(&asset)
            .map_err(|error| error.to_string())?
    };
    platform::reveal_path(&path);
    Ok(())
}

#[tauri::command]
pub fn get_library_status(app: AppHandle) -> LibraryStatusDto {
    let state = app.state::<AppState>();
    let status = state.library.lock().unwrap().status();
    status.into()
}

#[tauri::command]
pub fn get_asset_availability(app: AppHandle, id: String) -> Result<AssetAvailabilityDto, String> {
    let parsed = uuid::Uuid::parse_str(&id).map_err(|error| error.to_string())?;
    let state = app.state::<AppState>();
    let mut context = state.library.lock().unwrap();
    let status = match context.library() {
        Ok(library) => match library
            .asset_availability(&parsed)
            .map_err(|error| error.to_string())?
        {
            crate::core::library::AssetAvailability::Ready => "ready",
            crate::core::library::AssetAvailability::Missing => "missing",
            crate::core::library::AssetAvailability::Unreadable => "unreadable",
        },
        Err(LibraryLocationError::Unavailable | LibraryLocationError::Migrating) => {
            "libraryUnavailable"
        }
        Err(error) => return Err(error.to_string()),
    };
    Ok(AssetAvailabilityDto { status })
}

#[tauri::command]
pub fn reveal_library(app: AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let root = {
        let mut context = state.library.lock().unwrap();
        context.library().map_err(|error| error.to_string())?;
        context.root().to_path_buf()
    };
    platform::reveal_path(&root);
    Ok(())
}

#[tauri::command]
pub fn retry_library(app: AppHandle) -> Result<LibraryStatusDto, String> {
    let state = app.state::<AppState>();
    let mut context = state.library.lock().unwrap();
    context.retry().map_err(|error| error.to_string())?;
    let status = context.status().into();
    drop(context);
    crate::protocol::clear_thumbnails(&app.state::<crate::protocol::ProtocolStore>());
    emit_library_changed(&app);
    Ok(status)
}

async fn pick_library_folder(app: &AppHandle) -> Result<Option<PathBuf>, String> {
    use tauri_plugin_dialog::DialogExt;
    let dialog_app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        dialog_app
            .dialog()
            .file()
            .blocking_pick_folder()
            .and_then(|path| path.into_path().ok())
    })
    .await
    .map_err(|error| format!("Could not open the folder picker: {error}"))
}

fn ensure_library_location_change_idle(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    if state.capture.lock().unwrap().session.is_some() {
        return Err("Finish the current capture before moving the library.".into());
    }
    let recording = state.recording.lock().unwrap();
    if recording.is_starting
        || recording.is_recording
        || recording.is_paused
        || recording.is_transitioning
        || recording.is_finalizing
    {
        return Err("Finish the current recording before moving the library.".into());
    }
    Ok(())
}

async fn migrate_active_library(
    app: &AppHandle,
    target_root: PathBuf,
    replace_same_library: bool,
) -> Result<LibraryStatusDto, String> {
    let state = app.state::<AppState>();
    let transition = state.library_transition.lock().unwrap();
    ensure_library_location_change_idle(app)?;
    let source = {
        let mut context = state.library.lock().unwrap();
        if context.root_matches(&target_root) {
            context.retry().map_err(|error| error.to_string())?;
            return Ok(context.status().into());
        }
        context
            .begin_migration()
            .map_err(|error| error.to_string())?
    };
    drop(transition);
    emit_library_changed(app);
    let prepared = tauri::async_runtime::spawn_blocking(move || {
        library_location::migrate_library(&source, &target_root, replace_same_library)
    })
    .await;
    let state = app.state::<AppState>();
    let mut context = state.library.lock().unwrap();
    match prepared {
        Ok(Ok(prepared)) => {
            context
                .activate_prepared(prepared)
                .map_err(|error| error.to_string())?;
            let status = context.status().into();
            drop(context);
            crate::protocol::clear_thumbnails(&app.state::<crate::protocol::ProtocolStore>());
            emit_library_changed(app);
            Ok(status)
        }
        Ok(Err(error)) => {
            context.cancel_migration();
            drop(context);
            emit_library_changed(app);
            Err(error.to_string())
        }
        Err(error) => {
            context.cancel_migration();
            drop(context);
            emit_library_changed(app);
            Err(format!("The library move did not finish: {error}"))
        }
    }
}

#[tauri::command]
pub async fn choose_library_location(app: AppHandle) -> Result<LibraryStatusDto, String> {
    let Some(container) = pick_library_folder(&app).await? else {
        return Ok(get_library_status(app));
    };
    let target = library_location::target_root_for_container(&container)
        .map_err(|error| error.to_string())?;
    if target.exists() {
        let availability = {
            let state = app.state::<AppState>();
            let availability = state.library.lock().unwrap().status().availability;
            availability
        };
        match availability {
            LibraryAvailability::Ready => migrate_active_library(&app, target, true).await,
            LibraryAvailability::Unavailable => switch_to_existing_library(&app, target).await,
            LibraryAvailability::Migrating => Err("library storage is being moved".into()),
        }
    } else {
        migrate_active_library(&app, target, false).await
    }
}

#[tauri::command]
pub async fn restore_default_library(app: AppHandle) -> Result<LibraryStatusDto, String> {
    let (target, availability) = {
        let state = app.state::<AppState>();
        let mut context = state.library.lock().unwrap();
        if context.is_default() {
            context.retry().map_err(|error| error.to_string())?;
            return Ok(context.status().into());
        }
        (
            context.default_root().to_path_buf(),
            context.status().availability,
        )
    };
    match availability {
        LibraryAvailability::Ready => migrate_active_library(&app, target, true).await,
        LibraryAvailability::Unavailable => {
            Err("Reconnect the library before restoring the default location.".into())
        }
        LibraryAvailability::Migrating => Err("library storage is being moved".into()),
    }
}

#[tauri::command]
pub async fn locate_library(app: AppHandle) -> Result<LibraryStatusDto, String> {
    let Some(selected) = pick_library_folder(&app).await? else {
        return Ok(get_library_status(app));
    };
    switch_to_existing_library(&app, selected).await
}

async fn switch_to_existing_library(
    app: &AppHandle,
    selected: PathBuf,
) -> Result<LibraryStatusDto, String> {
    let state = app.state::<AppState>();
    let transition = state.library_transition.lock().unwrap();
    ensure_library_location_change_idle(app)?;
    let (expected_id, expected_generation, expected_root) = {
        let mut context = state.library.lock().unwrap();
        if context.status().availability != LibraryAvailability::Unavailable {
            return Err("Locate is available only while the library is unavailable.".into());
        }
        if context.root_matches(&selected) {
            context.retry().map_err(|error| error.to_string())?;
            return Ok(context.status().into());
        }
        context
            .begin_locating()
            .map_err(|error| error.to_string())?;
        (
            context.expected_library_id(),
            context.expected_library_generation(),
            context.root().to_path_buf(),
        )
    };
    drop(transition);
    emit_library_changed(app);
    let prepared = tauri::async_runtime::spawn_blocking(move || {
        library_location::prepare_existing_location(
            &selected,
            expected_id,
            expected_generation,
            &expected_root,
        )
    })
    .await;
    let state = app.state::<AppState>();
    let mut context = state.library.lock().unwrap();
    match prepared {
        Ok(Ok(prepared)) => {
            context
                .activate_prepared(prepared)
                .map_err(|error| error.to_string())?;
            let status = context.status().into();
            drop(context);
            crate::protocol::clear_thumbnails(&app.state::<crate::protocol::ProtocolStore>());
            emit_library_changed(app);
            Ok(status)
        }
        Ok(Err(error)) => {
            context.cancel_locating();
            drop(context);
            emit_library_changed(app);
            Err(error.to_string())
        }
        Err(error) => {
            context.cancel_locating();
            drop(context);
            emit_library_changed(app);
            Err(format!(
                "The selected library could not be checked: {error}"
            ))
        }
    }
}

#[tauri::command]
pub async fn restore_missing_asset(app: AppHandle, id: String) -> Result<bool, String> {
    use tauri_plugin_dialog::DialogExt;
    let parsed = uuid::Uuid::parse_str(&id).map_err(|error| error.to_string())?;
    let (filter_label, extension, expected_asset) = {
        let state = app.state::<AppState>();
        let mut context = state.library.lock().unwrap();
        let library = context.library().map_err(|error| error.to_string())?;
        let asset = library
            .asset_by_id(&parsed)
            .cloned()
            .ok_or_else(|| "The capture could not be found.".to_string())?;
        if library
            .asset_availability(&parsed)
            .map_err(|error| error.to_string())?
            != crate::core::library::AssetAvailability::Missing
        {
            return Err("The capture file is not missing.".into());
        }
        let (label, extension) = match asset.kind {
            CaptureKind::Image => ("PNG image", "png"),
            CaptureKind::Video => ("MP4 video", "mp4"),
            CaptureKind::Gif => ("GIF image", "gif"),
        };
        (label, extension, asset)
    };
    let dialog_app = app.clone();
    let selected = tauri::async_runtime::spawn_blocking(move || {
        dialog_app
            .dialog()
            .file()
            .add_filter(filter_label, &[extension])
            .blocking_pick_file()
            .and_then(|path| path.into_path().ok())
    })
    .await
    .map_err(|error| format!("Could not open the file picker: {error}"))?;
    let Some(selected) = selected else {
        return Ok(false);
    };
    let validation_path = selected.clone();
    let validation_asset = expected_asset.clone();
    let proof = tauri::async_runtime::spawn_blocking(move || {
        let proof =
            crate::core::library::replacement_file_proof(&validation_path, validation_asset.kind)
                .map_err(|error| error.to_string())?;
        validate_replacement_metadata(&validation_asset, &validation_path)?;
        Ok::<_, String>(proof)
    })
    .await
    .map_err(|error| format!("Could not validate the selected file: {error}"))??;
    let state = app.state::<AppState>();
    let mut context = state.library.lock().unwrap();
    context
        .library_mut()
        .map_err(|error| error.to_string())?
        .restore_missing_asset(&parsed, &selected, &proof)
        .map_err(|error| error.to_string())?;
    drop(context);
    crate::protocol::clear_thumbnail(&app.state::<crate::protocol::ProtocolStore>(), parsed);
    emit_asset_content_changed(&app, &parsed);
    emit_library_changed(&app);
    Ok(true)
}

fn validate_replacement_metadata(asset: &CaptureAsset, path: &Path) -> Result<(), String> {
    let (pixel_width, pixel_height, duration) = match asset.kind {
        CaptureKind::Image => {
            let reader = image::ImageReader::open(path)
                .map_err(|_| "The selected PNG could not be read.".to_string())?
                .with_guessed_format()
                .map_err(|_| "The selected PNG could not be read.".to_string())?;
            if reader.format() != Some(image::ImageFormat::Png) {
                return Err("The selected file is not a PNG image.".into());
            }
            let decoded = reader
                .decode()
                .map_err(|_| "The selected PNG is invalid.".to_string())?;
            (
                i64::from(decoded.width()),
                i64::from(decoded.height()),
                None,
            )
        }
        #[cfg(windows)]
        CaptureKind::Video => {
            let (width, height) = crate::gif::video_dimensions(path)
                .map_err(|_| "The selected video file is invalid.".to_string())?;
            (i64::from(width), i64::from(height), asset.duration)
        }
        #[cfg(windows)]
        CaptureKind::Gif => {
            let reader = image::ImageReader::open(path)
                .map_err(|_| "The selected GIF could not be read.".to_string())?
                .with_guessed_format()
                .map_err(|_| "The selected GIF could not be read.".to_string())?;
            if reader.format() != Some(image::ImageFormat::Gif) {
                return Err("The selected file is not a GIF image.".into());
            }
            let decoded = reader
                .decode()
                .map_err(|_| "The selected GIF is invalid.".to_string())?;
            (
                i64::from(decoded.width()),
                i64::from(decoded.height()),
                asset.duration,
            )
        }
        #[cfg(not(windows))]
        CaptureKind::Video | CaptureKind::Gif => {
            let ffmpeg = crate::record::existing_ffmpeg()
                .ok_or_else(|| "A local FFmpeg is required to validate this file.".to_string())?;
            crate::record::probe_video(&ffmpeg, path)
                .ok_or_else(|| "The selected media file is invalid.".to_string())?
        }
    };
    if (asset.pixel_width > 0 && pixel_width != asset.pixel_width)
        || (asset.pixel_height > 0 && pixel_height != asset.pixel_height)
        || asset
            .duration
            .is_some_and(|expected| duration.is_none_or(|actual| (actual - expected).abs() > 0.25))
    {
        return Err("The selected file does not match this library item.".into());
    }
    Ok(())
}

#[tauri::command]
pub fn remove_missing_asset(app: AppHandle, id: String) -> Result<(), String> {
    let parsed = uuid::Uuid::parse_str(&id).map_err(|error| error.to_string())?;
    let state = app.state::<AppState>();
    let store = app.state::<crate::protocol::ProtocolStore>();
    crate::protocol::with_thumbnail_invalidation(&store, parsed, || {
        let mut context = state.library.lock().unwrap();
        context
            .library_mut()
            .map_err(|error| error.to_string())?
            .remove_missing_asset(&parsed)
            .map_err(|error| error.to_string())
    })?;
    emit_library_changed(&app);
    Ok(())
}

#[tauri::command]
pub async fn list_pending_recordings(app: AppHandle) -> Result<Vec<PendingRecordingDto>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let Ok(_transition) = state.recording_recovery_transition.try_lock() else {
            return Ok(Vec::new());
        };
        let pending = state.recording_recovery.lock().unwrap().list();
        Ok(pending
            .into_iter()
            .map(|pending| PendingRecordingDto {
                id: pending.id.to_string(),
                created_at: pending.created_at,
            })
            .collect())
    })
    .await
    .map_err(|error| format!("Could not check pending recordings: {error}"))?
}

#[tauri::command]
pub async fn retry_pending_recordings(app: AppHandle) -> Result<usize, String> {
    tauri::async_runtime::spawn_blocking(move || retry_pending_recordings_inner(&app))
        .await
        .map_err(|error| format!("Could not retry pending recordings: {error}"))?
}

fn retry_pending_recordings_inner(app: &AppHandle) -> Result<usize, String> {
    let state = app.state::<AppState>();
    let _transition = state
        .recording_recovery_transition
        .try_lock()
        .map_err(|_| "A recording is still being saved.".to_string())?;
    let pending_items = state.recording_recovery.lock().unwrap().list();
    if pending_items.is_empty() {
        return Ok(0);
    }

    let mut imported_count = 0;
    let mut completed_count = 0;
    let mut first_error = None;
    #[cfg(not(windows))]
    let mut ffmpeg = None;
    for mut pending in pending_items {
        let result = (|| -> Result<bool, String> {
            let video_path = state
                .recording_recovery
                .lock()
                .unwrap()
                .validate_video(&pending)
                .map_err(|error| error.to_string())?;
            let asset_state = {
                let store = state.recording_recovery.lock().unwrap();
                recovery_asset_state(&state, &store, &pending)?
            };
            match asset_state {
                RecoveryAssetState::Matching => {
                    cleanup_verified_recovery(&state, &mut pending)?;
                    return Ok(false);
                }
                RecoveryAssetState::Conflict => {
                    return Err(
                        "The active library contains a different item with this recording id."
                            .into(),
                    )
                }
                RecoveryAssetState::Absent | RecoveryAssetState::RestorableVideo => {}
            }

            state
                .recording_recovery
                .lock()
                .unwrap()
                .prepare_import(&mut pending, CaptureKind::Video, &video_path)
                .map_err(|error| error.to_string())?;
            #[cfg(windows)]
            let (pixel_width, pixel_height, duration) = {
                let (width, height) =
                    crate::gif::video_dimensions(&video_path).map_err(|error| error.to_string())?;
                (i64::from(width), i64::from(height), pending.duration)
            };
            #[cfg(not(windows))]
            let (pixel_width, pixel_height, duration) = crate::record::probe_video(
                match &ffmpeg {
                    Some(path) => path,
                    None => ffmpeg.insert(state.ffmpeg().map_err(|error| error.to_string())?),
                },
                &video_path,
            )
            .ok_or_else(|| "The pending recording is not a valid MP4.".to_string())?;
            import_recovery_output(
                &state,
                &video_path,
                &pending,
                CaptureKind::Video,
                "mp4",
                pixel_width,
                pixel_height,
                duration.or(pending.duration),
            )?;
            cleanup_verified_recovery(&state, &mut pending)?;
            Ok(true)
        })();

        match result {
            Ok(imported) => {
                completed_count += 1;
                imported_count += usize::from(imported);
            }
            Err(error) => {
                log::error!("recording recovery {} failed: {error}", pending.id);
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
    }

    if imported_count > 0 {
        emit_library_changed(app);
    }
    if completed_count == 0 {
        return Err(first_error.unwrap_or_else(|| "No recording could be recovered.".into()));
    }
    Ok(imported_count)
}

fn cleanup_verified_recovery(
    state: &AppState,
    pending: &mut PendingRecording,
) -> Result<(), String> {
    let store = state.recording_recovery.lock().unwrap();
    let mut context = state.library.lock().unwrap();
    let library = context.library().map_err(|error| error.to_string())?;
    store
        .finish_verified_import(library, pending)
        .map_err(|error| error.to_string())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RecoveryAssetState {
    Absent,
    RestorableVideo,
    Matching,
    Conflict,
}

fn recovery_asset_state(
    state: &AppState,
    store: &crate::core::recording_recovery::RecordingRecoveryStore,
    pending: &PendingRecording,
) -> Result<RecoveryAssetState, String> {
    let mut context = state.library.lock().unwrap();
    let library = context.library().map_err(|error| error.to_string())?;
    let Some(asset) = library.asset_by_id(&pending.id) else {
        return Ok(RecoveryAssetState::Absent);
    };
    match library
        .asset_availability(&asset.id)
        .map_err(|error| error.to_string())?
    {
        crate::core::library::AssetAvailability::Ready => {
            let path = library
                .readable_asset_url(asset)
                .map_err(|error| error.to_string())?;
            if store
                .imported_asset_matches(pending, asset, &path)
                .map_err(|error| error.to_string())?
            {
                Ok(RecoveryAssetState::Matching)
            } else {
                Ok(RecoveryAssetState::Conflict)
            }
        }
        crate::core::library::AssetAvailability::Missing
            if asset.kind == CaptureKind::Video
                && store
                    .recovery_matches_import_proof(pending)
                    .map_err(|error| error.to_string())? =>
        {
            Ok(RecoveryAssetState::RestorableVideo)
        }
        crate::core::library::AssetAvailability::Missing
        | crate::core::library::AssetAvailability::Unreadable => Ok(RecoveryAssetState::Conflict),
    }
}

#[tauri::command]
pub fn convert_to_gif(app: AppHandle, window: WebviewWindow, id: String) -> Result<(), String> {
    let parsed = uuid::Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    let (asset, source_path) = {
        let state = app.state::<AppState>();
        let mut context = state.library.lock().unwrap();
        let library = context.library().map_err(|error| error.to_string())?;
        let asset = library
            .asset_by_id(&parsed)
            .cloned()
            .ok_or_else(|| "The capture could not be found.".to_string())?;
        let source_path = library
            .readable_asset_url(&asset)
            .map_err(|error| error.to_string())?;
        (asset, source_path)
    };
    if asset.kind != CaptureKind::Video
        || !crate::core::policy::RecordingPolicy::is_gif_eligible(asset.duration)
    {
        return Err("Only recordings with a known duration can be converted to GIF.".into());
    }
    let from_completion = window.label() == "toast";
    let completion_monitor = from_completion
        .then(|| window.current_monitor().ok().flatten())
        .flatten();
    let completion_id = uuid::Uuid::new_v4().to_string();
    {
        let state = app.state::<AppState>();
        let mut converting = state.gif_conversion_ids.lock().unwrap();
        if converting.contains(&parsed) {
            return Err("Already converting.".into());
        }
        converting.insert(parsed);
    }
    let _ = app.emit(
        "gif-conversion-state",
        GifConversionStateDto {
            id: parsed.to_string(),
            is_converting: true,
        },
    );
    if from_completion {
        show_completion_preview(
            &app,
            &CompletionPreviewDto::processing(
                completion_id.clone(),
                "gif",
                "Creating GIF",
                &completion_asset_detail(&asset),
            ),
            completion_monitor.clone(),
        );
    }
    let handle = app.clone();
    std::thread::spawn(move || {
        let result = convert_asset_to_gif(&handle, &asset, &source_path);
        let state = handle.state::<AppState>();
        state.gif_conversion_ids.lock().unwrap().remove(&parsed);
        let _ = handle.emit(
            "gif-conversion-state",
            GifConversionStateDto {
                id: parsed.to_string(),
                is_converting: false,
            },
        );
        match result {
            Ok(gif_asset) if from_completion => show_completion_preview(
                &handle,
                &CompletionPreviewDto::ready(
                    completion_id,
                    &gif_asset,
                    "GIF Created",
                    completion_asset_detail(&gif_asset),
                    false,
                ),
                completion_monitor,
            ),
            Ok(_) => emit_notice_local(
                &handle,
                "GIF Created".into(),
                "sparkles.rectangle.stack".into(),
            ),
            Err(error) => {
                if from_completion {
                    emit_notice_on_monitor(
                        &handle,
                        "Could not create GIF".into(),
                        "exclamationmark.triangle.fill".into(),
                        completion_monitor,
                    );
                }
                emit_error(&handle, error, None);
            }
        }
    });
    Ok(())
}

fn convert_asset_to_gif(
    app: &AppHandle,
    asset: &CaptureAsset,
    source_path: &std::path::Path,
) -> Result<CaptureAsset, String> {
    let state = app.state::<AppState>();
    let (gif_path, gif_width, gif_height, gif_duration) = export_gif_file(
        app,
        source_path,
        asset.pixel_width,
        asset.pixel_height,
        asset.duration,
    )?;
    let import_result = {
        let mut context = state.library.lock().unwrap();
        context
            .library_mut()
            .map_err(|error| error.to_string())
            .and_then(|library| {
                library
                    .import_file(
                        &gif_path,
                        CaptureKind::Gif,
                        "gif",
                        gif_width,
                        gif_height,
                        gif_duration.or(asset.duration),
                        asset.source_application.clone(),
                    )
                    .map_err(|e| e.to_string())
            })
    };
    let _ = std::fs::remove_file(&gif_path);
    let gif_asset = import_result?;
    emit_library_changed(app);
    Ok(gif_asset)
}

fn export_gif_file(
    app: &AppHandle,
    source_path: &std::path::Path,
    source_width: i64,
    source_height: i64,
    source_duration: Option<f64>,
) -> Result<(PathBuf, i64, i64, Option<f64>), String> {
    let max_long_edge = crate::core::policy::RecordingPolicy::MAXIMUM_GIF_LONG_EDGE;
    let fps = crate::core::policy::RecordingPolicy::GIF_FRAMES_PER_SECOND;

    #[cfg(windows)]
    {
        let _ = app;
        let gif_path = crate::gif::export_gif(source_path, max_long_edge, fps)
            .map_err(|error| error.to_string())?;
        let (width, height) = if source_width > 0 && source_height > 0 {
            (source_width as u32, source_height as u32)
        } else {
            crate::gif::video_dimensions(source_path).map_err(|error| error.to_string())?
        };
        let (width, height) = crate::gif::scaled_dimensions(width, height, max_long_edge);
        Ok((
            gif_path,
            i64::from(width),
            i64::from(height),
            source_duration,
        ))
    }

    #[cfg(not(windows))]
    {
        let state = app.state::<AppState>();
        let ffmpeg = state.ffmpeg().map_err(|error| error.to_string())?;
        let gif_path = crate::gif::export_gif(source_path, max_long_edge, fps, &ffmpeg)
            .map_err(|error| error.to_string())?;
        let metadata = crate::record::probe_video(&ffmpeg, &gif_path).unwrap_or((
            source_width,
            source_height,
            source_duration,
        ));
        Ok((gif_path, metadata.0, metadata.1, metadata.2))
    }
}

// ---------------------------------------------------------------------------
// Capture flow commands (Windows startup is dispatched from a worker thread)
// ---------------------------------------------------------------------------

fn capture_context(session: &CaptureSession) -> CaptureContextDto {
    let display = &session.display;
    CaptureContextDto {
        display_width: display.screen_frame.width,
        display_height: display.screen_frame.height,
        scale: display.backing_scale,
        pixel_width: display.pixel_width,
        pixel_height: display.pixel_height,
        window_rects: display.window_rects.iter().map(RectDto::from).collect(),
        source_application: session.source_application.clone(),
    }
}

#[tauri::command]
pub fn start_capture(app: AppHandle) -> Result<CaptureContextDto, String> {
    log::info!("start_capture: beginning capture flow");
    let state = app.state::<AppState>();
    let _start_permit = match state.capture_start.try_begin() {
        Some(permit) => permit,
        None => {
            // The overlay can request its already-created context while the
            // initiating call is still creating/focusing the WebView. Preserve
            // that re-entry, but never start a second native display freeze.
            let capture = state.capture.lock().unwrap();
            if let Some(session) = capture.session.as_ref() {
                log::info!(
                    "start_capture: returning active session during startup capture_id={} overlays={}",
                    session.capture_id,
                    session.overlay_labels.len()
                );
                return Ok(capture_context(session));
            }
            log::warn!("start_capture: ignored concurrent native display freeze request");
            return Err("Screen capture is already starting.".into());
        }
    };
    let _transition = state.library_transition.lock().unwrap();

    {
        let capture = state.capture.lock().unwrap();
        // The overlay frontend calls start_capture again when it loads; return
        // the existing session context instead of failing.
        if let Some(session) = capture.session.as_ref() {
            log::info!(
                "start_capture: returning active session capture_id={} overlays={}",
                session.capture_id,
                session.overlay_labels.len()
            );
            return Ok(capture_context(session));
        }
        let recording = state.recording.lock().unwrap();
        if recording.is_recording
            || recording.is_paused
            || recording.is_transitioning
            || recording.is_finalizing
            || recording.is_starting
        {
            return Err("A recording session is active.".into());
        }
    }
    {
        let mut context = state.library.lock().unwrap();
        context.library().map_err(|error| error.to_string())?;
    }

    // Pre-flight only after ruling out an existing capture session. The
    // overlay asks for its current context when it mounts (twice under React
    // StrictMode in development), and that re-entry must never touch the
    // system permission request path.
    #[cfg(target_os = "macos")]
    {
        use crate::capture::current::PermissionState;
        match crate::capture::current::check_capture_permission() {
            PermissionState::Authorized => {}
            PermissionState::RestartRequired => {
                let message = "Screen Recording access was granted. Quit and reopen Kiri once to finish enabling capture.";
                emit_error(&app, message.into(), Some(RecoveryAction::QuitKiri));
                return Err(message.into());
            }
            PermissionState::SettingsRequired => {
                let message = "Screen Recording is off. Enable Kiri in System Settings, then quit and reopen it once.";
                emit_error(&app, message.into(), Some(RecoveryAction::OpenSettings));
                return Err(message.into());
            }
        }
    }

    let (pid, name) = platform::frontmost_application()
        .map(|(pid, name)| (Some(pid), name))
        .unwrap_or((None, None));
    let was_kiri_frontmost = pid == Some(std::process::id());
    let hidden_windows = hide_library_windows(&app);

    // Give the system a beat to settle after hiding Kiri windows.
    std::thread::sleep(std::time::Duration::from_millis(120));

    let display = match capture_backend::capture_active_display() {
        Ok(display) => display,
        Err(error) => {
            restore_capture_origin(&app, &hidden_windows, was_kiri_frontmost, pid);
            let message = format!("Screen capture could not start: {error}");
            log::error!("start_capture: display capture failed: {error}");
            emit_error(&app, message.clone(), None);
            return Err(message);
        }
    };
    log::info!(
        "start_capture: display frozen logical={}x{} pixels={}x{} scale={}",
        display.screen_frame.width,
        display.screen_frame.height,
        display.pixel_width,
        display.pixel_height,
        display.backing_scale
    );

    let context = CaptureContextDto {
        display_width: display.screen_frame.width,
        display_height: display.screen_frame.height,
        scale: display.backing_scale,
        pixel_width: display.pixel_width,
        pixel_height: display.pixel_height,
        window_rects: display.window_rects.iter().map(RectDto::from).collect(),
        source_application: name.clone(),
    };

    let capture_id = uuid::Uuid::new_v4();
    let overlay_frame = display.screen_frame;
    let overlay_scale = display.backing_scale;
    log::info!("start_capture: publishing frozen image capture_id={capture_id}");
    let capture_token = {
        let store = app.state::<crate::protocol::ProtocolStore>();
        crate::protocol::set_frozen_png(&store, capture_id, display.png_data.clone())
    };
    log::info!("start_capture: frozen image published capture_id={capture_id}");

    let overlay_label = "overlay".to_string();
    log::info!("start_capture: publishing capture session capture_id={capture_id}");
    {
        let state = app.state::<AppState>();
        let mut capture = state.capture.lock().unwrap();
        capture.session = Some(CaptureSession {
            capture_id,
            display,
            source_application: name,
            return_pid: pid,
            was_kiri_frontmost,
            hidden_windows,
            overlay_labels: vec![overlay_label.clone()],
            annotation: None,
        });
    }
    log::info!("start_capture: capture session published capture_id={capture_id}");

    log::info!("start_capture: creating overlay window capture_id={capture_id}");
    if let Err(error) = create_overlay_window(&app, overlay_frame, overlay_scale, &capture_token) {
        let failed_session = {
            let state = app.state::<AppState>();
            let mut capture = state.capture.lock().unwrap();
            if capture
                .session
                .as_ref()
                .is_some_and(|session| session.capture_id == capture_id)
            {
                capture.session.take()
            } else {
                None
            }
        };
        if let Some(session) = failed_session {
            teardown_cancelled_capture(&app, session, false);
        }
        log::error!("start_capture: overlay window creation failed: {error}");
        return Err(error.to_string());
    }
    // Make the app active so the overlay webview receives keyboard input
    // (Esc/Return/tool keys) while the user interacts with it.
    platform::activate_self();
    log::info!("start_capture: overlay window ready ({overlay_label})");

    Ok(context)
}

fn hide_library_windows(app: &AppHandle) -> Vec<String> {
    let mut hidden = Vec::new();
    for (label, window) in app.webview_windows() {
        if label == "toast" {
            let _ = window.emit("toast-dismiss", ());
            let _ = window.hide();
            continue;
        }
        if label.starts_with("library") && window.is_visible().unwrap_or(false) {
            let _ = window.hide();
            hidden.push(label);
        }
    }
    hidden
}

fn restore_capture_origin(
    app: &AppHandle,
    hidden_windows: &[String],
    was_kiri_frontmost: bool,
    return_pid: Option<u32>,
) {
    for label in hidden_windows {
        if let Some(window) = app.get_webview_window(label) {
            let _ = window.show();
        }
    }

    if was_kiri_frontmost {
        let focus_label = hidden_windows
            .iter()
            .find(|label| label.as_str() == "library")
            .or_else(|| hidden_windows.first());
        if let Some(window) = focus_label.and_then(|label| app.get_webview_window(label)) {
            let _ = window.set_focus();
        }
    } else if let Some(pid) = return_pid {
        // Showing a previously visible Kiri window may activate the process on
        // some systems. Restore the original external app after all windows
        // have returned to their pre-capture visibility.
        platform::activate_application(pid);
    }
}

fn create_overlay_window(
    app: &AppHandle,
    screen_frame: Rect,
    backing_scale: f64,
    capture_token: &str,
) -> anyhow::Result<String> {
    let label = "overlay".to_string();
    // Tauri inner_size/position are LOGICAL (points on macOS, DIPs on
    // Windows); the frontend works in display points 1:1.
    let builder = WebviewWindowBuilder::new(
        app,
        label.clone(),
        WebviewUrl::App(format!("index.html?window=overlay&captureToken={capture_token}").into()),
    )
    .title("kiri")
    .inner_size(screen_frame.width, screen_frame.height)
    .position(screen_frame.x, screen_frame.y)
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .resizable(false)
    .shadow(false);
    // Build visible: creating a hidden webview and showing it immediately can
    // race WKWebView initialization and leave the page blank on macOS.
    log::info!("create_overlay_window: building webview label={label}");
    let window = builder.build()?;
    log::info!("create_overlay_window: webview built label={label}");
    log::info!("create_overlay_window: placing window label={label}");
    if let Err(error) = platform::place_transient_window(&window, screen_frame, backing_scale) {
        let _ = window.close();
        return Err(error.into());
    }
    log::info!("create_overlay_window: window placed label={label}");
    log::info!("create_overlay_window: configuring window label={label}");
    platform::configure_transient_window(&window, platform::TransientWindowRole::CaptureOverlay);
    log::info!("create_overlay_window: window configured label={label}");
    log::info!("create_overlay_window: focusing window label={label}");
    if let Err(error) = window.set_focus() {
        let _ = window.close();
        return Err(error.into());
    }
    log::info!("create_overlay_window: window focused label={label}");
    Ok(label)
}

#[tauri::command]
pub fn cancel_capture(window: WebviewWindow, app: AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    // Keep the active capture visible to library migration until its bytes are
    // durably committed. This closes the handoff gap between taking the
    // session and writing the library entry.
    let _transition = state.library_transition.lock().unwrap();
    let session = {
        let mut capture = state.capture.lock().unwrap();
        match capture.session.as_ref() {
            Some(session)
                if session
                    .overlay_labels
                    .iter()
                    .any(|label| label == window.label()) =>
            {
                capture.session.take()
            }
            Some(_) => return Err("Capture command is only available to its overlay.".into()),
            None if window.label() == "overlay" => None,
            None => return Err("Capture command is only available to its overlay.".into()),
        }
    };
    let Some(session) = session else {
        log::info!("cancel_capture: no active session; closing orphan overlay");
        let _ = window.close();
        return Ok(());
    };
    log::info!(
        "cancel_capture: capture_id={} requested",
        session.capture_id
    );
    teardown_cancelled_capture(&app, session, true);
    Ok(())
}

pub(crate) fn teardown_cancelled_capture(
    app: &AppHandle,
    session: CaptureSession,
    close_overlays: bool,
) {
    invalidate_capture_resources(app, &session);
    if close_overlays {
        for label in &session.overlay_labels {
            if let Some(window) = app.get_webview_window(label) {
                let _ = window.close();
            }
        }
    }
    restore_capture_origin(
        app,
        &session.hidden_windows,
        session.was_kiri_frontmost,
        session.return_pid,
    );
}

fn invalidate_capture_resources(app: &AppHandle, session: &CaptureSession) {
    let state = app.state::<AppState>();
    for label in &session.overlay_labels {
        state
            .ocr_requests
            .clear_owner(&crate::ocr_controller::OcrRequestOwner {
                label: label.clone(),
                capture_id: session.capture_id,
            });
    }
    crate::protocol::clear_frozen_png_for_capture(
        &app.state::<crate::protocol::ProtocolStore>(),
        session.capture_id,
    );
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PrepareCaptureAnnotationRequest {
    selection: RectDto,
    document_json: String,
}

#[tauri::command]
pub fn prepare_capture_annotation(
    window: WebviewWindow,
    app: AppHandle,
    request: PrepareCaptureAnnotationRequest,
) -> Result<String, String> {
    let document = AnnotationDocument::from_json(&request.document_json)?;
    let state = app.state::<AppState>();
    let mut capture = state.capture.lock().unwrap();
    let session = capture
        .session
        .as_mut()
        .filter(|session| {
            session
                .overlay_labels
                .iter()
                .any(|label| label == window.label())
        })
        .ok_or_else(|| {
            "This command is only available from the active capture overlay.".to_string()
        })?;
    let selection = Rect::new(
        request.selection.x,
        request.selection.y,
        request.selection.width,
        request.selection.height,
    );
    validate_staged_capture_annotation(&session.display, selection, &document)?;
    let token = uuid::Uuid::new_v4();
    session.annotation = Some(StagedCaptureAnnotation {
        token,
        selection,
        document,
    });
    Ok(token.to_string())
}

#[tauri::command]
pub fn confirm_capture(
    window: WebviewWindow,
    app: AppHandle,
    request: tauri::ipc::Request<'_>,
) -> Result<(), String> {
    let annotation_token = request
        .headers()
        .get("x-kiri-annotation-token")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| uuid::Uuid::parse_str(value).ok())
        .ok_or_else(|| "The capture annotation snapshot is invalid.".to_string())?;
    let png = match request.body() {
        tauri::ipc::InvokeBody::Raw(bytes) => bytes.as_slice(),
        tauri::ipc::InvokeBody::Json(_) => {
            return Err("The capture image must use the binary IPC payload.".into())
        }
    };
    if png.is_empty() {
        return Err("The capture image is empty.".into());
    }

    let state = app.state::<AppState>();
    let (capture_id, max_width, max_height, staged, display) = {
        let capture = state.capture.lock().unwrap();
        let session = capture
            .session
            .as_ref()
            .ok_or_else(|| "No active capture session.".to_string())?;
        if !session
            .overlay_labels
            .iter()
            .any(|label| label == window.label())
        {
            return Err("Capture confirmation is only available to its overlay.".into());
        }
        let staged = session
            .annotation
            .as_ref()
            .filter(|staged| staged.token == annotation_token)
            .cloned()
            .ok_or_else(|| {
                "The capture annotation snapshot changed before confirmation.".to_string()
            })?;
        (
            session.capture_id,
            session.display.pixel_width,
            session.display.pixel_height,
            staged,
            session.display.clone(),
        )
    };
    // Validate the complete PNG before consuming the active session or writing
    // to the clipboard/library. This also bounds decoder allocation by the
    // dimensions of the frozen display that owns the request.
    let pixel_size = validate_capture_png(png, max_width, max_height)?;
    log::info!(
        "confirm_capture: validated capture_id={} bytes={} pixels={}x{}",
        capture_id,
        png.len(),
        pixel_size.0,
        pixel_size.1
    );
    let editable_project = if staged.document.has_marks() {
        if (
            i64::from(staged.document.source_pixels.width),
            i64::from(staged.document.source_pixels.height),
        ) != pixel_size
        {
            return Err("The annotation document does not match the captured image.".to_string());
        }
        let source =
            crop_annotation_source(&display, staged.selection, staged.document.source_pixels)?;
        Some((source, staged.document.clone()))
    } else {
        None
    };
    let _transition = state.library_transition.lock().unwrap();
    let session = {
        let mut capture = state.capture.lock().unwrap();
        if capture.session.as_ref().is_none_or(|session| {
            session.capture_id != capture_id
                || session
                    .annotation
                    .as_ref()
                    .is_none_or(|staged| staged.token != annotation_token)
        }) {
            return Err("The capture session changed before confirmation.".into());
        }
        capture.session.take().unwrap()
    };
    log::info!(
        "confirm_capture: session consumed capture_id={}",
        session.capture_id
    );
    invalidate_capture_resources(&app, &session);
    // Errors must be visible: show the library window before emitting.
    let session_feedback =
        match confirm_capture_inner(&app, &state, &session, png, pixel_size, editable_project) {
            Ok(feedback) => feedback,
            Err(error) => {
                if let Some(window) = app.get_webview_window("library") {
                    let _ = window.show();
                }
                if capture_failure_requires_global_error(error.copied) {
                    emit_error(&app, error.message.clone(), None);
                }
                return Err(error.message);
            }
        };
    let mut pending = state.pending_capture_completion.lock().unwrap();
    if pending.is_some() {
        return Err("The previous capture is still completing.".into());
    }
    log::info!(
        "confirm_capture: completion queued until overlay destruction labels={}",
        session.overlay_labels.len()
    );
    *pending = Some(PendingCaptureCompletion {
        session,
        preview: session_feedback.preview,
        monitor: session_feedback.monitor,
    });
    Ok(())
}

#[derive(Debug)]
struct CaptureConfirmationFailure {
    message: String,
    copied: bool,
}

struct CaptureCompletionFeedback {
    preview: CompletionPreviewDto,
    monitor: Option<Monitor>,
}

fn capture_failure_requires_global_error(copied: bool) -> bool {
    !copied
}

fn confirm_capture_inner(
    app: &AppHandle,
    state: &AppState,
    session: &CaptureSession,
    png: &[u8],
    pixel_size: (i64, i64),
    editable_project: Option<(Vec<u8>, AnnotationDocument)>,
) -> Result<CaptureCompletionFeedback, CaptureConfirmationFailure> {
    log::info!("confirm_capture: bytes={}", png.len());

    let completion_monitor = session.overlay_labels.iter().find_map(|label| {
        app.get_webview_window(label)
            .and_then(|window| window.current_monitor().ok().flatten())
    });
    let copied = match platform::write_image_to_clipboard(png) {
        Ok(()) => {
            log::info!("confirm_capture: clipboard write complete");
            true
        }
        Err(error) => {
            log::error!("confirm_capture: clipboard write failed: {error}");
            false
        }
    };

    let (pixel_width, pixel_height) = pixel_size;
    let mut context = state.library.lock().unwrap();
    let library = match context.library_mut() {
        Ok(library) => library,
        Err(error) => {
            if copied {
                emit_notice_on_monitor(
                    app,
                    "Copied, but could not save".into(),
                    "exclamationmark.triangle.fill".into(),
                    completion_monitor,
                );
            }
            return Err(CaptureConfirmationFailure {
                message: error.to_string(),
                copied,
            });
        }
    };
    let import_result = match editable_project {
        Some((source, document)) => serde_json::to_value(document)
            .map_err(|_| "The annotation document could not be encoded.".to_string())
            .and_then(|document| {
                library
                    .import_data_with_annotation_project(
                        png,
                        CaptureKind::Image,
                        "png",
                        pixel_width,
                        pixel_height,
                        None,
                        session.source_application.clone(),
                        None,
                        &source,
                        &document,
                    )
                    .map_err(|error| error.to_string())
            }),
        None => library
            .import_data(
                png,
                CaptureKind::Image,
                "png",
                pixel_width,
                pixel_height,
                None,
                session.source_application.clone(),
                None,
            )
            .map_err(|error| error.to_string()),
    };
    drop(context);
    let asset = match import_result {
        Ok(asset) => asset,
        Err(error) => {
            if copied {
                emit_notice_on_monitor(
                    app,
                    "Copied, but could not save".into(),
                    "exclamationmark.triangle.fill".into(),
                    completion_monitor,
                );
            }
            return Err(CaptureConfirmationFailure {
                message: error,
                copied,
            });
        }
    };
    emit_library_changed(app);
    log::info!(
        "confirm_capture: library import complete asset_id={} copied={}",
        asset.id,
        copied
    );

    let title = if copied {
        "Copied and Saved"
    } else {
        "Saved — Copy Failed"
    };
    Ok(CaptureCompletionFeedback {
        preview: CompletionPreviewDto::ready(
            uuid::Uuid::new_v4().to_string(),
            &asset,
            title,
            completion_asset_detail(&asset),
            copied,
        ),
        monitor: completion_monitor,
    })
}

/// Finalizes a successful capture only after its owner WebView has gone away.
/// This callback runs outside the synchronous confirmation IPC, so creating
/// the completion preview cannot prevent WebView2 from delivering that IPC
/// response. The first destroyed overlay also closes any peers on other
/// monitors; the last one presents feedback and restores the capture origin.
pub(crate) fn finalize_confirmed_capture_after_overlay_destroyed(app: &AppHandle, label: &str) {
    let state = app.state::<AppState>();
    let (remaining_labels, completed) = {
        let mut slot = state.pending_capture_completion.lock().unwrap();
        let Some(pending) = slot.as_mut() else {
            return;
        };
        if !pending
            .session
            .overlay_labels
            .iter()
            .any(|owner| owner == label)
        {
            return;
        }
        pending
            .session
            .overlay_labels
            .retain(|owner| owner != label && app.get_webview_window(owner).is_some());
        let remaining_labels = pending.session.overlay_labels.clone();
        let completed = if remaining_labels.is_empty() {
            slot.take()
        } else {
            None
        };
        (remaining_labels, completed)
    };

    for owner in remaining_labels {
        if let Some(window) = app.get_webview_window(&owner) {
            if let Err(error) = window.close() {
                log::error!("confirm_capture: peer overlay close failed label={owner}: {error}");
            }
        }
    }

    if let Some(completed) = completed {
        show_completion_preview(app, &completed.preview, completed.monitor);
        restore_focus(app, &completed.session);
        log::info!("confirm_capture: completion flow returned after overlay destruction");
    }
}

fn validate_staged_capture_annotation(
    display: &crate::capture::CapturedDisplay,
    selection: Rect,
    document: &AnnotationDocument,
) -> Result<(), String> {
    let values = [
        selection.x,
        selection.y,
        selection.width,
        selection.height,
        display.screen_frame.width,
        display.screen_frame.height,
    ];
    if values.iter().any(|value| !value.is_finite())
        || selection.x < 0.0
        || selection.y < 0.0
        || selection.width <= 0.0
        || selection.height <= 0.0
        || selection.x + selection.width > display.screen_frame.width + 0.01
        || selection.y + selection.height > display.screen_frame.height + 0.01
        || (document.canvas.width - selection.width).abs() > 0.01
        || (document.canvas.height - selection.height).abs() > 0.01
    {
        return Err("The annotation selection is invalid.".into());
    }
    let scale_x = display.pixel_width as f64 / display.screen_frame.width;
    let scale_y = display.pixel_height as f64 / display.screen_frame.height;
    let expected_width = (selection.width * scale_x).round();
    let expected_height = (selection.height * scale_y).round();
    if expected_width != f64::from(document.source_pixels.width)
        || expected_height != f64::from(document.source_pixels.height)
    {
        return Err("The annotation source dimensions are invalid.".into());
    }
    Ok(())
}

fn crop_annotation_source(
    display: &crate::capture::CapturedDisplay,
    selection: Rect,
    expected: AnnotationPixelSize,
) -> Result<Vec<u8>, String> {
    validate_capture_png(
        display.png_data.as_ref(),
        display.pixel_width,
        display.pixel_height,
    )?;
    let image =
        image::load_from_memory_with_format(display.png_data.as_ref(), image::ImageFormat::Png)
            .map_err(|_| "The frozen capture image is invalid.".to_string())?;
    let width = expected.width;
    let height = expected.height;
    if width == 0 || height == 0 || width > image.width() || height > image.height() {
        return Err("The annotation source dimensions are invalid.".into());
    }
    let scale_x = image.width() as f64 / display.screen_frame.width;
    let scale_y = image.height() as f64 / display.screen_frame.height;
    let max_left = image.width() - width;
    let max_top = image.height() - height;
    let left = (selection.x * scale_x)
        .round()
        .clamp(0.0, f64::from(max_left)) as u32;
    let top = (selection.y * scale_y)
        .round()
        .clamp(0.0, f64::from(max_top)) as u32;
    let cropped = image.crop_imm(left, top, width, height);
    let mut output = std::io::Cursor::new(Vec::new());
    cropped
        .write_to(&mut output, image::ImageFormat::Png)
        .map_err(|_| "The editable screenshot source could not be prepared.".to_string())?;
    Ok(output.into_inner())
}

const CAPTURE_PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
const CAPTURE_PNG_END: &[u8; 12] = b"\0\0\0\0IEND\xaeB`\x82";
const CAPTURE_PNG_SIZE_SLACK: u64 = 1024 * 1024;
const CAPTURE_PNG_DECODE_BYTES_PER_PIXEL: u64 = 8;

fn capture_png_dimensions(png: &[u8]) -> Option<(u32, u32)> {
    if png.len() < 33
        || !png.starts_with(CAPTURE_PNG_SIGNATURE)
        || png.get(8..12)? != 13_u32.to_be_bytes()
        || png.get(12..16)? != b"IHDR"
    {
        return None;
    }
    let width = u32::from_be_bytes(png.get(16..20)?.try_into().ok()?);
    let height = u32::from_be_bytes(png.get(20..24)?.try_into().ok()?);
    (width > 0 && height > 0).then_some((width, height))
}

fn validate_capture_png(png: &[u8], max_width: i64, max_height: i64) -> Result<(i64, i64), String> {
    if !png.starts_with(CAPTURE_PNG_SIGNATURE)
        || !png.ends_with(CAPTURE_PNG_END)
        || max_width <= 0
        || max_height <= 0
    {
        return Err("The capture image is invalid.".into());
    }
    let (header_width, header_height) =
        capture_png_dimensions(png).ok_or_else(|| "The capture image is invalid.".to_string())?;
    let width = i64::from(header_width);
    let height = i64::from(header_height);
    if width > max_width || height > max_height {
        return Err("The capture dimensions are invalid.".into());
    }

    let max_bytes = u64::from(header_width)
        .checked_mul(u64::from(header_height))
        .and_then(|pixels| pixels.checked_mul(4))
        .and_then(|bytes| bytes.checked_add(CAPTURE_PNG_SIZE_SLACK))
        .ok_or_else(|| "The capture dimensions are invalid.".to_string())?;
    if u64::try_from(png.len()).unwrap_or(u64::MAX) > max_bytes {
        return Err("The capture image is too large.".into());
    }

    let max_alloc = u64::from(header_width)
        .checked_mul(u64::from(header_height))
        .and_then(|pixels| pixels.checked_mul(CAPTURE_PNG_DECODE_BYTES_PER_PIXEL))
        .and_then(|bytes| bytes.checked_add(CAPTURE_PNG_SIZE_SLACK))
        .ok_or_else(|| "The capture dimensions are invalid.".to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(header_width);
    limits.max_image_height = Some(header_height);
    limits.max_alloc = Some(max_alloc);
    let mut reader =
        image::ImageReader::with_format(std::io::Cursor::new(png), image::ImageFormat::Png);
    reader.limits(limits);
    let image = reader
        .decode()
        .map_err(|_| "The capture image is invalid.".to_string())?;
    if image.width() != header_width || image.height() != header_height {
        return Err("The capture dimensions are invalid.".into());
    }
    Ok((width, height))
}

#[tauri::command]
pub async fn save_file_dialog(
    window: WebviewWindow,
    app: AppHandle,
    default_name: String,
) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let editor_id = editor_window_id(&window)?;
    {
        let state = app.state::<AppState>();
        let mut context = state.library.lock().unwrap();
        let library = context.library().map_err(|error| error.to_string())?;
        let asset = library
            .asset_by_id(&editor_id)
            .ok_or_else(|| "The edited asset could not be found.".to_string())?;
        if asset.kind != CaptureKind::Image {
            return Err("Only image captures can be edited.".into());
        }
    }
    let window_label = window.label().to_string();
    // Localize the filter label from the persisted language preference.
    let filter_label = match crate::state::load_language(&app).as_str() {
        "zh-Hans" => "PNG 图片",
        "ja" => "PNG 画像",
        _ => "PNG image",
    }
    .to_string();
    let dialog_app = app.clone();
    let destination = tauri::async_runtime::spawn_blocking(move || {
        dialog_app
            .dialog()
            .file()
            .set_file_name(default_name)
            .add_filter(filter_label, &["png"])
            .blocking_save_file()
            .and_then(|path| path.into_path().ok())
    })
    .await
    .map_err(|error| format!("Could not open the save panel: {error}"))?;

    let state = app.state::<AppState>();
    let mut destinations = state.editor_save_destinations.lock().unwrap();
    let Some(path) = destination else {
        destinations.remove(&window_label);
        return Ok(None);
    };
    let token = uuid::Uuid::new_v4();
    destinations.insert(window_label, ApprovedEditorSave { token, path });
    Ok(Some(token.to_string()))
}

fn restore_focus(app: &AppHandle, session: &CaptureSession) {
    restore_capture_origin(
        app,
        &session.hidden_windows,
        session.was_kiri_frontmost,
        session.return_pid,
    );
}

// ---------------------------------------------------------------------------
// Editor command
// ---------------------------------------------------------------------------

const EDITOR_ACTION_INVALID_ERROR: &str = "The editor save action is invalid.";
const EDITOR_SAVE_ERROR: &str = "The edited image could not be saved to the selected file.";
const EDITOR_REVISION_MISMATCH_ERROR: &str = "The screenshot changed after the editor opened.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EditorSaveAction {
    Save,
    SaveAs,
}

fn parse_editor_save_action(
    value: Option<&str>,
    has_save_token: bool,
) -> Result<EditorSaveAction, String> {
    match (value, has_save_token) {
        (Some("save"), false) => Ok(EditorSaveAction::Save),
        (Some("save-as"), true) => Ok(EditorSaveAction::SaveAs),
        _ => Err(EDITOR_ACTION_INVALID_ERROR.into()),
    }
}

fn write_editor_save(path: &Path, png: &[u8]) -> Result<(), String> {
    let parent = path.parent().ok_or_else(|| EDITOR_SAVE_ERROR.to_string())?;
    let mut temporary = tempfile::Builder::new()
        .prefix(".kiri-export-")
        .suffix(".tmp")
        .tempfile_in(parent)
        .map_err(|_| EDITOR_SAVE_ERROR.to_string())?;
    std::io::Write::write_all(temporary.as_file_mut(), png)
        .and_then(|_| std::io::Write::flush(temporary.as_file_mut()))
        .and_then(|_| temporary.as_file().sync_all())
        .map_err(|_| EDITOR_SAVE_ERROR.to_string())?;
    temporary
        .persist(path)
        .map_err(|_| EDITOR_SAVE_ERROR.to_string())?;
    Ok(())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnnotationProjectDto {
    revision_sha256: String,
    state: EditorAnnotationState,
    document_json: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorUpdateDto {
    revision_sha256: String,
    action_succeeded: bool,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EditorCropPixels {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

fn require_editor_window(window: &WebviewWindow, id: &uuid::Uuid) -> Result<(), String> {
    let expected = format!("editor-{}", id.to_string().to_lowercase());
    if window.label() == expected {
        Ok(())
    } else {
        Err("This command is only available from the matching screenshot editor.".into())
    }
}

fn editor_window_id(window: &WebviewWindow) -> Result<uuid::Uuid, String> {
    let raw_id = window
        .label()
        .strip_prefix("editor-")
        .ok_or_else(|| "This command is only available from a screenshot editor.".to_string())?;
    let id = uuid::Uuid::parse_str(raw_id)
        .map_err(|_| "This command is only available from a screenshot editor.".to_string())?;
    require_editor_window(window, &id)?;
    Ok(id)
}

fn editor_save_destination(
    destinations: &HashMap<String, ApprovedEditorSave>,
    window_label: &str,
    token: Option<uuid::Uuid>,
) -> Result<Option<PathBuf>, String> {
    let approved = destinations.get(window_label);
    match (token, approved) {
        (None, _) => Ok(None),
        (Some(token), Some(approved)) if approved.token == token => Ok(Some(approved.path.clone())),
        (Some(_), _) => Err("The selected save destination is no longer authorized.".into()),
    }
}

fn commit_editor_update<T, E>(
    mutation: impl FnOnce() -> Result<T, E>,
    committed_action: impl FnOnce() -> bool,
) -> Result<(T, bool), E> {
    let revision = mutation()?;
    Ok((revision, committed_action()))
}

fn is_annotation_revision(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn validate_editor_annotation_document(
    document: &AnnotationDocument,
    expected_size: (i64, i64),
) -> Result<(), String> {
    document.validate_for_image_pixels(expected_size.0, expected_size.1)
}

fn crop_editor_source(
    source_png: &[u8],
    expected_size: (i64, i64),
    crop: EditorCropPixels,
) -> Result<Vec<u8>, String> {
    if validate_capture_png(source_png, expected_size.0, expected_size.1)? != expected_size {
        return Err("The editable screenshot source dimensions changed unexpectedly.".into());
    }
    if crop.width == 0
        || crop.height == 0
        || u64::from(crop.x) + u64::from(crop.width) > expected_size.0 as u64
        || u64::from(crop.y) + u64::from(crop.height) > expected_size.1 as u64
    {
        return Err("The crop area is invalid.".into());
    }
    let image = image::load_from_memory_with_format(source_png, image::ImageFormat::Png)
        .map_err(|_| "The editable screenshot source is invalid.".to_string())?;
    let cropped = image.crop_imm(crop.x, crop.y, crop.width, crop.height);
    let mut output = std::io::Cursor::new(Vec::new());
    cropped
        .write_to(&mut output, image::ImageFormat::Png)
        .map_err(|_| "The cropped screenshot source could not be prepared.".to_string())?;
    Ok(output.into_inner())
}

#[tauri::command]
pub fn get_asset_annotation_project(
    window: WebviewWindow,
    app: AppHandle,
    id: String,
) -> Result<AnnotationProjectDto, String> {
    let parsed =
        uuid::Uuid::parse_str(&id).map_err(|_| "The edited asset id is invalid.".to_string())?;
    require_editor_window(&window, &parsed)?;
    let state = app.state::<AppState>();
    let (snapshot, expected_size) = {
        let mut context = state.library.lock().unwrap();
        let library = context.library().map_err(|error| error.to_string())?;
        let asset = library
            .asset_by_id(&parsed)
            .ok_or_else(|| "The edited asset could not be found.".to_string())?;
        if asset.kind != CaptureKind::Image {
            return Err("Only image captures can be edited.".into());
        }
        let expected_size = (asset.pixel_width, asset.pixel_height);
        let snapshot = library
            .load_editor_snapshot(&parsed)
            .map_err(|error| error.to_string())?;
        (snapshot, expected_size)
    };
    let document_json = snapshot
        .document
        .map(|document| {
            let document_json = serde_json::to_string(&document)
                .map_err(|_| "The annotation document could not be decoded.".to_string())?;
            let document = AnnotationDocument::from_json(&document_json)?;
            validate_editor_annotation_document(&document, expected_size)?;
            String::from_utf8(document.to_json()?)
                .map_err(|_| "The annotation document could not be decoded.".to_string())
        })
        .transpose()?;
    Ok(AnnotationProjectDto {
        revision_sha256: snapshot.revision_sha256,
        state: snapshot.state,
        document_json,
    })
}

#[tauri::command]
pub async fn prepare_asset_annotation(
    window: WebviewWindow,
    app: AppHandle,
    id: String,
    document_json: String,
    revision_sha256: String,
    crop_pixels: Option<EditorCropPixels>,
) -> Result<String, String> {
    let parsed =
        uuid::Uuid::parse_str(&id).map_err(|_| "The edited asset id is invalid.".to_string())?;
    require_editor_window(&window, &parsed)?;
    let document = AnnotationDocument::from_json(&document_json)?;
    let state = app.state::<AppState>();
    if !is_annotation_revision(&revision_sha256) {
        return Err("The editor source revision is invalid.".into());
    }
    let (expected_size, source_png) = {
        let mut context = state.library.lock().unwrap();
        let library = context.library().map_err(|error| error.to_string())?;
        let asset = library
            .asset_by_id(&parsed)
            .ok_or_else(|| "The edited asset could not be found.".to_string())?;
        if asset.kind != CaptureKind::Image {
            return Err("Only image captures can be edited.".into());
        }
        let snapshot = library
            .load_editor_snapshot(&parsed)
            .map_err(|error| error.to_string())?;
        if snapshot.revision_sha256 != revision_sha256 {
            return Err(EDITOR_REVISION_MISMATCH_ERROR.into());
        }
        ((asset.pixel_width, asset.pixel_height), snapshot.source)
    };
    let replacement_source_png = match crop_pixels {
        Some(crop) => Some(
            tauri::async_runtime::spawn_blocking(move || {
                crop_editor_source(&source_png, expected_size, crop)
            })
            .await
            .map_err(|_| "The cropped screenshot source could not be prepared.".to_string())??,
        ),
        None => None,
    };
    let output_size = crop_pixels
        .map(|crop| (i64::from(crop.width), i64::from(crop.height)))
        .unwrap_or(expected_size);
    validate_editor_annotation_document(&document, output_size)?;
    let token = uuid::Uuid::new_v4();
    state.editor_annotations.lock().unwrap().insert(
        window.label().to_string(),
        StagedEditorAnnotation {
            token,
            document,
            replacement_source_png,
            output_size,
            revision_sha256,
        },
    );
    Ok(token.to_string())
}

#[tauri::command]
pub fn update_asset(
    window: WebviewWindow,
    app: AppHandle,
    request: tauri::ipc::Request<'_>,
) -> Result<EditorUpdateDto, String> {
    let parsed = request
        .headers()
        .get("x-kiri-asset-id")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| uuid::Uuid::parse_str(value).ok())
        .ok_or_else(|| "The edited asset id is invalid.".to_string())?;
    require_editor_window(&window, &parsed)?;
    let annotation_token = request
        .headers()
        .get("x-kiri-annotation-token")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| uuid::Uuid::parse_str(value).ok())
        .ok_or_else(|| "The editor annotation snapshot is invalid.".to_string())?;
    let editor_action = request
        .headers()
        .get("x-kiri-editor-action")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let save_token = request
        .headers()
        .get("x-kiri-save-token")
        .map(|value| {
            value
                .to_str()
                .map_err(|_| "The save authorization is invalid.".to_string())
                .and_then(|value| {
                    uuid::Uuid::parse_str(value)
                        .map_err(|_| "The save authorization is invalid.".to_string())
                })
        })
        .transpose()?;
    let editor_action = parse_editor_save_action(editor_action.as_deref(), save_token.is_some())?;
    let png = match request.body() {
        tauri::ipc::InvokeBody::Raw(bytes) if !bytes.is_empty() => bytes.as_slice(),
        tauri::ipc::InvokeBody::Raw(_) => return Err("The edited image is empty.".into()),
        tauri::ipc::InvokeBody::Json(_) => {
            return Err("The edited image must use the binary IPC payload.".into())
        }
    };

    let state = app.state::<AppState>();
    let current_size = {
        let mut context = state.library.lock().unwrap();
        let library = context.library().map_err(|error| error.to_string())?;
        let asset = library
            .asset_by_id(&parsed)
            .ok_or_else(|| "The edited asset could not be found.".to_string())?;
        if asset.kind != CaptureKind::Image {
            return Err("Only image captures can be edited.".into());
        }
        (asset.pixel_width, asset.pixel_height)
    };
    let mut annotations = state.editor_annotations.lock().unwrap();
    let staged = annotations
        .get(window.label())
        .filter(|staged| staged.token == annotation_token)
        .cloned()
        .ok_or_else(|| "The editor annotation snapshot changed before saving.".to_string())?;
    if staged.replacement_source_png.is_none() && staged.output_size != current_size {
        return Err("The edited image dimensions changed unexpectedly.".into());
    }
    if validate_capture_png(png, staged.output_size.0, staged.output_size.1)? != staged.output_size
    {
        return Err("The edited image dimensions changed unexpectedly.".into());
    }
    validate_editor_annotation_document(&staged.document, staged.output_size)?;
    let mut destinations = state.editor_save_destinations.lock().unwrap();
    let save_path = editor_save_destination(&destinations, window.label(), save_token)?;
    let store = app.state::<crate::protocol::ProtocolStore>();
    let document_value = serde_json::to_value(&staged.document)
        .map_err(|_| "The annotation document could not be encoded.".to_string())?;
    if editor_action == EditorSaveAction::SaveAs {
        let current_revision = {
            let mut context = state.library.lock().unwrap();
            let library = context.library().map_err(|error| error.to_string())?;
            library
                .load_editor_snapshot(&parsed)
                .map_err(|error| error.to_string())?
                .revision_sha256
        };
        if current_revision != staged.revision_sha256 {
            return Err(EDITOR_REVISION_MISMATCH_ERROR.into());
        }
        annotations.remove(window.label());
        if save_token.is_some() {
            destinations.remove(window.label());
        }
        drop(destinations);
        drop(annotations);
        let action_succeeded = save_path
            .as_ref()
            .is_some_and(|path| write_editor_save(path, png).is_ok());
        return Ok(EditorUpdateDto {
            revision_sha256: current_revision,
            action_succeeded,
        });
    }
    let mutation: Result<(String, bool), LibraryLocationError> = commit_editor_update(
        || {
            crate::protocol::with_thumbnail_invalidation(&store, parsed, || {
                let mut context = state.library.lock().unwrap();
                let library = context.library_mut()?;
                if let Some(source_png) = staged.replacement_source_png.as_deref() {
                    library.save_editor_cropped_snapshot(
                        &parsed,
                        &staged.revision_sha256,
                        png,
                        source_png,
                        staged.output_size,
                        staged.document.has_marks().then_some(&document_value),
                    )?;
                } else {
                    library.save_editor_snapshot(
                        &parsed,
                        &staged.revision_sha256,
                        png,
                        staged.document.has_marks().then_some(&document_value),
                    )?;
                }
                Ok::<String, LibraryLocationError>(
                    library.load_editor_snapshot(&parsed)?.revision_sha256,
                )
            })
        },
        || {
            annotations.remove(window.label());
            if save_token.is_some() {
                destinations.remove(window.label());
            }
            drop(destinations);
            drop(annotations);

            debug_assert!(save_path.is_none());
            true
        },
    );
    let (revision_sha256, action_succeeded) = match mutation {
        Ok(result) => result,
        Err(LibraryLocationError::Library(AssetLibraryError::AnnotationRevisionMismatch)) => {
            return Err(EDITOR_REVISION_MISMATCH_ERROR.into())
        }
        Err(error) => return Err(error.to_string()),
    };
    emit_asset_content_changed(&app, &parsed);
    emit_library_changed(&app);
    Ok(EditorUpdateDto {
        revision_sha256,
        action_succeeded,
    })
}

/// Sets a friendly display title for a capture (metadata only; the on-disk
/// filename is unchanged so existing libraries stay compatible).
#[tauri::command]
pub fn rename_asset(app: AppHandle, id: String, title: String) -> Result<(), String> {
    let parsed = uuid::Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    let trimmed = title.trim().to_string();
    let state = app.state::<AppState>();
    let mut context = state.library.lock().unwrap();
    context
        .library_mut()
        .map_err(|error| error.to_string())?
        .set_title(
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            },
            &parsed,
        )
        .map_err(|e| e.to_string())?;
    drop(context);
    emit_library_changed(&app);
    Ok(())
}

/// Replaces the tag list of a capture (metadata only).
#[tauri::command]
pub fn set_tags(app: AppHandle, id: String, tags: Vec<String>) -> Result<(), String> {
    let parsed = uuid::Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    let state = app.state::<AppState>();
    let mut context = state.library.lock().unwrap();
    context
        .library_mut()
        .map_err(|error| error.to_string())?
        .set_tags(tags, &parsed)
        .map_err(|e| e.to_string())?;
    drop(context);
    emit_library_changed(&app);
    Ok(())
}

#[tauri::command]
pub fn copy_text(app: AppHandle, window: WebviewWindow, text: String) -> Result<(), String> {
    platform::write_text_to_clipboard(&text).map_err(|e| e.to_string())?;
    // OCR runs inside a screen-covering always-on-top overlay. Tear that
    // capture session down before presenting confirmation, otherwise the
    // resident toast exists but remains hidden until the user closes OCR.
    cancel_capture(window, app.clone())?;
    emit_notice(&app, "Text Copied".into(), "checkmark.circle.fill".into());
    Ok(())
}

// ---------------------------------------------------------------------------
// Recording commands
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartRecordingRequest {
    pub region: RectDto,
    pub options: RecordingOptions,
}

#[tauri::command]
pub async fn start_recording_flow(
    app: AppHandle,
    request: StartRecordingRequest,
) -> Result<(), String> {
    // Do not trigger a privacy prompt for stale/direct IPC without a live
    // user-initiated capture session.
    {
        let state = app.state::<AppState>();
        if state.capture.lock().unwrap().session.is_none() {
            return Err("No active capture session.".into());
        }
    }
    {
        let state = app.state::<AppState>();
        let mut context = state.library.lock().unwrap();
        context.library().map_err(|error| error.to_string())?;
    }

    let saved_options = request.options.normalized();
    let mut options = saved_options;
    if options.output_format == RecordingOutputFormat::Gif {
        // GIF has no audio channel. Keep the saved MP4 preferences intact and
        // disable audio only for this capture session.
        options.captures_system_audio = false;
        options.captures_microphone = false;
    }
    #[cfg(target_os = "macos")]
    if options.captures_microphone {
        match platform::request_microphone_access() {
            Ok(platform::MicrophoneAccess::Authorized) => {}
            Ok(platform::MicrophoneAccess::Unsupported) => {
                // A stale preference or direct IPC call must not send the
                // macOS 15-only SCK microphone selector on older systems.
                options.captures_microphone = false;
            }
            Ok(platform::MicrophoneAccess::Denied) | Err(_) => {
                let message =
                    "Microphone access is off. Enable Kiri in System Settings to record your microphone.";
                emit_error(
                    &app,
                    message.into(),
                    Some(RecoveryAction::OpenMicrophoneSettings),
                );
                return Err(message.into());
            }
        }
    }

    let (screen_frame, backing_scale, return_pid, was_kiri_frontmost, overlay_labels) = {
        let state = app.state::<AppState>();
        let _transition = state.library_transition.lock().unwrap();
        {
            let mut context = state.library.lock().unwrap();
            context.library().map_err(|error| error.to_string())?;
        }
        let mut capture = state.capture.lock().unwrap();
        let Some(session) = capture.session.take() else {
            return Err("No active capture session.".into());
        };
        drop(capture);
        let completion_monitor = session.overlay_labels.iter().find_map(|label| {
            app.get_webview_window(label)
                .and_then(|window| window.current_monitor().ok().flatten())
        });
        *state.saved_recording_options.lock().unwrap() = saved_options;
        let mut recording = state.recording.lock().unwrap();
        *recording = RecordingFlow {
            session_id: uuid::Uuid::new_v4(),
            return_pid: session.return_pid,
            was_kiri_frontmost: session.was_kiri_frontmost,
            is_starting: true,
            completion_monitor,
            configuration: Some(RecordingConfiguration {
                display_id: session.display.display_id,
                display_identity: session.display.display_identity.clone(),
                region: Rect::new(
                    request.region.x,
                    request.region.y,
                    request.region.width,
                    request.region.height,
                ),
                screen_frame: session.display.screen_frame,
                backing_scale: session.display.backing_scale,
                options,
            }),
            ..Default::default()
        };
        emit_recording_state(&app, &recording);
        invalidate_capture_resources(&app, &session);
        (
            session.display.screen_frame,
            session.display.backing_scale,
            session.return_pid,
            session.was_kiri_frontmost,
            session.overlay_labels,
        )
    };

    for label in &overlay_labels {
        if let Some(window) = app.get_webview_window(label) {
            let _ = window.close();
        }
    }

    // Restore focus to the source application (mirrors AppModel.onRecord).
    if !was_kiri_frontmost {
        if let Some(pid) = return_pid {
            platform::activate_application(pid);
        }
    }

    crate::state::save_recording_options(&app, &saved_options);

    if !options.uses_countdown {
        // No countdown requested: start recording immediately.
        return begin_recording(app).await;
    }

    let label = "countdown".to_string();
    let region_screen = Rect::new(
        screen_frame.x + request.region.x,
        screen_frame.y + request.region.y,
        request.region.width,
        request.region.height,
    );
    log::info!(
        "create countdown window: screen_frame=({:.0},{:.0}) region=({:.0},{:.0}) → window at ({:.0},{:.0} {:.0}x{:.0})",
        screen_frame.x,
        screen_frame.y,
        request.region.x,
        request.region.y,
        region_screen.x,
        region_screen.y,
        region_screen.width,
        region_screen.height,
    );
    let builder = WebviewWindowBuilder::new(
        &app,
        label.clone(),
        WebviewUrl::App("index.html?window=countdown".into()),
    )
    .title("kiri")
    // Cover the whole display so the countdown badge is centered on the
    // SCREEN, not on the selected region (the user's expectation).
    .inner_size(screen_frame.width, screen_frame.height)
    .position(screen_frame.x, screen_frame.y)
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .shadow(false);
    let window = match builder.build() {
        Ok(window) => window,
        Err(error) => {
            reset_recording_session(&app);
            return Err(error.to_string());
        }
    };
    if let Err(error) = platform::place_transient_window(&window, screen_frame, backing_scale) {
        let _ = window.close();
        reset_recording_session(&app);
        return Err(error.to_string());
    }
    platform::set_window_capture_excluded(&app, &label, true);
    // Spec (recording §5.1): the countdown window is level .screenSaver.
    platform::configure_transient_window(
        &window,
        platform::TransientWindowRole::RecordingCountdown,
    );
    let _ = window.show();
    Ok(())
}

#[tauri::command]
pub fn cancel_recording_flow(app: AppHandle) -> Result<(), String> {
    reset_recording_session(&app);
    Ok(())
}

fn close_recording_windows(app: &AppHandle) {
    for label in ["countdown", "control-panel", "ripple"] {
        if let Some(window) = app.get_webview_window(label) {
            let _ = window.close();
        }
    }
}

fn stop_click_monitor(app: &AppHandle) {
    let monitor = {
        let state = app.state::<AppState>();
        let monitor = state.click_monitor.lock().unwrap().take();
        monitor
    };
    if let Some(monitor) = monitor {
        monitor.stop();
    }
}

fn discard_active_recording(mut active: ActiveRecording) {
    if let Some(mut recorder) = active.recorder.take() {
        let _ = recorder.stop();
    }
    if let Some(encoder) = active.encoder.take() {
        encoder.cancel();
    }
}

fn cleanup_abandoned_recording(app: &AppHandle, abandoned: RecordingFlow) {
    stop_click_monitor(app);
    close_recording_windows(app);
    if let Some(active) = abandoned.active {
        discard_active_recording(active);
    }
    for path in abandoned.segments {
        let _ = std::fs::remove_file(path);
    }
}

/// Aborts a recording and returns every session-owned resource to idle so no
/// spinner, overlay, click monitor, encoder, or partial segment survives.
fn reset_recording_session(app: &AppHandle) {
    let abandoned = {
        let state = app.state::<AppState>();
        let mut recording = state.recording.lock().unwrap();
        let abandoned = recording.take_and_reset();
        emit_recording_state(app, &recording);
        abandoned
    };
    cleanup_abandoned_recording(app, abandoned);
}

fn take_completed_recording_segments(app: &AppHandle) -> Vec<PathBuf> {
    let state = app.state::<AppState>();
    let mut recording = state.recording.lock().unwrap();
    recording.take_completed_segments()
}

/// Imports every segment that was completed before the active segment failed.
/// `finalize_recording` removes source files only after the library import is
/// durable; on an import failure the valid temporary files remain intact for
/// recovery instead of being erased by the generic session reset.
fn recover_completed_recording(
    app: &AppHandle,
    completed_segments: Vec<PathBuf>,
) -> Result<bool, String> {
    if completed_segments.is_empty() {
        return Ok(false);
    }
    finalize_recording(
        app,
        completed_segments,
        RecordingOutputFormat::Mp4,
        false,
        None,
    )
    .map(|_| true)
    .map_err(|error| error.to_string())
}

/// Resets only the startup task that still owns this session. A late download
/// failure from a cancelled task must not tear down a replacement recording.
fn reset_startup_if_current(app: &AppHandle, token: uuid::Uuid) -> bool {
    let abandoned = {
        let state = app.state::<AppState>();
        let mut recording = state.recording.lock().unwrap();
        let abandoned = recording.take_if_startup_is_current(token);
        if abandoned.is_some() {
            emit_recording_state(app, &recording);
        }
        abandoned
    };
    if let Some(abandoned) = abandoned {
        cleanup_abandoned_recording(app, abandoned);
        true
    } else {
        false
    }
}

fn startup_is_current(app: &AppHandle, token: uuid::Uuid) -> bool {
    let state = app.state::<AppState>();
    let recording = state.recording.lock().unwrap();
    recording.startup_is_current(token)
}

fn recover_failed_resume(app: &AppHandle) {
    let stale_active = {
        let state = app.state::<AppState>();
        let mut recording = state.recording.lock().unwrap();
        let stale_active = recording.recover_failed_resume();
        emit_recording_state(app, &recording);
        stale_active
    };
    if let Some(active) = stale_active {
        discard_active_recording(active);
    }
}

fn create_control_panel(
    app: &AppHandle,
    configuration: &RecordingConfiguration,
) -> Result<(), String> {
    let frame = configuration.screen_frame;
    // Newer product behavior places the draggable panel at the bottom-right;
    // the frontend persists any position the user chooses afterwards.
    let panel_x = frame.x + frame.width - 296.0 - 24.0;
    let panel_y = frame.y + frame.height - 64.0 - 64.0;
    log::info!(
        "create_control_panel: screen_frame=({:.0},{:.0} {:.0}x{:.0}) → panel at ({:.0},{:.0})",
        frame.x,
        frame.y,
        frame.width,
        frame.height,
        panel_x,
        panel_y,
    );
    let panel = WebviewWindowBuilder::new(
        app,
        "control-panel".to_string(),
        WebviewUrl::App("index.html?window=control-panel".into()),
    )
    .title("kiri")
    .inner_size(296.0, 64.0)
    .position(panel_x, panel_y)
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .shadow(false)
    .build()
    .map_err(|e| e.to_string())?;
    let panel_frame = Rect::new(panel_x, panel_y, 296.0, 64.0);
    if let Err(error) =
        platform::place_transient_window(&panel, panel_frame, configuration.backing_scale)
    {
        let _ = panel.close();
        return Err(error.to_string());
    }
    platform::set_window_capture_excluded(app, "control-panel", true);
    // Keep the panel above every app window. It takes keyboard focus so the
    // recording hotkeys work (Space = pause/resume, Esc = stop); the panel
    // itself is the only window the user interacts with while recording.
    platform::configure_transient_window(&panel, platform::TransientWindowRole::RecordingControls);
    let _ = panel.show();
    let _ = panel.set_focus();
    Ok(())
}

fn create_ripple_window(
    app: &AppHandle,
    configuration: &RecordingConfiguration,
) -> Result<(), String> {
    let region = configuration.region;
    let frame = configuration.screen_frame;
    let ripple = WebviewWindowBuilder::new(
        app,
        "ripple".to_string(),
        WebviewUrl::App("index.html?window=ripple".into()),
    )
    .title("kiri")
    .inner_size(region.width, region.height)
    .position(frame.x + region.x, frame.y + region.y)
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .focused(false)
    .skip_taskbar(true)
    .shadow(false)
    .build()
    .map_err(|e| e.to_string())?;
    let ripple_frame = Rect::new(
        frame.x + region.x,
        frame.y + region.y,
        region.width,
        region.height,
    );
    if let Err(error) =
        platform::place_transient_window(&ripple, ripple_frame, configuration.backing_scale)
    {
        let _ = ripple.close();
        return Err(error.to_string());
    }
    platform::configure_transient_window(&ripple, platform::TransientWindowRole::ClickRipple);
    platform::set_window_click_through(app, "ripple");
    Ok(())
}

struct StartedRecorder {
    recorder: Box<dyn crate::capture::PlatformRecorder + Send>,
    system_audio_spec: Option<crate::record::AudioSpec>,
    microphone_spec: Option<crate::record::AudioSpec>,
}

struct RecorderSenders {
    video: crate::capture::VideoFrameSender,
    system_audio: Option<crate::record::AudioChunkSender>,
    microphone: Option<crate::record::AudioChunkSender>,
}

struct EncoderReceivers {
    video: mpsc::Receiver<Vec<u8>>,
    system_audio: Option<crate::record::AudioChunkReceiver>,
    microphone: Option<crate::record::AudioChunkReceiver>,
}

fn recording_channels(options: RecordingOptions) -> (RecorderSenders, EncoderReceivers) {
    let (video_tx, video_rx) = mpsc::sync_channel(crate::capture::VIDEO_FRAME_QUEUE_CAPACITY);
    let (audio_tx, audio_rx) = options
        .captures_system_audio
        .then(crate::record::bounded_audio_channel)
        .map(|(tx, rx)| (Some(tx), Some(rx)))
        .unwrap_or((None, None));
    let (mic_tx, mic_rx) = options
        .captures_microphone
        .then(crate::record::bounded_audio_channel)
        .map(|(tx, rx)| (Some(tx), Some(rx)))
        .unwrap_or((None, None));
    (
        RecorderSenders {
            video: video_tx,
            system_audio: audio_tx,
            microphone: mic_tx,
        },
        EncoderReceivers {
            video: video_rx,
            system_audio: audio_rx,
            microphone: mic_rx,
        },
    )
}

fn start_recorder(
    app: &AppHandle,
    configuration: &RecordingConfiguration,
    senders: RecorderSenders,
) -> Result<StartedRecorder, String> {
    #[cfg(windows)]
    let _ = app;
    #[cfg(target_os = "macos")]
    let ripple_excepted = platform::window_capture_id(app, "ripple")
        .into_iter()
        .collect::<Vec<_>>();
    #[cfg(target_os = "macos")]
    {
        let recorder = crate::capture::macos::MacRecordingSession::start(
            configuration.display_id,
            configuration.region,
            configuration.backing_scale,
            configuration.options,
            &ripple_excepted,
            senders.video,
            senders.system_audio,
            senders.microphone,
        )
        .map_err(|e| e.to_string())?;
        let float_audio = crate::record::AudioSpec {
            sample_rate: 48_000,
            channels: 2,
            format: crate::record::AudioSampleFormat::F32,
        };
        Ok(StartedRecorder {
            recorder: Box::new(recorder),
            system_audio_spec: configuration
                .options
                .captures_system_audio
                .then_some(float_audio),
            microphone_spec: configuration
                .options
                .captures_microphone
                .then_some(float_audio),
        })
    }
    #[cfg(windows)]
    {
        let display_identity = configuration.display_identity.as_ref().ok_or_else(|| {
            "The selected display identity is unavailable. Select it again.".to_string()
        })?;
        let recorder = crate::capture::windows::WindowsRecorder::start(
            display_identity,
            configuration.region,
            configuration.backing_scale,
            configuration.options,
            senders.video,
            senders.system_audio,
            senders.microphone,
        )
        .map_err(|e| e.to_string())?;
        let system_audio_spec = recorder.system_audio_spec();
        let microphone_spec = recorder.microphone_spec();
        Ok(StartedRecorder {
            recorder: Box::new(recorder),
            system_audio_spec,
            microphone_spec,
        })
    }
}

fn start_encoder(
    prepared: &PreparedEncoder,
    configuration: &RecordingConfiguration,
    out_path: PathBuf,
    receivers: EncoderReceivers,
    recorder: &StartedRecorder,
) -> Result<crate::record::SegmentEncoder, String> {
    let width = crate::core::policy::RecordingPolicy::pixel_dimension(
        configuration.region.width,
        configuration.backing_scale,
    );
    let height = crate::core::policy::RecordingPolicy::pixel_dimension(
        configuration.region.height,
        configuration.backing_scale,
    );
    let mut encoder_config = crate::record::EncoderConfig {
        width,
        height,
        fps: crate::core::policy::RecordingPolicy::FRAMES_PER_SECOND,
        bitrate: crate::record::bitrate_for(width, height),
        audio: recorder.system_audio_spec,
        mic: recorder.microphone_spec,
        video_encoder: String::new(),
    };
    match prepared {
        PreparedEncoder::Ffmpeg {
            ffmpeg,
            video_encoder,
        } => {
            encoder_config.video_encoder.clone_from(video_encoder);
            log::info!(
                "start_encoder: ffmpeg={} video {}x{}@{}, audio={}, mic={}",
                ffmpeg.display(),
                encoder_config.width,
                encoder_config.height,
                encoder_config.fps,
                encoder_config.audio.is_some(),
                encoder_config.mic.is_some(),
            );
            crate::record::SegmentEncoder::start(
                &encoder_config,
                out_path,
                ffmpeg,
                receivers.video,
                receivers.system_audio,
                receivers.microphone,
            )
            .map_err(|e| e.to_string())
        }
        #[cfg(windows)]
        PreparedEncoder::WindowsNative => {
            log::info!(
                "start_encoder: Windows Media Foundation H.264 {}x{}@{}, audio={}, mic={}",
                encoder_config.width,
                encoder_config.height,
                encoder_config.fps,
                encoder_config.audio.is_some(),
                encoder_config.mic.is_some(),
            );
            crate::record::SegmentEncoder::start_windows_native(
                &encoder_config,
                out_path,
                receivers.video,
                receivers.system_audio,
                receivers.microphone,
            )
            .map_err(|e| e.to_string())
        }
    }
}

enum PreparedEncoder {
    Ffmpeg {
        ffmpeg: PathBuf,
        video_encoder: String,
    },
    #[cfg(windows)]
    WindowsNative,
}

async fn prepare_encoder(
    app: AppHandle,
    output_format: RecordingOutputFormat,
) -> Result<PreparedEncoder, String> {
    #[cfg(windows)]
    {
        let _ = (app, output_format);
        return Ok(PreparedEncoder::WindowsNative);
    }
    #[cfg(not(windows))]
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let ffmpeg = state.ffmpeg().map_err(|error| error.to_string())?;
        // Hardware probing can launch several short ffmpeg processes on first
        // use. Finish it before native capture starts so live audio cannot fill
        // its bounded queue while no pipe writer exists yet.
        let video_encoder =
            crate::record::pick_video_encoder(&ffmpeg).map_err(|error| error.to_string())?;
        Ok(PreparedEncoder::Ffmpeg {
            ffmpeg,
            video_encoder,
        })
    })
    .await
    .map_err(|error| format!("video encoder preparation task failed: {error}"))?
}

fn spawn_recording_clock(app: &AppHandle, session_id: uuid::Uuid) {
    let handle = app.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_millis(250));
        let should_continue = {
            let state = handle.state::<AppState>();
            let recording = state.recording.lock().unwrap();
            if recording.session_id != session_id
                || recording.configuration.is_none()
                || recording.is_finalizing
            {
                false
            } else if recording.is_recording && !recording.is_paused {
                emit_recording_state(&handle, &recording);
                true
            } else {
                // Keep the one session clock alive across pause/resume
                // transitions; paused time is excluded by recording_state().
                true
            }
        };
        if !should_continue {
            break;
        }
    });
}

#[tauri::command]
pub async fn begin_recording(app: AppHandle) -> Result<(), String> {
    log::info!("begin_recording: called");
    if let Some(window) = app.get_webview_window("countdown") {
        let _ = window.close();
    }

    let (startup_token, configuration, session_id) = {
        let state = app.state::<AppState>();
        let mut recording = state.recording.lock().unwrap();
        if recording.is_recording || recording.is_paused {
            return Ok(());
        }
        let Some(configuration) = recording.configuration.clone() else {
            return Err("No recording configuration.".into());
        };
        let Some(token) = recording.claim_startup() else {
            // Another begin request already owns this session, or the flow is
            // transitioning/finalizing. Treat duplicate IPC as idempotent.
            return Ok(());
        };
        emit_recording_state(&app, &recording);
        (token, configuration, recording.session_id)
    };

    if let Err(error) = create_control_panel(&app, &configuration) {
        if reset_startup_if_current(&app, startup_token) {
            return Err(error);
        }
        return Ok(());
    }
    if configuration.options.highlights_clicks {
        if let Err(error) = create_ripple_window(&app, &configuration) {
            if reset_startup_if_current(&app, startup_token) {
                return Err(error);
            }
            return Ok(());
        }
        // The click monitor needs the Input Monitoring permission; install
        // it only while highlighting clicks (avoids a permission prompt at
        // every launch).
        if let Err(error) = crate::ensure_click_monitor(&app) {
            log::error!("recording: global click monitor unavailable: {error}");
            let message = "Input Monitoring is off. Enable Kiri in System Settings to highlight clicks while recording.";
            #[cfg(target_os = "macos")]
            let recovery = Some(RecoveryAction::OpenInputMonitoringSettings);
            #[cfg(windows)]
            let recovery = None;
            if reset_startup_if_current(&app, startup_token) {
                emit_error(&app, message.into(), recovery);
                return Err(error.to_string());
            }
            return Ok(());
        }
    }

    // Windows MP4 and GIF sessions both start with the native Media Foundation
    // MP4 fallback, so encoder preparation is immediate and offline. Other
    // platforms still resolve their FFmpeg encoder before native capture begins.
    let prepared = match prepare_encoder(app.clone(), configuration.options.output_format).await {
        Ok(prepared) => prepared,
        Err(error) => {
            log::error!("recording: encoder preparation failed: {error}");
            if reset_startup_if_current(&app, startup_token) {
                emit_error(
                    &app,
                    "Could not prepare the video encoder. Check your connection and try recording again."
                        .into(),
                    None,
                );
                return Err(error);
            }
            return Ok(());
        }
    };

    if !startup_is_current(&app, startup_token) {
        return Ok(());
    }

    let out_path = std::env::temp_dir().join(format!(
        "kiri-recording-{}.mp4",
        uuid::Uuid::new_v4().to_string().to_lowercase()
    ));

    let (senders, receivers) = recording_channels(configuration.options);
    let mut started = match start_recorder(&app, &configuration, senders) {
        Ok(started) => started,
        Err(error) => {
            log::error!("recording: start_recorder failed: {error}");
            if reset_startup_if_current(&app, startup_token) {
                emit_error(&app, "Could not start screen recording.".into(), None);
                return Err(error);
            }
            return Ok(());
        }
    };
    if !startup_is_current(&app, startup_token) {
        let _ = started.recorder.stop();
        let _ = std::fs::remove_file(out_path);
        return Ok(());
    }
    let encoder = match start_encoder(
        &prepared,
        &configuration,
        out_path.clone(),
        receivers,
        &started,
    ) {
        Ok(encoder) => encoder,
        Err(error) => {
            log::error!("recording: start_encoder failed: {error}");
            let _ = started.recorder.stop();
            let _ = std::fs::remove_file(&out_path);
            if reset_startup_if_current(&app, startup_token) {
                emit_error(&app, "Could not start the video encoder.".into(), None);
                return Err(error);
            }
            return Ok(());
        }
    };

    let stale_active = {
        let state = app.state::<AppState>();
        let mut recording = state.recording.lock().unwrap();
        let active = ActiveRecording {
            encoder: Some(encoder),
            recorder: Some(started.recorder),
        };
        match recording.complete_startup(startup_token, active) {
            Ok(()) => {
                emit_recording_state(&app, &recording);
                None
            }
            Err(active) => Some(active),
        }
    };
    if let Some(active) = stale_active {
        discard_active_recording(active);
        let _ = std::fs::remove_file(out_path);
        return Ok(());
    }

    // Focus stays on the control panel while recording so the hotkeys work
    // (Space = pause/resume, Esc = stop); stop_recording hands focus back to
    // the original application afterwards.

    // A single session-owned clock survives pause/resume and refreshes the
    // control panel without counting paused time.
    spawn_recording_clock(&app, session_id);

    emit_notice(
        &app,
        "Recording Started".into(),
        "record.circle.fill".into(),
    );
    Ok(())
}

#[tauri::command]
pub async fn pause_recording(app: AppHandle) -> Result<(), String> {
    log::info!("pause_recording: called");
    let active = {
        let state = app.state::<AppState>();
        let mut recording = state.recording.lock().unwrap();
        if !recording.is_recording || recording.is_transitioning {
            return Ok(());
        }
        recording.elapsed_before_segment = crate::state::recording_state(&recording).elapsed;
        recording.started_at = None;
        recording.is_recording = false;
        recording.is_transitioning = true;
        emit_recording_state(&app, &recording);
        recording.active.take()
    };
    let Some(mut active) = active else {
        let error = "The active recording session is unavailable.".to_string();
        reset_recording_session(&app);
        return Err(error);
    };

    if active
        .encoder
        .as_ref()
        .is_some_and(crate::record::SegmentEncoder::is_windows_native)
    {
        let pause_result = active
            .recorder
            .as_mut()
            .ok_or_else(|| "The native recorder is unavailable.".to_string())
            .and_then(|recorder| recorder.pause().map_err(|error| error.to_string()));
        if let Err(error) = pause_result {
            discard_active_recording(active);
            reset_recording_session(&app);
            return Err(error);
        }
        let state = app.state::<AppState>();
        let mut recording = state.recording.lock().unwrap();
        recording.active = Some(active);
        recording.is_paused = true;
        recording.is_transitioning = false;
        emit_recording_state(&app, &recording);
        emit_notice(&app, "Recording Paused".into(), "pause.circle.fill".into());
        return Ok(());
    }

    let mut segment_path = None;
    let mut failure = None;
    match active.recorder.take() {
        Some(mut recorder) => {
            if let Err(error) = recorder.stop() {
                failure = Some(error.to_string());
            }
        }
        None => failure = Some("The native recorder is unavailable.".into()),
    }
    if failure.is_none() {
        match active.encoder.take() {
            Some(encoder) => match encoder.finish() {
                Ok(path) => segment_path = Some(path),
                Err(error) => failure = Some(error.to_string()),
            },
            None => failure = Some("The video encoder is unavailable.".into()),
        }
    } else if let Some(encoder) = active.encoder.take() {
        encoder.cancel();
    }
    drop(active);

    if let Some(error) = failure {
        log::error!("recording: current segment failed while pausing: {error}");
        let completed_segments = take_completed_recording_segments(&app);
        reset_recording_session(&app);
        let (message, recovery_error) = match recover_completed_recording(&app, completed_segments) {
            Ok(true) => (
                "The current recording section failed. Earlier completed sections were saved as a partial recording."
                    .to_string(),
                None,
            ),
            Ok(false) => ("Could not pause screen recording.".to_string(), None),
            Err(recovery_error) => (
                "Could not pause screen recording. Kiri kept the completed sections so they are not lost."
                    .to_string(),
                Some(recovery_error),
            ),
        };
        emit_error(&app, message, None);
        return Err(match recovery_error {
            Some(recovery_error) => {
                format!("{error}; partial recording recovery failed: {recovery_error}")
            }
            None => error,
        });
    }
    let state = app.state::<AppState>();
    let mut recording = state.recording.lock().unwrap();
    if let Some(path) = segment_path {
        recording.segments.push(path);
    }
    recording.is_paused = true;
    recording.is_transitioning = false;
    emit_recording_state(&app, &recording);
    emit_notice(&app, "Recording Paused".into(), "pause.circle.fill".into());
    Ok(())
}

#[tauri::command]
pub async fn resume_recording(app: AppHandle) -> Result<(), String> {
    log::info!("resume_recording: called");
    let resumed_native = {
        let state = app.state::<AppState>();
        let mut recording = state.recording.lock().unwrap();
        let is_native = recording
            .active
            .as_ref()
            .and_then(|active| active.encoder.as_ref())
            .is_some_and(crate::record::SegmentEncoder::is_windows_native);
        if !recording.is_paused || recording.is_transitioning || !is_native {
            false
        } else {
            let active = recording
                .active
                .as_mut()
                .ok_or_else(|| "The active recording session is unavailable.".to_string())?;
            let recorder = active
                .recorder
                .as_mut()
                .ok_or_else(|| "The native recorder is unavailable.".to_string())?;
            recorder.resume().map_err(|error| error.to_string())?;
            recording.is_paused = false;
            recording.is_recording = true;
            recording.started_at = Some(std::time::Instant::now());
            emit_recording_state(&app, &recording);
            true
        }
    };
    if resumed_native {
        if let Some(window) = app.get_webview_window("control-panel") {
            let _ = window.set_focus();
        }
        emit_notice(&app, "Recording Resumed".into(), "play.circle.fill".into());
        return Ok(());
    }

    let configuration = {
        let state = app.state::<AppState>();
        let mut recording = state.recording.lock().unwrap();
        if !recording.is_paused || recording.is_transitioning {
            return Ok(());
        }
        let Some(configuration) = recording.configuration.clone() else {
            return Err("No recording configuration.".into());
        };
        recording.is_transitioning = true;
        emit_recording_state(&app, &recording);
        configuration
    };

    // Resolve the encoder before recreating bounded capture channels. A
    // normal resume hits AppState's initialized path immediately; the async
    // boundary also makes inconsistent/direct IPC fail safely.
    let prepared = match prepare_encoder(app.clone(), configuration.options.output_format).await {
        Ok(prepared) => prepared,
        Err(error) => {
            log::error!("recording: resume encoder preparation failed: {error}");
            emit_error(&app, "Could not prepare the video encoder.".into(), None);
            recover_failed_resume(&app);
            return Err(error);
        }
    };

    let out_path = std::env::temp_dir().join(format!(
        "kiri-recording-{}.mp4",
        uuid::Uuid::new_v4().to_string().to_lowercase()
    ));

    let (senders, receivers) = recording_channels(configuration.options);
    let mut started = match start_recorder(&app, &configuration, senders) {
        Ok(started) => started,
        Err(error) => {
            log::error!("recording: start_recorder failed: {error}");
            emit_error(&app, "Could not start screen recording.".into(), None);
            recover_failed_resume(&app);
            return Err(error);
        }
    };
    let encoder = match start_encoder(
        &prepared,
        &configuration,
        out_path.clone(),
        receivers,
        &started,
    ) {
        Ok(encoder) => encoder,
        Err(error) => {
            log::error!("recording: start_encoder failed: {error}");
            let _ = started.recorder.stop();
            let _ = std::fs::remove_file(out_path);
            emit_error(&app, "Could not start the video encoder.".into(), None);
            recover_failed_resume(&app);
            return Err(error);
        }
    };

    {
        let state = app.state::<AppState>();
        let mut recording = state.recording.lock().unwrap();
        recording.is_paused = false;
        recording.is_transitioning = false;
        recording.is_recording = true;
        recording.started_at = Some(std::time::Instant::now());
        recording.active = Some(ActiveRecording {
            encoder: Some(encoder),
            recorder: Some(started.recorder),
        });
        emit_recording_state(&app, &recording);
    }
    if let Some(window) = app.get_webview_window("control-panel") {
        let _ = window.set_focus();
    }
    emit_notice(&app, "Recording Resumed".into(), "play.circle.fill".into());
    Ok(())
}

#[tauri::command]
pub async fn stop_recording(app: AppHandle) -> Result<(), String> {
    log::info!("stop_recording: called");
    let abandoned_startup = {
        let state = app.state::<AppState>();
        let mut recording = state.recording.lock().unwrap();
        if recording.is_starting && !recording.is_recording && !recording.is_paused {
            let abandoned = recording.take_and_reset();
            emit_recording_state(&app, &recording);
            Some(abandoned)
        } else {
            None
        }
    };
    if let Some(abandoned) = abandoned_startup {
        cleanup_abandoned_recording(&app, abandoned);
        return Ok(());
    }

    let (
        segments,
        active,
        needs_active_segment,
        return_pid,
        was_kiri_frontmost,
        output_format,
        completion_monitor,
        windows_native,
        finalize_metadata,
    ) = {
        let state = app.state::<AppState>();
        let mut recording = state.recording.lock().unwrap();
        if !(recording.is_recording || recording.is_paused) || recording.is_transitioning {
            return Ok(());
        }
        let needs_active_segment = recording.is_recording;
        let elapsed = crate::state::recording_state(&recording).elapsed;
        let windows_native = recording
            .active
            .as_ref()
            .and_then(|active| active.encoder.as_ref())
            .is_some_and(crate::record::SegmentEncoder::is_windows_native);
        let finalize_metadata =
            recording
                .configuration
                .as_ref()
                .map(|configuration| RecordingFinalizeMetadata {
                    pixel_width: crate::core::policy::RecordingPolicy::pixel_dimension(
                        configuration.region.width,
                        configuration.backing_scale,
                    ),
                    pixel_height: crate::core::policy::RecordingPolicy::pixel_dimension(
                        configuration.region.height,
                        configuration.backing_scale,
                    ),
                    duration: (elapsed > 0.0).then_some(elapsed),
                });
        recording.is_recording = false;
        recording.is_paused = false;
        recording.is_finalizing = true;
        recording.started_at = None;
        emit_recording_state(&app, &recording);
        (
            recording.take_completed_segments(),
            recording.active.take(),
            needs_active_segment,
            recording.return_pid,
            recording.was_kiri_frontmost,
            recording
                .configuration
                .as_ref()
                .map(|configuration| configuration.options.output_format)
                .unwrap_or_default(),
            recording.completion_monitor.take(),
            windows_native,
            finalize_metadata,
        )
    };

    stop_click_monitor(&app);
    close_recording_windows(&app);

    // Entire stop + finalize runs on a background thread: recorder.stop()
    // and encoder.finish() can block on SCK/ffmpeg callbacks, and merging
    // + probing takes time. The UI already shows the finalizing spinner, so
    // the async command returns immediately and the panel stays responsive.
    let handle = app.clone();
    std::thread::spawn(move || {
        let mut failure = None;
        let mut final_segments = segments;
        if let Some(mut active) = active {
            match active.recorder.take() {
                Some(mut recorder) => {
                    if let Err(error) = recorder.stop() {
                        failure = Some(error.to_string());
                    }
                }
                None if needs_active_segment => {
                    failure = Some("The native recorder is unavailable.".into())
                }
                None => {}
            }
            if failure.is_none() {
                match active.encoder.take() {
                    Some(encoder) => match encoder.finish() {
                        Ok(path) => final_segments.push(path),
                        Err(error) => failure = Some(error.to_string()),
                    },
                    None if needs_active_segment => {
                        failure = Some("The video encoder is unavailable.".into())
                    }
                    None => {}
                }
            } else if let Some(encoder) = active.encoder.take() {
                encoder.cancel();
            }
        } else if needs_active_segment {
            failure = Some("The active recording session is unavailable.".into());
        }

        // The native capture session is over once its final MP4 segment has
        // been closed. Release the global recording lock and restore the
        // source application before potentially long merge/GIF work so an
        // unlimited-duration GIF never blocks the next capture or holds
        // focus for the whole conversion.
        reset_recording_session(&handle);
        if !was_kiri_frontmost {
            if let Some(pid) = return_pid {
                platform::activate_application(pid);
            }
        }

        if let Some(error) = failure {
            log::error!("recording: active segment failed while stopping: {error}");
            let recovery = recover_completed_recording(&handle, final_segments);
            let message = match recovery {
                Ok(true) => "Screen recording stopped unexpectedly. Earlier completed sections were saved as a partial recording."
                    .to_string(),
                Ok(false) => "Could not save the recording.".to_string(),
                Err(recovery_error) => {
                    log::error!("recording: partial recording recovery failed: {recovery_error}");
                    "Could not save the recording. Kiri kept the completed sections so they are not lost."
                        .to_string()
                }
            };
            emit_error(&handle, message, None);
        } else {
            let completion_id = uuid::Uuid::new_v4().to_string();
            let (processing_kind, processing_title, processing_detail) = match output_format {
                RecordingOutputFormat::Mp4 => ("video", "Saving Recording", "MP4"),
                RecordingOutputFormat::Gif => ("gif", "Creating GIF", "12 fps · 720 px long edge"),
            };
            show_completion_preview(
                &handle,
                &CompletionPreviewDto::processing(
                    completion_id.clone(),
                    processing_kind,
                    processing_title,
                    processing_detail,
                ),
                completion_monitor.clone(),
            );
            let result = finalize_recording(
                &handle,
                final_segments,
                output_format,
                windows_native,
                finalize_metadata,
            );
            match result {
                Ok(outcome) => show_completion_preview(
                    &handle,
                    &CompletionPreviewDto::ready(
                        completion_id,
                        &outcome.asset,
                        if outcome.gif_fallback {
                            "GIF Failed — Recording Saved as MP4"
                        } else if outcome.asset.kind == CaptureKind::Gif {
                            "GIF Created"
                        } else {
                            "Recording Saved"
                        },
                        completion_asset_detail(&outcome.asset),
                        false,
                    ),
                    completion_monitor,
                ),
                Err(error) if error.is_queued() => {
                    log::warn!("recording: final import queued for retry: {error}");
                    emit_notice_on_monitor(
                        &handle,
                        "A recording is waiting to save".into(),
                        "arrow.clockwise".into(),
                        completion_monitor,
                    );
                }
                Err(error) => {
                    log::error!("recording: final import failed: {error}");
                    emit_notice_on_monitor(
                        &handle,
                        "Could not save the recording".into(),
                        "exclamationmark.triangle.fill".into(),
                        completion_monitor,
                    );
                    emit_error(&handle, "Could not save the recording.".into(), None);
                }
            }
        }
    });
    Ok(())
}

struct RecordingFinalizeOutcome {
    asset: CaptureAsset,
    gif_fallback: bool,
}

#[derive(Clone, Copy)]
struct RecordingFinalizeMetadata {
    pixel_width: i64,
    pixel_height: i64,
    duration: Option<f64>,
}

enum RecordingFinalizeError {
    Queued(String),
    Failed(String),
}

impl RecordingFinalizeError {
    fn is_queued(&self) -> bool {
        matches!(self, Self::Queued(_))
    }
}

impl std::fmt::Display for RecordingFinalizeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Queued(message) | Self::Failed(message) => formatter.write_str(message),
        }
    }
}

fn import_recording_file(
    state: &AppState,
    path: &std::path::Path,
    kind: CaptureKind,
    extension: &str,
    pixel_width: i64,
    pixel_height: i64,
    duration: Option<f64>,
) -> Result<CaptureAsset, String> {
    let mut context = state.library.lock().unwrap();
    context
        .library_mut()
        .map_err(|error| error.to_string())?
        .import_file(
            path,
            kind,
            extension,
            pixel_width,
            pixel_height,
            duration,
            None,
        )
        .map_err(|error| error.to_string())
}

#[allow(clippy::too_many_arguments)]
fn import_recovery_output(
    state: &AppState,
    path: &std::path::Path,
    pending: &PendingRecording,
    kind: CaptureKind,
    extension: &str,
    pixel_width: i64,
    pixel_height: i64,
    duration: Option<f64>,
) -> Result<CaptureAsset, String> {
    let mut context = state.library.lock().unwrap();
    context
        .library_mut()
        .map_err(|error| error.to_string())?
        .import_file_with_stable_id(
            path,
            pending.id,
            pending.created_at,
            kind,
            extension,
            pixel_width,
            pixel_height,
            duration,
            None,
        )
        .map_err(|error| error.to_string())
}

fn finalize_recording(
    app: &AppHandle,
    segments: Vec<PathBuf>,
    output_format: RecordingOutputFormat,
    windows_native: bool,
    metadata: Option<RecordingFinalizeMetadata>,
) -> Result<RecordingFinalizeOutcome, RecordingFinalizeError> {
    let state = app.state::<AppState>();
    let _recovery_transition = state.recording_recovery_transition.lock().unwrap();
    let merged_path = std::env::temp_dir().join(format!(
        "kiri-recording-merged-{}.mp4",
        uuid::Uuid::new_v4().to_string().to_lowercase()
    ));
    let mut merge_completed = false;
    let mut pending = None;
    let result: Result<RecordingFinalizeOutcome, String> = (|| {
        #[cfg(windows)]
        let ffmpeg: Option<PathBuf> = None;
        #[cfg(not(windows))]
        let ffmpeg = if windows_native {
            None
        } else {
            Some(state.ffmpeg().map_err(|error| error.to_string())?)
        };
        if windows_native {
            if segments.len() != 1 {
                return Err("The Windows recording did not produce exactly one segment.".into());
            }
            std::fs::copy(&segments[0], &merged_path)
                .map_err(|error| format!("could not stage the Windows MP4: {error}"))?;
        } else {
            crate::record::merge_segments(
                &segments,
                &merged_path,
                ffmpeg.as_deref().expect("ffmpeg path was prepared"),
            )
            .map_err(|e| e.to_string())?;
        }
        merge_completed = true;
        let (pixel_width, pixel_height, duration) = if windows_native {
            metadata
                .map(|metadata| {
                    (
                        metadata.pixel_width,
                        metadata.pixel_height,
                        metadata.duration,
                    )
                })
                .unwrap_or((0, 0, None))
        } else {
            crate::record::probe_video(
                ffmpeg.as_deref().expect("ffmpeg path was prepared"),
                &merged_path,
            )
            .unwrap_or((0, 0, None))
        };
        match state.recording_recovery.lock().unwrap().persist(
            &merged_path,
            pixel_width,
            pixel_height,
            duration,
        ) {
            Ok(recording) => pending = Some(recording),
            Err(error) => {
                log::error!("recording: could not stage finalized MP4 for recovery: {error}")
            }
        }

        if output_format == RecordingOutputFormat::Mp4 {
            if let Some(pending) = pending.as_mut() {
                state
                    .recording_recovery
                    .lock()
                    .unwrap()
                    .prepare_import(pending, CaptureKind::Video, &merged_path)
                    .map_err(|error| error.to_string())?;
            }
            let asset = import_finalized_output(
                &state,
                &merged_path,
                pending.as_ref(),
                CaptureKind::Video,
                "mp4",
                pixel_width,
                pixel_height,
                duration,
            )?;
            return Ok(RecordingFinalizeOutcome {
                asset,
                gif_fallback: false,
            });
        }

        let gif_result = (|| {
            let (gif_path, gif_width, gif_height, gif_duration) =
                export_gif_file(app, &merged_path, pixel_width, pixel_height, duration)?;
            if let Some(pending) = pending.as_mut() {
                state
                    .recording_recovery
                    .lock()
                    .unwrap()
                    .prepare_import(pending, CaptureKind::Gif, &gif_path)
                    .map_err(|error| error.to_string())?;
            }
            let import_result = import_finalized_output(
                &state,
                &gif_path,
                pending.as_ref(),
                CaptureKind::Gif,
                "gif",
                gif_width,
                gif_height,
                gif_duration.or(duration),
            );
            let _ = std::fs::remove_file(&gif_path);
            import_result
        })();

        match gif_result {
            Ok(asset) => Ok(RecordingFinalizeOutcome {
                asset,
                gif_fallback: false,
            }),
            Err(gif_error) => {
                log::warn!(
                    "recording: GIF finalization failed, preserving the MP4 fallback: {gif_error}"
                );
                if let Some(pending) = pending.as_mut() {
                    state
                        .recording_recovery
                        .lock()
                        .unwrap()
                        .prepare_import(pending, CaptureKind::Video, &merged_path)
                        .map_err(|error| error.to_string())?;
                }
                let asset = import_finalized_output(
                    &state,
                    &merged_path,
                    pending.as_ref(),
                    CaptureKind::Video,
                    "mp4",
                    pixel_width,
                    pixel_height,
                    duration,
                )
                .map_err(|fallback_error| {
                    format!(
                        "GIF finalization failed: {gif_error}; MP4 fallback failed: {fallback_error}"
                    )
                })?;
                Ok(RecordingFinalizeOutcome {
                    asset,
                    gif_fallback: true,
                })
            }
        }
    })();
    let recovery_kept = pending.as_ref().is_some_and(|pending| {
        state
            .recording_recovery
            .lock()
            .unwrap()
            .validate_video(pending)
            .is_ok()
    });
    let durably_kept = result.is_ok() || recovery_kept;
    if result.is_ok() {
        if let Some(mut pending) = pending.take() {
            if let Err(error) = cleanup_verified_recovery(&state, &mut pending) {
                log::warn!(
                    "recording: imported recovery {} could not be cleaned: {error}",
                    pending.id
                );
            }
        }
    }
    cleanup_finalization_files(&segments, &merged_path, merge_completed, durably_kept);

    match result {
        Ok(outcome) => {
            emit_library_changed(app);
            Ok(outcome)
        }
        Err(error) if recovery_kept => {
            emit_library_changed(app);
            Err(RecordingFinalizeError::Queued(error))
        }
        Err(error) => Err(RecordingFinalizeError::Failed(error)),
    }
}

#[allow(clippy::too_many_arguments)]
fn import_finalized_output(
    state: &AppState,
    path: &std::path::Path,
    pending: Option<&PendingRecording>,
    kind: CaptureKind,
    extension: &str,
    pixel_width: i64,
    pixel_height: i64,
    duration: Option<f64>,
) -> Result<CaptureAsset, String> {
    match pending {
        Some(pending) => import_recovery_output(
            state,
            path,
            pending,
            kind,
            extension,
            pixel_width,
            pixel_height,
            duration,
        ),
        None => import_recording_file(
            state,
            path,
            kind,
            extension,
            pixel_width,
            pixel_height,
            duration,
        ),
    }
}

fn cleanup_finalization_files(
    segments: &[PathBuf],
    merged_path: &std::path::Path,
    merge_completed: bool,
    durably_kept: bool,
) {
    if durably_kept {
        for segment in segments {
            let _ = std::fs::remove_file(segment);
        }
        let _ = std::fs::remove_file(merged_path);
    } else if !merge_completed {
        let _ = std::fs::remove_file(merged_path);
    }
}

// ---------------------------------------------------------------------------
// Settings / locale / shortcuts
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn log_frontend_error(window: WebviewWindow, message: String) -> Result<(), String> {
    require_known_frontend_window(&window)?;
    log::error!("[frontend] {}", sanitize_frontend_log(message));
    Ok(())
}

fn require_known_frontend_window(window: &WebviewWindow) -> Result<(), String> {
    let label = window.label();
    if matches!(
        label,
        "library" | "overlay" | "countdown" | "control-panel" | "ripple" | "confirm" | "toast"
    ) || ["viewer-", "editor-"]
        .iter()
        .any(|prefix| label.starts_with(prefix))
    {
        Ok(())
    } else {
        Err("This command is unavailable from this window.".into())
    }
}

fn sanitize_frontend_log(message: String) -> String {
    const MAX_BYTES: usize = 4 * 1024;
    let mut sanitized = String::with_capacity(message.len().min(MAX_BYTES));
    for character in message.chars() {
        let character = if character.is_control() {
            ' '
        } else {
            character
        };
        if sanitized.len() + character.len_utf8() > MAX_BYTES {
            break;
        }
        sanitized.push(character);
    }
    sanitized
}

#[tauri::command]
pub fn show_confirm_dialog(
    app: AppHandle,
    kind: String,
    title: String,
    message: String,
    confirmLabel: String,
    ids: Option<Vec<String>>,
) {
    crate::state::show_confirm_dialog(
        &app,
        kind,
        title,
        message,
        confirmLabel,
        ids.unwrap_or_default(),
    );
}

#[tauri::command]
pub fn mic_supported() -> bool {
    platform::mic_supported()
}

#[tauri::command]
pub fn get_language(app: AppHandle) -> String {
    crate::state::load_language(&app)
}

#[tauri::command]
pub fn set_language(app: AppHandle, language: String) -> Result<(), String> {
    crate::state::save_language(&app, &language);
    crate::refresh_tray_menu(&app, &language)
}

#[tauri::command]
pub fn get_locale() -> String {
    let locale = sys_locale::get_locale().unwrap_or_else(|| "en".into());
    let lower = locale.to_lowercase();
    if lower.starts_with("zh")
        && (lower.contains("hans") || lower.contains("-cn") || lower.contains("_cn"))
    {
        "zh-Hans".into()
    } else if lower.starts_with("ja") {
        "ja".into()
    } else {
        "en".into()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ShortcutRegistrationStatus {
    Enabled,
    Occupied,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutStatusDto {
    pub label: String,
    pub status: ShortcutRegistrationStatus,
}

fn shortcut_status(registered: bool) -> ShortcutStatusDto {
    ShortcutStatusDto {
        label: KIRI_CAPTURE.display_label(),
        status: if registered {
            ShortcutRegistrationStatus::Enabled
        } else {
            ShortcutRegistrationStatus::Occupied
        },
    }
}

fn require_library_window(window: &WebviewWindow) -> Result<(), String> {
    if window.label() == "library" {
        Ok(())
    } else {
        Err("This command is unavailable from this window.".into())
    }
}

#[tauri::command]
pub fn get_shortcut_status(window: WebviewWindow) -> Result<ShortcutStatusDto, String> {
    require_library_window(&window)?;
    Ok(shortcut_status(crate::capture_shortcut_is_registered(
        window.app_handle(),
    )))
}

#[tauri::command]
pub fn retry_shortcut(window: WebviewWindow) -> Result<ShortcutStatusDto, String> {
    require_library_window(&window)?;
    let app = window.app_handle();
    if !crate::capture_shortcut_is_registered(app) {
        if let Err(error) = crate::register_shortcut(app) {
            log::warn!("[shortcut] retry failed: {error}");
        }
    }
    Ok(shortcut_status(crate::capture_shortcut_is_registered(app)))
}

#[tauri::command]
pub fn open_settings(action: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let url = match action.as_str() {
            "openSettings" => {
                "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture"
            }
            "openInputMonitoringSettings" => {
                "x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent"
            }
            "openMicrophoneSettings" => {
                "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone"
            }
            _ => return Err("unknown action".into()),
        };
        let workspace = objc2_app_kit::NSWorkspace::sharedWorkspace();
        let url =
            objc2_foundation::NSURL::URLWithString(&objc2_foundation::NSString::from_str(url))
                .ok_or_else(|| "bad url".to_string())?;
        workspace.openURL(&url);
    }
    #[cfg(windows)]
    {
        let url = match action.as_str() {
            "openMicrophoneSettings" => "ms-settings:privacy-microphone",
            "openSettings" => "ms-settings:privacy",
            _ => return Err("unknown action".into()),
        };
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", url])
            .spawn();
    }
    Ok(())
}

#[tauri::command]
pub fn quit_app(app: AppHandle) -> Result<(), String> {
    app.exit(0);
    Ok(())
}

#[cfg(test)]
mod command_security_tests {
    use super::{
        capture_failure_requires_global_error, cleanup_finalization_files, commit_editor_update,
        crop_annotation_source, crop_editor_source, editor_save_destination,
        parse_editor_save_action, recording_channels, sanitize_frontend_log, validate_capture_png,
        validate_editor_annotation_document, validate_replacement_metadata,
        validate_staged_capture_annotation, write_editor_save, EditorCropPixels, EditorSaveAction,
        ShortcutRegistrationStatus, EDITOR_ACTION_INVALID_ERROR, EDITOR_SAVE_ERROR,
    };
    use crate::core::annotation::AnnotationDocument;
    use crate::core::asset::{CaptureAsset, CaptureKind};
    use crate::core::geometry::Rect;
    use crate::core::policy::RecordingOptions;
    use crate::state::ApprovedEditorSave;
    use std::cell::Cell;
    use std::collections::HashMap;
    use std::io::Cursor;
    use std::path::PathBuf;
    use std::sync::mpsc::TrySendError;

    #[test]
    fn frontend_error_log_is_single_line_and_bounded() {
        let sanitized = sanitize_frontend_log(format!("first\nsecond\0{}", "界".repeat(2_000)));
        assert!(!sanitized.chars().any(char::is_control));
        assert!(sanitized.len() <= 4 * 1024);
        assert!(sanitized.starts_with("first second "));
    }

    #[test]
    fn shortcut_status_reflects_native_registration() {
        let enabled = super::shortcut_status(true);
        assert_eq!(
            enabled.label,
            crate::core::shortcut::KIRI_CAPTURE.display_label()
        );
        assert_eq!(enabled.status, ShortcutRegistrationStatus::Enabled);
        assert_eq!(serde_json::to_value(&enabled).unwrap()["status"], "enabled");

        let occupied = super::shortcut_status(false);
        assert_eq!(
            occupied.label,
            crate::core::shortcut::KIRI_CAPTURE.display_label()
        );
        assert_eq!(occupied.status, ShortcutRegistrationStatus::Occupied);
        assert_eq!(
            serde_json::to_value(&occupied).unwrap()["status"],
            "occupied"
        );
    }

    #[test]
    fn replacement_png_must_decode_and_match_the_indexed_dimensions() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("replacement.png");
        let mut encoded = Cursor::new(Vec::new());
        image::DynamicImage::new_rgba8(10, 20)
            .write_to(&mut encoded, image::ImageFormat::Png)
            .unwrap();
        std::fs::write(&path, encoded.into_inner()).unwrap();
        let mut asset = CaptureAsset {
            id: uuid::Uuid::new_v4(),
            kind: CaptureKind::Image,
            created_at: 1.0,
            filename: "missing.png".into(),
            title: None,
            tags: Vec::new(),
            pixel_width: 10,
            pixel_height: 20,
            duration: None,
            source_application: None,
            is_favorite: false,
            trashed_at: None,
        };

        assert!(validate_replacement_metadata(&asset, &path).is_ok());
        asset.pixel_width = 11;
        assert!(validate_replacement_metadata(&asset, &path).is_err());
        std::fs::write(&path, b"\x89PNG\r\n\x1a\ntruncated").unwrap();
        assert!(validate_replacement_metadata(&asset, &path).is_err());
    }

    #[test]
    fn raw_video_frame_queue_has_a_hard_capacity() {
        let (senders, _receivers) = recording_channels(RecordingOptions::default());
        for _ in 0..crate::capture::VIDEO_FRAME_QUEUE_CAPACITY {
            senders.video.try_send(vec![0]).unwrap();
        }
        assert!(matches!(
            senders.video.try_send(vec![1]),
            Err(TrySendError::Full(_))
        ));
    }

    #[test]
    fn failed_recording_import_retains_completed_segments_for_recovery() {
        let directory = tempfile::tempdir().unwrap();
        let segment = directory.path().join("completed.mp4");
        let merged = directory.path().join("merged.mp4");
        std::fs::write(&segment, b"completed segment").unwrap();
        std::fs::write(&merged, b"failed merged output").unwrap();

        cleanup_finalization_files(std::slice::from_ref(&segment), &merged, true, false);

        assert!(segment.exists(), "valid completed segment must be retained");
        assert!(merged.exists(), "a completed merge remains recoverable");

        cleanup_finalization_files(std::slice::from_ref(&segment), &merged, true, true);
        assert!(
            !segment.exists(),
            "durably retained segments may be removed"
        );
        assert!(
            !merged.exists(),
            "the durable copy replaces the merged temp"
        );
    }

    #[test]
    fn failed_merge_removes_only_its_partial_output() {
        let directory = tempfile::tempdir().unwrap();
        let segment = directory.path().join("completed.mp4");
        let merged = directory.path().join("partial-merge.mp4");
        std::fs::write(&segment, b"completed segment").unwrap();
        std::fs::write(&merged, b"partial merge").unwrap();

        cleanup_finalization_files(std::slice::from_ref(&segment), &merged, false, false);

        assert!(segment.exists());
        assert!(!merged.exists());
    }

    #[test]
    fn capture_confirmation_validates_png_before_consuming_the_session() {
        let image = image::DynamicImage::ImageRgba8(image::RgbaImage::new(8, 6));
        let mut png = Cursor::new(Vec::new());
        image.write_to(&mut png, image::ImageFormat::Png).unwrap();
        let png = png.into_inner();

        assert_eq!(validate_capture_png(&png, 8, 6).unwrap(), (8, 6));
        assert!(validate_capture_png(&png, 7, 6).is_err());
        assert!(validate_capture_png(&png[..png.len() - 1], 8, 6).is_err());
        assert!(validate_capture_png(b"not a png", 8, 6).is_err());

        let mut oversized_header = png.clone();
        oversized_header[16..20].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(validate_capture_png(&oversized_header, 8, 6).is_err());
    }

    #[test]
    fn capture_double_failure_requires_a_global_error_after_the_overlay_closes() {
        assert!(capture_failure_requires_global_error(false));
        assert!(!capture_failure_requires_global_error(true));
    }

    #[test]
    fn editable_capture_source_matches_the_staged_pixel_dimensions() {
        let image = image::RgbaImage::from_fn(8, 6, |x, y| image::Rgba([x as u8, y as u8, 0, 255]));
        let mut png = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        let display = crate::capture::CapturedDisplay {
            png_data: png.into_inner().into(),
            pixel_width: 8,
            pixel_height: 6,
            screen_frame: Rect::new(0.0, 0.0, 4.0, 3.0),
            window_rects: Vec::new(),
            display_id: 1,
            display_identity: None,
            backing_scale: 2.0,
        };
        let selection = Rect::new(0.25, 0.5, 2.0, 1.5);
        let document = AnnotationDocument::from_json(
            r#"{"schemaVersion":1,"canvas":{"width":2,"height":1.5},"sourcePixels":{"width":4,"height":3},"marks":[{"kind":"mosaic","id":1,"points":[{"x":1,"y":1}],"brushDiameter":20,"intensity":"standard","style":"pixel"}]}"#,
        )
        .unwrap();
        validate_staged_capture_annotation(&display, selection, &document).unwrap();
        let source = crop_annotation_source(&display, selection, document.source_pixels).unwrap();
        let cropped =
            image::load_from_memory_with_format(&source, image::ImageFormat::Png).unwrap();
        assert_eq!((cropped.width(), cropped.height()), (4, 3));
        assert_eq!(cropped.to_rgba8().get_pixel(0, 0).0, [1, 1, 0, 255]);
    }

    #[test]
    fn editor_document_must_match_asset_pixels_and_aspect_ratio() {
        let document = AnnotationDocument::from_json(
            r#"{"schemaVersion":1,"canvas":{"width":400,"height":300},"sourcePixels":{"width":800,"height":600},"marks":[]}"#,
        )
        .unwrap();
        validate_editor_annotation_document(&document, (800, 600)).unwrap();
        assert!(validate_editor_annotation_document(&document, (801, 600)).is_err());

        let mut wrong_aspect = document;
        wrong_aspect.canvas.width = 500.0;
        assert!(validate_editor_annotation_document(&wrong_aspect, (800, 600)).is_err());
    }

    #[test]
    fn editor_crop_uses_the_exact_staged_source_pixels_and_rejects_bad_bounds() {
        let image = image::RgbaImage::from_fn(8, 6, |x, y| image::Rgba([x as u8, y as u8, 0, 255]));
        let mut png = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        let png = png.into_inner();

        let cropped = crop_editor_source(
            &png,
            (8, 6),
            EditorCropPixels {
                x: 2,
                y: 1,
                width: 4,
                height: 3,
            },
        )
        .unwrap();
        let cropped = image::load_from_memory_with_format(&cropped, image::ImageFormat::Png)
            .unwrap()
            .to_rgba8();
        assert_eq!(cropped.dimensions(), (4, 3));
        assert_eq!(cropped.get_pixel(0, 0).0, [2, 1, 0, 255]);
        assert_eq!(cropped.get_pixel(3, 2).0, [5, 3, 0, 255]);

        assert!(crop_editor_source(
            &png,
            (8, 6),
            EditorCropPixels {
                x: 7,
                y: 0,
                width: 2,
                height: 1,
            },
        )
        .is_err());

        let smaller = image::DynamicImage::new_rgba8(4, 3);
        let mut smaller_png = Cursor::new(Vec::new());
        smaller
            .write_to(&mut smaller_png, image::ImageFormat::Png)
            .unwrap();
        assert!(crop_editor_source(
            &smaller_png.into_inner(),
            (8, 6),
            EditorCropPixels {
                x: 0,
                y: 0,
                width: 4,
                height: 3,
            },
        )
        .is_err());
    }

    #[test]
    fn editor_save_action_matches_its_native_destination() {
        assert_eq!(
            parse_editor_save_action(None, false).unwrap_err(),
            EDITOR_ACTION_INVALID_ERROR
        );
        assert_eq!(
            parse_editor_save_action(Some("save"), false).unwrap(),
            EditorSaveAction::Save
        );
        assert_eq!(
            parse_editor_save_action(Some("save-as"), true).unwrap(),
            EditorSaveAction::SaveAs
        );
        assert!(parse_editor_save_action(Some("save"), true).is_err());
        assert!(parse_editor_save_action(Some("save-as"), false).is_err());
    }

    #[test]
    fn editor_save_destination_requires_the_native_token_without_consuming_it() {
        let label = "editor-00000000-0000-0000-0000-000000000001";
        let token = uuid::Uuid::new_v4();
        let path = PathBuf::from("/tmp/kiri-approved.png");
        let destinations = HashMap::from([(
            label.to_string(),
            ApprovedEditorSave {
                token,
                path: path.clone(),
            },
        )]);

        assert_eq!(
            editor_save_destination(&destinations, label, Some(token)).unwrap(),
            Some(path.clone())
        );
        assert_eq!(
            editor_save_destination(&destinations, label, Some(token)).unwrap(),
            Some(path)
        );
        assert!(editor_save_destination(&destinations, label, Some(uuid::Uuid::new_v4())).is_err());
        assert!(destinations.contains_key(label));
    }

    #[test]
    fn editor_revision_mismatch_retains_tokens_and_skips_external_action() {
        let label = "editor-00000000-0000-0000-0000-000000000001";
        let mut annotations = HashMap::from([(label, "annotation-token")]);
        let mut destinations = HashMap::from([(label, "save-token")]);
        let external_action_ran = Cell::new(false);

        let result = commit_editor_update(
            || Err::<String, _>("revision mismatch"),
            || {
                annotations.remove(label);
                destinations.remove(label);
                external_action_ran.set(true);
                true
            },
        );

        assert_eq!(result, Err("revision mismatch"));
        assert_eq!(annotations.get(label), Some(&"annotation-token"));
        assert_eq!(destinations.get(label), Some(&"save-token"));
        assert!(!external_action_ran.get());
    }

    #[test]
    fn editor_committed_action_returns_revision_even_when_external_action_fails() {
        let label = "editor-00000000-0000-0000-0000-000000000001";
        let mut annotations = HashMap::from([(label, "annotation-token")]);
        let mut destinations = HashMap::from([(label, "save-token")]);

        let result = commit_editor_update(
            || Ok::<String, &str>("next-revision".into()),
            || {
                annotations.remove(label);
                destinations.remove(label);
                false
            },
        )
        .unwrap();

        assert_eq!(result, ("next-revision".into(), false));
        assert!(!annotations.contains_key(label));
        assert!(!destinations.contains_key(label));
    }

    #[test]
    fn editor_save_failures_return_stable_errors() {
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("capture.png");
        std::fs::write(&output, b"old").unwrap();
        write_editor_save(&output, b"new-png").unwrap();
        assert_eq!(std::fs::read(&output).unwrap(), b"new-png");

        assert_eq!(
            write_editor_save(directory.path(), b"png").unwrap_err(),
            EDITOR_SAVE_ERROR
        );
        assert!(directory.path().is_dir());
        assert!(!std::fs::read_dir(directory.path())
            .unwrap()
            .any(|entry| entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".kiri-export-")));
    }
}

#[tauri::command]
pub fn get_recording_options(app: AppHandle) -> Result<RecordingOptions, String> {
    let options = {
        let state = app.state::<AppState>();
        let guard = state.saved_recording_options.lock().unwrap();
        *guard
    };
    Ok(options)
}

#[tauri::command]
pub fn get_annotation_appearance(app: AppHandle) -> Result<AnnotationAppearance, String> {
    let appearance = {
        let state = app.state::<AppState>();
        let saved = *state.saved_annotation_appearance.lock().unwrap();
        saved
    };
    Ok(appearance)
}

#[tauri::command]
pub fn set_annotation_appearance(
    app: AppHandle,
    appearance: AnnotationAppearance,
) -> Result<(), String> {
    let normalized = appearance.normalized();
    let state = app.state::<AppState>();
    let mut saved = state.saved_annotation_appearance.lock().unwrap();
    crate::state::save_annotation_appearance(&app, &normalized)
        .map_err(|_| "The annotation preferences could not be saved.".to_string())?;
    *saved = normalized;
    Ok(())
}

#[tauri::command]
pub fn set_recording_options(app: AppHandle, options: RecordingOptions) -> Result<(), String> {
    let normalized = options.normalized();
    {
        let state = app.state::<AppState>();
        *state.saved_recording_options.lock().unwrap() = normalized;
    }
    crate::state::save_recording_options(&app, &normalized);
    Ok(())
}
