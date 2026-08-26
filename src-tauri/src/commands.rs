//! Tauri command surface and application orchestration.
//! Synchronous commands run on the main thread; heavy work is spawned onto
//! background threads.

use std::path::PathBuf;
use std::sync::mpsc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use crate::capture::current as capture_backend;
use crate::core::asset::{CaptureAsset, CaptureKind};
use crate::core::geometry::Rect;
use crate::core::policy::{RecordingOptions, RecordingOutputFormat};
use crate::core::shortcut::KIRI_CAPTURE;
use crate::platform;
#[cfg(target_os = "macos")]
use crate::state::RecoveryAction;
use crate::state::{
    emit_asset_content_changed, emit_error, emit_library_changed, emit_notice, emit_notice_local,
    emit_notice_on_monitor, emit_recording_state, show_completion_preview, ActiveRecording,
    AppState, CaptureSession, CompletionPreviewDto, RecordingConfiguration, RecordingFlow,
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
    pub file_path: String,
    pub gif_eligible: bool,
}

fn asset_dto(asset: &CaptureAsset, root: &std::path::Path) -> AssetDto {
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
        file_path: root
            .join("Assets")
            .join(&asset.filename)
            .display()
            .to_string(),
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
    let library = state.library.lock().unwrap();
    let assets = library.search(&query, showing_trash);
    let root = state.library_root.clone();
    Ok(assets.iter().map(|asset| asset_dto(asset, &root)).collect())
}

fn with_asset_mutation(
    app: &AppHandle,
    id: &str,
    mutation: impl FnOnce(&mut crate::core::library::AssetLibrary, &uuid::Uuid) -> Result<(), String>,
) -> Result<(), String> {
    let parsed = uuid::Uuid::parse_str(id).map_err(|e| e.to_string())?;
    let state = app.state::<AppState>();
    let mut library = state.library.lock().unwrap();
    mutation(&mut library, &parsed)?;
    drop(library);
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
    crate::protocol::with_thumbnail_invalidation(&store, parsed, || {
        state
            .library
            .lock()
            .unwrap()
            .permanently_delete(&parsed)
            .map_err(|e| e.to_string())
    })?;
    emit_library_changed(&app);
    emit_notice_local(&app, "Deleted Permanently".into(), "trash.fill".into());
    Ok(())
}

#[tauri::command]
pub fn empty_trash(app: AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let removed_ids = state
        .library
        .lock()
        .unwrap()
        .all_assets(true)
        .into_iter()
        .map(|asset| asset.id)
        .collect::<Vec<_>>();
    let store = app.state::<crate::protocol::ProtocolStore>();
    crate::protocol::with_thumbnail_invalidations(&store, &removed_ids, || {
        state
            .library
            .lock()
            .unwrap()
            .empty_trash()
            .map_err(|e| e.to_string())
    })?;
    emit_library_changed(&app);
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
        let mut library = state.library.lock().unwrap();
        for id in &parsed {
            library.move_to_trash(id).map_err(|e| e.to_string())?;
        }
    }
    emit_library_changed(&app);
    Ok(())
}

#[tauri::command]
pub fn batch_restore(app: AppHandle, ids: Vec<String>) -> Result<(), String> {
    let parsed = parse_ids(&ids)?;
    {
        let state = app.state::<AppState>();
        let mut library = state.library.lock().unwrap();
        for id in &parsed {
            library.restore(id).map_err(|e| e.to_string())?;
        }
    }
    emit_library_changed(&app);
    Ok(())
}

#[tauri::command]
pub fn batch_permanently_delete(app: AppHandle, ids: Vec<String>) -> Result<(), String> {
    let parsed = parse_ids(&ids)?;
    let state = app.state::<AppState>();
    let store = app.state::<crate::protocol::ProtocolStore>();
    crate::protocol::with_thumbnail_invalidations(&store, &parsed, || {
        let mut library = state.library.lock().unwrap();
        for id in &parsed {
            library.permanently_delete(id).map_err(|e| e.to_string())?;
        }
        Ok::<(), String>(())
    })?;
    emit_library_changed(&app);
    Ok(())
}

