//! Tauri command surface and application orchestration.
//! Synchronous commands run on the main thread; heavy work is spawned onto
//! background threads.

use std::path::PathBuf;
use std::sync::mpsc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use crate::capture::current as capture_backend;
use crate::core::asset::{CaptureAsset, CaptureKind};
use crate::core::geometry::Rect;
use crate::core::policy::RecordingOptions;
use crate::core::shortcut::KIRI_CAPTURE;
use crate::platform;
#[cfg(target_os = "macos")]
use crate::state::RecoveryAction;
use crate::state::{
    emit_error, emit_library_changed, emit_notice, emit_notice_local, emit_recording_state,
    ActiveRecording, AppState, CaptureSession, RecordingConfiguration, RecordingFlow,
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
pub fn move_to_trash(app: AppHandle, id: String) -> Result<(), String> {
    with_asset_mutation(&app, &id, |library, parsed| {
        library.move_to_trash(parsed).map_err(|e| e.to_string())
    })?;
    emit_notice_local(&app, "Moved to Trash".into(), "trash".into());
    Ok(())
}

#[tauri::command]
pub fn restore_asset(app: AppHandle, id: String) -> Result<(), String> {
    with_asset_mutation(&app, &id, |library, parsed| {
        library.restore(parsed).map_err(|e| e.to_string())
    })?;
    emit_notice_local(
        &app,
        "Restored to Library".into(),
        "arrow.uturn.backward".into(),
    );
    Ok(())
}

#[tauri::command]
pub fn permanently_delete(app: AppHandle, id: String) -> Result<(), String> {
    with_asset_mutation(&app, &id, |library, parsed| {
        library
            .permanently_delete(parsed)
            .map_err(|e| e.to_string())
    })?;
    emit_notice_local(&app, "Deleted Permanently".into(), "trash.fill".into());
    Ok(())
}

#[tauri::command]
pub fn empty_trash(app: AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let mut library = state.library.lock().unwrap();
    library.empty_trash().map_err(|e| e.to_string())?;
    drop(library);
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
    {
        let state = app.state::<AppState>();
        let mut library = state.library.lock().unwrap();
        for id in &parsed {
            library.permanently_delete(id).map_err(|e| e.to_string())?;
        }
    }
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
pub fn copy_asset(app: AppHandle, id: String) -> Result<(), String> {
    let parsed = uuid::Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    let state = app.state::<AppState>();
    let library = state.library.lock().unwrap();
    let asset = library
        .asset_by_id(&parsed)
        .cloned()
        .ok_or_else(|| "The capture could not be found.".to_string())?;
    if asset.kind != CaptureKind::Image {
        return Err("Only images can be copied.".to_string());
    }
    let path = state.asset_file_url(&asset);
    drop(library);
    let data = std::fs::read(&path).map_err(|e| e.to_string())?;
    platform::write_image_to_clipboard(&data).map_err(|e| e.to_string())?;
    emit_notice_local(
        &app,
        "Copied to Clipboard".into(),
        "checkmark.circle.fill".into(),
    );
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
pub fn convert_to_gif(app: AppHandle, id: String) -> Result<(), String> {
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
    {
        let state = app.state::<AppState>();
        let mut converting = state.gif_conversion_ids.lock().unwrap();
        if converting.contains(&parsed) {
            return Err("Already converting.".into());
        }
        converting.insert(parsed);
    }
    let handle = app.clone();
    std::thread::spawn(move || {
        let result = convert_asset_to_gif(&handle, &asset, &source_path);
        let state = handle.state::<AppState>();
        state.gif_conversion_ids.lock().unwrap().remove(&parsed);
        if let Err(error) = result {
            emit_error(&handle, error, None);
        }
    });
    Ok(())
}

fn convert_asset_to_gif(
    app: &AppHandle,
    asset: &CaptureAsset,
    source_path: &std::path::Path,
) -> Result<(), String> {
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
    let import_result = state
        .library
        .lock()
        .unwrap()
        .import_file(
            &gif_path,
            CaptureKind::Gif,
            "gif",
            asset.pixel_width,
            asset.pixel_height,
            asset.duration,
            asset.source_application.clone(),
        )
        .map_err(|e| e.to_string());
    let _ = std::fs::remove_file(&gif_path);
    import_result?;
    emit_library_changed(app);
    emit_notice(app, "GIF Created".into(), "sparkles.rectangle.stack".into());
    Ok(())
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
    if let Ok(ns_window) = window.ns_window() {
        let ns_window = ns_window as *mut NSWindow;
        let ns_window = unsafe { &*ns_window };
        ns_window.setLevel(NSScreenSaverWindowLevel);
    }
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

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmCaptureRequest {
    pub png: Vec<u8>,
    pub action: String,
}

#[tauri::command]
pub fn confirm_capture(app: AppHandle, request: ConfirmCaptureRequest) -> Result<(), String> {
    let state = app.state::<AppState>();
    let mut capture = state.capture.lock().unwrap();
    let Some(session) = capture.session.take() else {
        return Err("No active capture session.".into());
    };
    drop(capture);
    invalidate_capture_resources(&app, &session);
    // Errors must be visible: show the library window before emitting.
    let session_failure = match confirm_capture_inner(&app, &state, session, request) {
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
    request: ConfirmCaptureRequest,
) -> Result<(), String> {
    log::info!(
        "confirm_capture: action={} bytes={}",
        request.action,
        request.png.len()
    );

    for label in &session.overlay_labels {
        if let Some(window) = app.get_webview_window(label) {
            let _ = window.close();
        }
    }

    let action = request.action.clone();
    if action == "copy" {
        if let Err(error) = platform::write_image_to_clipboard(&request.png) {
            log::error!("confirm_capture: clipboard write failed: {error}");
            emit_error(
                app,
                "Could not copy the capture to the clipboard.".into(),
                None,
            );
        } else {
            emit_notice(
                app,
                "Copied to Clipboard".into(),
                "checkmark.circle.fill".into(),
            );
        }
    }

    let image = image::load_from_memory(&request.png).map_err(|e| e.to_string())?;
    let (pixel_width, pixel_height) = (image.width() as i64, image.height() as i64);
    let mut library = state.library.lock().unwrap();
    let imported = library
        .import_data(
            &request.png,
            CaptureKind::Image,
            "png",
            pixel_width,
            pixel_height,
            None,
            session.source_application.clone(),
            None,
        )
        .map_err(|e| e.to_string())?;
    drop(library);
    emit_library_changed(app);

    match action.as_str() {
        "save" => {
            // Spec §5.4: default name kiri-<timestamp>.png; write the PNG to
            // the chosen location after the save panel closes.
            let now = chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string();
            let default_name = format!("kiri-{now}.png");
            if let Ok(Some(path)) = save_file_dialog(app.clone(), default_name) {
                if let Err(error) = std::fs::write(&path, &request.png) {
                    log::error!("confirm_capture: save failed: {error}");
                    emit_error(app, "Could not save the capture.".into(), None);
                } else {
                    emit_notice(app, "Saved".into(), "checkmark.circle.fill".into());
                }
            }
        }
        "pin" => {
            let store = app.state::<crate::protocol::ProtocolStore>();
            store
                .pin_images
                .lock()
                .unwrap()
                .insert(imported.id.to_string(), request.png.clone());
            let label = format!("pin-{}", imported.id);
            let builder = WebviewWindowBuilder::new(
                app,
                label,
                WebviewUrl::App(format!("index.html?window=pin&id={}", imported.id).into()),
            )
            .title("kiri")
            .decorations(false)
            .always_on_top(true)
            .shadow(false)
            .transparent(true);
            let _ = builder.build();
        }
        "edit" => {
            let label = format!("editor-{}", imported.id);
            let builder = WebviewWindowBuilder::new(
                app,
                label,
                WebviewUrl::App(format!("index.html?window=editor&id={}", imported.id).into()),
            )
            .title("Kiri Editor")
            .inner_size(880.0, 620.0)
            .min_inner_size(860.0, 520.0)
            .center();
            if let Ok(window) = builder.build() {
                let _ = window.set_focus();
            }
        }
        _ => {}
    }

    restore_focus(app, &session);
    Ok(())
}

#[tauri::command]
pub fn save_file_dialog(app: AppHandle, default_name: String) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    // Localize the filter label from the persisted language preference.
    let filter_label = match crate::state::load_language(&app).as_str() {
        "zh-Hans" => "PNG 图片",
        "ja" => "PNG 画像",
        _ => "PNG image",
    };
    let path = app
        .dialog()
        .file()
        .set_file_name(default_name)
        .add_filter(filter_label, &["png"])
        .blocking_save_file();
    Ok(path
        .and_then(|p| p.into_path().ok())
        .map(|p| p.display().to_string()))
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

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAssetRequest {
    pub png: Vec<u8>,
    pub copy_to_clipboard: bool,
    pub save_path: Option<String>,
}

#[tauri::command]
pub fn update_asset(app: AppHandle, id: String, request: UpdateAssetRequest) -> Result<(), String> {
    let parsed = uuid::Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    let state = app.state::<AppState>();
    let mut library = state.library.lock().unwrap();
    library
        .replace_data(&request.png, &parsed)
        .map_err(|e| e.to_string())?;
    drop(library);
    if let Some(save_path) = &request.save_path {
        let _ = std::fs::write(save_path, &request.png);
    }
    if request.copy_to_clipboard && platform::write_image_to_clipboard(&request.png).is_ok() {
        emit_notice(
            &app,
            "Copied to Clipboard".into(),
            "checkmark.circle.fill".into(),
        );
    }
    emit_library_changed(&app);
    Ok(())
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

    let mut options = request.options.normalized();
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

    let (display_id, backing_scale, screen_frame, return_pid, was_kiri_frontmost) = {
        let state = app.state::<AppState>();
        let mut capture = state.capture.lock().unwrap();
        let Some(session) = capture.session.take() else {
            return Err("No active capture session.".into());
        };
        drop(capture);
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
        )
    };

    // Restore focus to the source application (mirrors AppModel.onRecord).
    if !was_kiri_frontmost {
        if let Some(pid) = return_pid {
            platform::activate_application(pid);
        }
    }

    crate::state::save_recording_options(&app, &options);

    {
        let state = app.state::<AppState>();
        let mut recording = state.recording.lock().unwrap();
        *recording = RecordingFlow {
            return_pid,
            was_kiri_frontmost,
            is_starting: true,
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
    video: mpsc::Sender<Vec<u8>>,
    system_audio: Option<mpsc::Sender<Vec<u8>>>,
    microphone: Option<mpsc::Sender<Vec<u8>>>,
}

struct EncoderReceivers {
    video: mpsc::Receiver<Vec<u8>>,
    system_audio: Option<mpsc::Receiver<Vec<u8>>>,
    microphone: Option<mpsc::Receiver<Vec<u8>>>,
}

fn recording_channels(options: RecordingOptions) -> (RecorderSenders, EncoderReceivers) {
    let (video_tx, video_rx) = mpsc::channel();
    let (audio_tx, audio_rx) = options
        .captures_system_audio
        .then(mpsc::channel)
        .map(|(tx, rx)| (Some(tx), Some(rx)))
        .unwrap_or((None, None));
    let (mic_tx, mic_rx) = options
        .captures_microphone
        .then(mpsc::channel)
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
        video_encoder: crate::record::pick_video_encoder(ffmpeg),
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

async fn resolve_ffmpeg(app: AppHandle) -> Result<PathBuf, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        state.ffmpeg().map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("video encoder preparation task failed: {error}"))?
}

#[tauri::command]
pub async fn begin_recording(app: AppHandle) -> Result<(), String> {
    log::info!("begin_recording: called");
    if let Some(window) = app.get_webview_window("countdown") {
        let _ = window.close();
    }

    let (startup_token, configuration) = {
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
        (token, configuration)
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

    // Resolve and, on first use, download + verify ffmpeg before channels or
    // native capture exist. Retina BGRA frames can arrive at hundreds of MB/s;
    // starting capture first would let the unbounded pipe grow throughout a
    // network download. spawn_blocking keeps that work off Tauri's event loop.
    let ffmpeg = match resolve_ffmpeg(app.clone()).await {
        Ok(path) => path,
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
        &ffmpeg,
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

    // Spec (app-orchestration §2.6 / recording §7): a 250ms recording clock
    // refreshes the elapsed time in the control panel while recording.
    {
        let handle = app.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_millis(250));
            let should_continue = {
                let state = handle.state::<AppState>();
                let recording = state.recording.lock().unwrap();
                if recording.is_recording && !recording.is_paused {
                    emit_recording_state(&handle, &recording);
                    true
                } else {
                    false
                }
            };
            if !should_continue {
                break;
            }
        });
    }

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
        emit_error(&app, "Could not pause screen recording.".into(), None);
        reset_recording_session(&app);
        return Err(error);
    }
    let state = app.state::<AppState>();
    let mut recording = state.recording.lock().unwrap();
    if let Some(path) = segment_path {
        recording.segments.push(path);
    }
    let elapsed = crate::state::recording_state(&recording).elapsed;
    recording.elapsed_before_segment = elapsed;
    recording.is_paused = true;
    recording.is_transitioning = false;
    recording.started_at = None;
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

    // Resolve the encoder before recreating unbounded capture channels. A
    // normal resume hits AppState's initialized path immediately; the async
    // boundary also makes inconsistent/direct IPC fail safely.
    let ffmpeg = match resolve_ffmpeg(app.clone()).await {
        Ok(path) => path,
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
        &ffmpeg,
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

    let (segments, active, needs_active_segment, return_pid, was_kiri_frontmost) = {
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
            recording.segments.clone(),
            recording.active.take(),
            needs_active_segment,
            recording.return_pid,
            recording.was_kiri_frontmost,
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
        if let Some(error) = failure {
            for segment in &final_segments {
                let _ = std::fs::remove_file(segment);
            }
            reset_recording_session(&handle);
            emit_error(&handle, error, None);
        } else {
            let result = finalize_recording(&handle, final_segments);
            reset_recording_session(&handle);
            match result {
                Ok(()) => emit_notice(&handle, "Recording Saved".into(), "video.fill".into()),
                Err(error) => emit_error(&handle, error, None),
            }
        }
        if !was_kiri_frontmost {
            if let Some(pid) = return_pid {
                platform::activate_application(pid);
            }
        }
    });
    Ok(())
}

fn finalize_recording(app: &AppHandle, segments: Vec<PathBuf>) -> Result<(), String> {
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
        let mut library = state.library.lock().unwrap();
        library
            .import_file(
                &merged_path,
                CaptureKind::Video,
                "mp4",
                pixel_width,
                pixel_height,
                duration,
                None,
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    })();
    for segment in &segments {
        let _ = std::fs::remove_file(segment);
    }
    let _ = std::fs::remove_file(&merged_path);
    if result.is_ok() {
        emit_library_changed(app);
    }
    result
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
    ) || ["viewer-", "pin-", "editor-"]
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
    use super::sanitize_frontend_log;

    #[test]
    fn frontend_error_log_is_single_line_and_bounded() {
        let sanitized = sanitize_frontend_log(format!("first\nsecond\0{}", "界".repeat(2_000)));
        assert!(!sanitized.chars().any(char::is_control));
        assert!(sanitized.len() <= 4 * 1024);
        assert!(sanitized.starts_with("first second "));
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