#[tauri::command]
pub fn batch_set_favorite(app: AppHandle, ids: Vec<String>, favorite: bool) -> Result<(), String> {
    let parsed = parse_ids(&ids)?;
    {
        let state = app.state::<AppState>();
        let mut library = state.library.lock().unwrap();
        for id in &parsed {
            library
                .set_favorite(favorite, id)
                .map_err(|e| e.to_string())?;
        }
    }
    emit_library_changed(&app);
    Ok(())
}

#[tauri::command]
pub fn copy_asset(app: AppHandle, window: WebviewWindow, id: String) -> Result<(), String> {
    let parsed = uuid::Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    let state = app.state::<AppState>();
    let library = state.library.lock().unwrap();
    let asset = library
        .asset_by_id(&parsed)
        .cloned()
        .ok_or_else(|| "The capture could not be found.".to_string())?;
    let path = state.asset_file_url(&asset);
    drop(library);
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
    let library = state.library.lock().unwrap();
    let asset = library
        .asset_by_id(&parsed)
        .cloned()
        .ok_or_else(|| "The capture could not be found.".to_string())?;
    let (width, height) = (asset.pixel_width, asset.pixel_height);
    drop(library);

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
pub fn get_asset(app: AppHandle, id: String) -> Result<AssetDto, String> {
    let parsed = uuid::Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    let state = app.state::<AppState>();
    let library = state.library.lock().unwrap();
    let asset = library
        .asset_by_id(&parsed)
        .cloned()
        .ok_or_else(|| "The capture could not be found.".to_string())?;
    let root = state.library_root.clone();
    Ok(asset_dto(&asset, &root))
}

#[tauri::command]
pub fn reveal_asset(app: AppHandle, id: String) -> Result<(), String> {
    let parsed = uuid::Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    let state = app.state::<AppState>();
    let library = state.library.lock().unwrap();
    let asset = library
        .asset_by_id(&parsed)
        .cloned()
        .ok_or_else(|| "The capture could not be found.".to_string())?;
    let path = state.asset_file_url(&asset);
    drop(library);
    platform::reveal_path(&path);
    Ok(())
}

#[tauri::command]
pub fn convert_to_gif(app: AppHandle, window: WebviewWindow, id: String) -> Result<(), String> {
    let parsed = uuid::Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    let (asset, source_path) = {
        let state = app.state::<AppState>();
        let library = state.library.lock().unwrap();
        let asset = library
            .asset_by_id(&parsed)
            .cloned()
            .ok_or_else(|| "The capture could not be found.".to_string())?;
        let source_path = state.asset_file_url(&asset);
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
    // GIF export is an explicit user action, so it may perform the same
    // pinned, verified first-use installation as recording.
    let ffmpeg = state.ffmpeg().map_err(|error| error.to_string())?;
    let gif_path = crate::gif::export_gif(
        source_path,
        crate::core::policy::RecordingPolicy::MAXIMUM_GIF_LONG_EDGE,
        crate::core::policy::RecordingPolicy::GIF_FRAMES_PER_SECOND,
        &ffmpeg,
    )
    .map_err(|e| e.to_string())?;
    let (gif_width, gif_height, gif_duration) = crate::record::probe_video(&ffmpeg, &gif_path)
        .unwrap_or((asset.pixel_width, asset.pixel_height, asset.duration));
    let import_result = state
        .library
        .lock()
        .unwrap()
        .import_file(
            &gif_path,
            CaptureKind::Gif,
            "gif",
            gif_width,
            gif_height,
            gif_duration.or(asset.duration),
            asset.source_application.clone(),
        )
        .map_err(|e| e.to_string());
    let _ = std::fs::remove_file(&gif_path);
    let gif_asset = import_result?;
    emit_library_changed(app);
    Ok(gif_asset)
}

// ---------------------------------------------------------------------------
// Capture flow commands (main thread)
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn start_capture(app: AppHandle) -> Result<CaptureContextDto, String> {
    log::info!("start_capture: beginning capture flow");

    {
        let state = app.state::<AppState>();
        let capture = state.capture.lock().unwrap();
        // The overlay frontend calls start_capture again when it loads; return
        // the existing session context instead of failing.
        if let Some(session) = capture.session.as_ref() {
            let display = &session.display;
            return Ok(CaptureContextDto {
                display_width: display.screen_frame.width,
                display_height: display.screen_frame.height,
                scale: display.backing_scale,
                pixel_width: display.pixel_width,
                pixel_height: display.pixel_height,
                window_rects: display.window_rects.iter().map(RectDto::from).collect(),
                source_application: session.source_application.clone(),
            });
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

    let display = capture_backend::capture_active_display().map_err(|e| e.to_string())?;

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
    let capture_token = {
        let store = app.state::<crate::protocol::ProtocolStore>();
        crate::protocol::set_frozen_png(&store, capture_id, display.png_data.clone())
    };

    let overlay_label = "overlay".to_string();
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
        });
    }

    if let Err(error) = create_overlay_window(&app, overlay_frame, &capture_token) {
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

fn create_overlay_window(
    app: &AppHandle,
    screen_frame: Rect,
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
    let window = builder.build()?;
    raise_overlay_window(&window);
    if let Err(error) = window.set_focus() {
        let _ = window.close();
        return Err(error.into());
    }
    Ok(label)
}

#[cfg(target_os = "macos")]
fn raise_overlay_window(window: &tauri::WebviewWindow) {
    use objc2_app_kit::{NSScreenSaverWindowLevel, NSWindow};

    // Recording commands are async Tauri commands and therefore run on a
    // Tokio worker. AppKit traps the whole process if NSWindow is mutated
    // there, so keep the native pointer lookup and mutation on the main
    // thread. `run_on_main` executes inline for the synchronous capture path
    // and dispatches synchronously for countdown/control-panel creation.
    let window = window.clone();
    dispatch2::run_on_main(move |_main_thread| {
        if let Ok(ns_window) = window.ns_window() {
            let ns_window = ns_window as *mut NSWindow;
            let ns_window = unsafe { &*ns_window };
            ns_window.setLevel(NSScreenSaverWindowLevel);
        }
    });
}

#[cfg(not(target_os = "macos"))]
fn raise_overlay_window(_window: &tauri::WebviewWindow) {}

#[tauri::command]
pub fn cancel_capture(window: WebviewWindow, app: AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
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
        let _ = window.close();
        return Ok(());
    };
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
    if session.was_kiri_frontmost {
        for label in &session.hidden_windows {
            if let Some(window) = app.get_webview_window(label) {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }
    } else if let Some(pid) = session.return_pid {
        platform::activate_application(pid);
    }
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

#[tauri::command]
pub fn confirm_capture(app: AppHandle, request: tauri::ipc::Request<'_>) -> Result<(), String> {
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
    let (capture_id, max_width, max_height) = {
        let capture = state.capture.lock().unwrap();
        let session = capture
            .session
            .as_ref()
            .ok_or_else(|| "No active capture session.".to_string())?;
        (
            session.capture_id,
            session.display.pixel_width,
            session.display.pixel_height,
        )
    };
    // Validate the complete PNG before consuming the active session or writing
    // to the clipboard/library. This also bounds decoder allocation by the
    // dimensions of the frozen display that owns the request.
    let pixel_size = validate_capture_png(png, max_width, max_height)?;
    let session = {
        let mut capture = state.capture.lock().unwrap();
        if capture
            .session
            .as_ref()
            .is_none_or(|session| session.capture_id != capture_id)
        {
            return Err("The capture session changed before confirmation.".into());
        }
        capture.session.take().unwrap()
    };
    invalidate_capture_resources(&app, &session);
    // Errors must be visible: show the library window before emitting.
    let session_failure = match confirm_capture_inner(&app, &state, session, png, pixel_size) {
        Ok(()) => return Ok(()),
        Err(error) => error,
    };
    if let Some(window) = app.get_webview_window("library") {
        let _ = window.show();
    }
    Err(session_failure)
}

fn confirm_capture_inner(
    app: &AppHandle,
    state: &AppState,
    session: CaptureSession,
    png: &[u8],
    pixel_size: (i64, i64),
) -> Result<(), String> {
    log::info!("confirm_capture: bytes={}", png.len());

    let completion_monitor = session.overlay_labels.iter().find_map(|label| {
        app.get_webview_window(label)
            .and_then(|window| window.current_monitor().ok().flatten())
    });
    for label in &session.overlay_labels {
        if let Some(window) = app.get_webview_window(label) {
            let _ = window.close();
        }
    }

    let copied = match platform::write_image_to_clipboard(png) {
        Ok(()) => true,
        Err(error) => {
            log::error!("confirm_capture: clipboard write failed: {error}");
            false
        }
    };

    let (pixel_width, pixel_height) = pixel_size;
    let mut library = state.library.lock().unwrap();
    let import_result = library
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
        .map_err(|e| e.to_string());
    drop(library);
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
            return Err(error);
        }
    };
    emit_library_changed(app);

    let title = if copied {
        "Copied and Saved"
    } else {
        "Saved — Copy Failed"
    };
    show_completion_preview(
        app,
        &CompletionPreviewDto::ready(
            uuid::Uuid::new_v4().to_string(),
            &asset,
            title,
            completion_asset_detail(&asset),
            copied,
        ),
        completion_monitor,
    );

    restore_focus(app, &session);
    Ok(())
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
    app: AppHandle,
    default_name: String,
) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    // Localize the filter label from the persisted language preference.
    let filter_label = match crate::state::load_language(&app).as_str() {
        "zh-Hans" => "PNG 图片",
        "ja" => "PNG 画像",
        _ => "PNG image",
    }
    .to_string();
    tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_file_name(default_name)
            .add_filter(filter_label, &["png"])
            .blocking_save_file()
            .and_then(|path| path.into_path().ok())
            .map(|path| path.display().to_string())
    })
    .await
    .map_err(|error| format!("Could not open the save panel: {error}"))
}

fn restore_focus(app: &AppHandle, session: &CaptureSession) {
    if session.was_kiri_frontmost {
        for label in &session.hidden_windows {
            if let Some(window) = app.get_webview_window(label) {
                let _ = window.show();
            }
        }
        if let Some(window) = app.get_webview_window("library") {
            let _ = window.set_focus();
        }
    } else if let Some(pid) = session.return_pid {
        platform::activate_application(pid);
    }
}

// ---------------------------------------------------------------------------
// Editor command
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn update_asset(app: AppHandle, request: tauri::ipc::Request<'_>) -> Result<(), String> {
    let parsed = request
        .headers()
        .get("x-kiri-asset-id")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| uuid::Uuid::parse_str(value).ok())
        .ok_or_else(|| "The edited asset id is invalid.".to_string())?;
    let copy_to_clipboard = match request
        .headers()
        .get("x-kiri-copy-to-clipboard")
        .and_then(|value| value.to_str().ok())
    {
        Some("1") => true,
        Some("0") => false,
        _ => return Err("The editor action is invalid.".into()),
    };
    let save_path = request
        .headers()
        .get("x-kiri-save-path")
        .map(|value| {
            value
                .to_str()
                .map_err(|_| "The save path is invalid.".to_string())
                .and_then(decode_save_path_header)
        })
        .transpose()?;
    let png = match request.body() {
        tauri::ipc::InvokeBody::Raw(bytes) if !bytes.is_empty() => bytes.as_slice(),
        tauri::ipc::InvokeBody::Raw(_) => return Err("The edited image is empty.".into()),
        tauri::ipc::InvokeBody::Json(_) => {
            return Err("The edited image must use the binary IPC payload.".into())
        }
    };

    let state = app.state::<AppState>();
    let expected_size = {
        let library = state.library.lock().unwrap();
        let asset = library
            .asset_by_id(&parsed)
            .ok_or_else(|| "The edited asset could not be found.".to_string())?;
        if asset.kind != CaptureKind::Image {
            return Err("Only image captures can be edited.".into());
        }
        (asset.pixel_width, asset.pixel_height)
    };
    if validate_capture_png(png, expected_size.0, expected_size.1)? != expected_size {
        return Err("The edited image dimensions changed unexpectedly.".into());
    }
    let store = app.state::<crate::protocol::ProtocolStore>();
    crate::protocol::with_thumbnail_invalidation(&store, parsed, || {
        state.library.lock().unwrap().replace_data(png, &parsed)
    })
    .map_err(|e| e.to_string())?;
    if let Some(save_path) = &save_path {
        let _ = std::fs::write(save_path, png);
    }
    if copy_to_clipboard && platform::write_image_to_clipboard(png).is_ok() {
        emit_notice(
            &app,
            "Copied to Clipboard".into(),
            "checkmark.circle.fill".into(),
        );
    }
    emit_asset_content_changed(&app, &parsed);
    emit_library_changed(&app);
    Ok(())
}

fn decode_save_path_header(encoded: &str) -> Result<String, String> {
    if encoded.is_empty()
        || encoded.len() > 16 * 1024
        || encoded
            .bytes()
            .any(|byte| matches!(byte, b'&' | b'=' | b'+'))
    {
        return Err("The save path is invalid.".into());
    }
    let query = format!("path={encoded}");
    url::form_urlencoded::parse(query.as_bytes())
        .find(|(key, _)| key == "path")
        .map(|(_, path)| path.into_owned())
        .filter(|path| !path.is_empty())
        .ok_or_else(|| "The save path is invalid.".to_string())
}

/// Sets a friendly display title for a capture (metadata only; the on-disk
/// filename is unchanged so existing libraries stay compatible).
#[tauri::command]
pub fn rename_asset(app: AppHandle, id: String, title: String) -> Result<(), String> {
    let parsed = uuid::Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    let trimmed = title.trim().to_string();
    let state = app.state::<AppState>();
    let mut library = state.library.lock().unwrap();
    library
        .set_title(
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            },
            &parsed,
        )
        .map_err(|e| e.to_string())?;
    drop(library);
    emit_library_changed(&app);
    Ok(())
}

/// Replaces the tag list of a capture (metadata only).
#[tauri::command]
pub fn set_tags(app: AppHandle, id: String, tags: Vec<String>) -> Result<(), String> {
    let parsed = uuid::Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    let state = app.state::<AppState>();
    let mut library = state.library.lock().unwrap();
    library.set_tags(tags, &parsed).map_err(|e| e.to_string())?;
    drop(library);
    emit_library_changed(&app);
    Ok(())
}

#[tauri::command]
pub fn copy_text(app: AppHandle, text: String) -> Result<(), String> {
    platform::write_text_to_clipboard(&text).map_err(|e| e.to_string())?;
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

    let (
        display_id,
        backing_scale,
        screen_frame,
        return_pid,
        was_kiri_frontmost,
        completion_monitor,
    ) = {
        let state = app.state::<AppState>();
        let mut capture = state.capture.lock().unwrap();
        let Some(session) = capture.session.take() else {
            return Err("No active capture session.".into());
        };
        drop(capture);
        let completion_monitor = session.overlay_labels.iter().find_map(|label| {
            app.get_webview_window(label)
                .and_then(|window| window.current_monitor().ok().flatten())
        });
        invalidate_capture_resources(&app, &session);
        for label in &session.overlay_labels {
            if let Some(window) = app.get_webview_window(label) {
                let _ = window.close();
            }
        }
        (
            session.display.display_id,
            session.display.backing_scale,
            session.display.screen_frame,
            session.return_pid,
            session.was_kiri_frontmost,
            completion_monitor,
        )
    };

    // Restore focus to the source application (mirrors AppModel.onRecord).
    if !was_kiri_frontmost {
        if let Some(pid) = return_pid {
            platform::activate_application(pid);
        }
    }

    crate::state::save_recording_options(&app, &saved_options);

    {
        let state = app.state::<AppState>();
        *state.saved_recording_options.lock().unwrap() = saved_options;
        let mut recording = state.recording.lock().unwrap();
        *recording = RecordingFlow {
            session_id: uuid::Uuid::new_v4(),
            return_pid,
            was_kiri_frontmost,
            is_starting: true,
            completion_monitor,
            configuration: Some(RecordingConfiguration {
                display_id,
                region: Rect::new(
                    request.region.x,
                    request.region.y,
                    request.region.width,
                    request.region.height,
                ),
                screen_frame,
                backing_scale,
                options,
            }),
            ..Default::default()
        };
        emit_recording_state(&app, &recording);
    }

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
    platform::set_window_capture_excluded(&app, &label, true);
    // Spec (recording §5.1): the countdown window is level .screenSaver.
    raise_overlay_window(&window);
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
    finalize_recording(app, completed_segments, RecordingOutputFormat::Mp4).map(|_| true)
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
    platform::set_window_capture_excluded(app, "control-panel", true);
    // Keep the panel above every app window. It takes keyboard focus so the
    // recording hotkeys work (Space = pause/resume, Esc = stop); the panel
    // itself is the only window the user interacts with while recording.
    raise_overlay_window(&panel);
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
    let _ripple = WebviewWindowBuilder::new(
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
    .skip_taskbar(true)
    .shadow(false)
    .build()
    .map_err(|e| e.to_string())?;
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
        let recorder = crate::capture::windows::WindowsRecorder::start(
            configuration.display_id,
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
    ffmpeg: &std::path::Path,
    video_encoder: String,
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
    let encoder_config = crate::record::EncoderConfig {
        width,
        height,
        fps: crate::core::policy::RecordingPolicy::FRAMES_PER_SECOND,
        bitrate: crate::record::bitrate_for(width, height),
        audio: recorder.system_audio_spec,
        mic: recorder.microphone_spec,
        video_encoder,
    };
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

struct PreparedEncoder {
    ffmpeg: PathBuf,
    video_encoder: String,
}

async fn prepare_encoder(app: AppHandle) -> Result<PreparedEncoder, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let ffmpeg = state.ffmpeg().map_err(|error| error.to_string())?;
        // Hardware probing can launch several short ffmpeg processes on first
        // use. Finish it before native capture starts so live audio cannot fill
        // its bounded queue while no pipe writer exists yet.
        let video_encoder =
            crate::record::pick_video_encoder(&ffmpeg).map_err(|error| error.to_string())?;
        Ok(PreparedEncoder {
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

    // Resolve and, on first use, download + verify ffmpeg and probe the video
    // encoder before channels or native capture exist. Retina BGRA frames can
    // arrive at hundreds of MB/s; preparing first prevents bounded queues from
    // filling before their consumers exist. spawn_blocking keeps that work off
    // Tauri's event loop.
    let prepared = match prepare_encoder(app.clone()).await {
        Ok(prepared) => prepared,
        Err(error) => {
            log::error!("recording: ffmpeg preparation failed: {error}");
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
        &prepared.ffmpeg,
        prepared.video_encoder,
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
    let prepared = match prepare_encoder(app.clone()).await {
        Ok(prepared) => prepared,
        Err(error) => {
            log::error!("recording: resume ffmpeg preparation failed: {error}");
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
        &prepared.ffmpeg,
        prepared.video_encoder,
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
    ) = {
        let state = app.state::<AppState>();
        let mut recording = state.recording.lock().unwrap();
        if !(recording.is_recording || recording.is_paused) || recording.is_transitioning {
            return Ok(());
        }
        let needs_active_segment = recording.is_recording;
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
            let result = finalize_recording(&handle, final_segments, output_format);
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

fn import_recording_file(
    state: &AppState,
    path: &std::path::Path,
    kind: CaptureKind,
    extension: &str,
    pixel_width: i64,
    pixel_height: i64,
    duration: Option<f64>,
) -> Result<CaptureAsset, String> {
    state
        .library
        .lock()
        .unwrap()
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

fn finalize_recording(
    app: &AppHandle,
    segments: Vec<PathBuf>,
    output_format: RecordingOutputFormat,
) -> Result<RecordingFinalizeOutcome, String> {
    let state = app.state::<AppState>();
    let ffmpeg = state
        .ffmpeg_path
        .get()
        .cloned()
        .ok_or_else(|| "ffmpeg unavailable".to_string())?;
    let merged_path = std::env::temp_dir().join(format!(
        "kiri-recording-merged-{}.mp4",
        uuid::Uuid::new_v4().to_string().to_lowercase()
    ));
    let result = (|| {
        crate::record::merge_segments(&segments, &merged_path, &ffmpeg)
            .map_err(|e| e.to_string())?;
        let (pixel_width, pixel_height, duration) =
            crate::record::probe_video(&ffmpeg, &merged_path).unwrap_or((0, 0, None));

        if output_format == RecordingOutputFormat::Mp4 {
            let asset = import_recording_file(
                &state,
                &merged_path,
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
            let gif_path = crate::gif::export_gif(
                &merged_path,
                crate::core::policy::RecordingPolicy::MAXIMUM_GIF_LONG_EDGE,
                crate::core::policy::RecordingPolicy::GIF_FRAMES_PER_SECOND,
                &ffmpeg,
            )
            .map_err(|error| error.to_string())?;
            let (gif_width, gif_height, gif_duration) = crate::record::probe_video(
                &ffmpeg, &gif_path,
            )
            .unwrap_or((pixel_width, pixel_height, duration));
            let import_result = import_recording_file(
                &state,
                &gif_path,
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
                let asset = import_recording_file(
                    &state,
                    &merged_path,
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
    cleanup_finalization_files(&segments, &merged_path, result.is_ok());
    if result.is_ok() {
        emit_library_changed(app);
    }
    result
}

fn cleanup_finalization_files(segments: &[PathBuf], merged_path: &std::path::Path, imported: bool) {
    if imported {
        for segment in segments {
            let _ = std::fs::remove_file(segment);
        }
    }
    let _ = std::fs::remove_file(merged_path);
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
pub fn set_language(app: AppHandle, language: String) {
    crate::state::save_language(&app, &language);
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

#[tauri::command]
pub fn get_shortcut_label() -> String {
    KIRI_CAPTURE.display_label()
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
        cleanup_finalization_files, decode_save_path_header, recording_channels,
        sanitize_frontend_log, validate_capture_png,
    };
    use crate::core::policy::RecordingOptions;
    use std::io::Cursor;
    use std::sync::mpsc::TrySendError;

    #[test]
    fn frontend_error_log_is_single_line_and_bounded() {
        let sanitized = sanitize_frontend_log(format!("first\nsecond\0{}", "界".repeat(2_000)));
        assert!(!sanitized.chars().any(char::is_control));
        assert!(sanitized.len() <= 4 * 1024);
        assert!(sanitized.starts_with("first second "));
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

        cleanup_finalization_files(std::slice::from_ref(&segment), &merged, false);

        assert!(segment.exists(), "valid completed segment must be retained");
        assert!(
            !merged.exists(),
            "failed merge output is not recoverable data"
        );

        cleanup_finalization_files(std::slice::from_ref(&segment), &merged, true);
        assert!(
            !segment.exists(),
            "durably imported segments may be removed"
        );
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
    fn editor_save_path_header_round_trips_unicode_without_raw_separators() {
        assert_eq!(
            decode_save_path_header("%2Ftmp%2F%E6%B5%8B%E8%AF%95%20image.png").unwrap(),
            "/tmp/测试 image.png"
        );
        assert!(decode_save_path_header("/tmp/raw+path.png").is_err());
        assert!(decode_save_path_header("path&extra=value").is_err());
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
pub fn set_recording_options(app: AppHandle, options: RecordingOptions) -> Result<(), String> {
    let normalized = options.normalized();
    {
        let state = app.state::<AppState>();
        *state.saved_recording_options.lock().unwrap() = normalized;
    }
    crate::state::save_recording_options(&app, &normalized);
    Ok(())
}
