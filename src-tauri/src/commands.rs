//! Tauri command surface — the AppModel orchestration in Rust.
//! Synchronous commands run on the main thread (mirroring the Swift @MainActor
//! design); heavy work is spawned onto background threads.

use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_opener::OpenerExt;

use crate::capture::current as capture_backend;
use crate::core::asset::{CaptureAsset, CaptureKind};
use crate::core::geometry::Rect;
use crate::core::policy::RecordingOptions;
use crate::core::shortcut::KIRI_CAPTURE;
use crate::platform;
use crate::state::{
    emit_error, emit_library_changed, emit_notice, emit_recording_state, ActiveRecording, AppState,
    CaptureSession, RecordingConfiguration, RecordingFlow,
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
        pixel_width: asset.pixel_width,
        pixel_height: asset.pixel_height,
        duration: asset.duration,
        source_application: asset.source_application.clone(),
        is_favorite: asset.is_favorite,
        trashed_at: asset.trashed_at,
        file_path: root.join("Assets").join(&asset.filename).display().to_string(),
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
        library.set_favorite(favorite, parsed).map_err(|e| e.to_string())
    })
}

#[tauri::command]
pub fn move_to_trash(app: AppHandle, id: String) -> Result<(), String> {
    with_asset_mutation(&app, &id, |library, parsed| {
        library.move_to_trash(parsed).map_err(|e| e.to_string())
    })?;
    emit_notice(&app, "Moved to Trash".into(), "trash".into());
    Ok(())
}

#[tauri::command]
pub fn restore_asset(app: AppHandle, id: String) -> Result<(), String> {
    with_asset_mutation(&app, &id, |library, parsed| {
        library.restore(parsed).map_err(|e| e.to_string())
    })?;
    emit_notice(&app, "Restored to Library".into(), "arrow.uturn.backward".into());
    Ok(())
}

#[tauri::command]
pub fn permanently_delete(app: AppHandle, id: String) -> Result<(), String> {
    with_asset_mutation(&app, &id, |library, parsed| {
        library.permanently_delete(parsed).map_err(|e| e.to_string())
    })?;
    emit_notice(&app, "Deleted Permanently".into(), "trash.fill".into());
    Ok(())
}

#[tauri::command]
pub fn empty_trash(app: AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let mut library = state.library.lock().unwrap();
    library.empty_trash().map_err(|e| e.to_string())?;
    drop(library);
    emit_library_changed(&app);
    emit_notice(&app, "Trash Emptied".into(), "trash.slash".into());
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
    emit_notice(&app, "Copied to Clipboard".into(), "checkmark.circle.fill".into());
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
    let path = state.asset_file_url(&asset);
    drop(library);
    app.opener()
        .open_path(path.display().to_string(), None::<&str>)
        .map_err(|e| e.to_string())
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
    {
        let state = app.state::<AppState>();
        let mut converting = state.gif_conversion_ids.lock().unwrap();
        if converting.contains(&parsed) {
            return Err("Already converting.".into());
        }
        converting.insert(parsed);
    }
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
    let ffmpeg = state
        .ffmpeg_path
        .get()
        .cloned()
        .ok_or_else(|| "ffmpeg unavailable".to_string())?;
    let gif_path = crate::gif::export_gif(
        source_path,
        None,
        crate::core::policy::RecordingPolicy::MAXIMUM_GIF_LONG_EDGE,
        crate::core::policy::RecordingPolicy::GIF_FRAMES_PER_SECOND,
        &ffmpeg,
    )
    .map_err(|e| e.to_string())?;
    let mut library = state.library.lock().unwrap();
    let imported = library
        .import_file(
            &gif_path,
            CaptureKind::Gif,
            "gif",
            asset.pixel_width,
            asset.pixel_height,
            asset.duration,
            asset.source_application.clone(),
        )
        .map_err(|e| e.to_string())?;
    drop(library);
    let _ = imported;
    let _ = std::fs::remove_file(&gif_path);
    emit_library_changed(app);
    emit_notice(app, "GIF Created".into(), "sparkles.rectangle.stack".into());
    Ok(())
}

// ---------------------------------------------------------------------------
// Capture flow commands (main thread)
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn start_capture(app: AppHandle) -> Result<CaptureContextDto, String> {
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

    {
        let store = app.state::<crate::protocol::ProtocolStore>();
        crate::protocol::set_frozen_png(&store, display.png_data.clone());
    }

    let overlay_label = create_overlay_window(&app, &display).map_err(|e| e.to_string())?;

    {
        let state = app.state::<AppState>();
        let mut capture = state.capture.lock().unwrap();
        capture.session = Some(CaptureSession {
            display,
            source_application: name,
            return_pid: pid,
            was_kiri_frontmost,
            hidden_windows,
            overlay_labels: vec![overlay_label],
        });
    }

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
    display: &crate::capture::CapturedDisplay,
) -> anyhow::Result<String> {
    let label = "overlay".to_string();
    let physical = |value: f64| (value * display.backing_scale).round();
    let builder = WebviewWindowBuilder::new(
        app,
        label.clone(),
        WebviewUrl::App("index.html?window=overlay".into()),
    )
    .title("kiri")
    .inner_size(
        physical(display.screen_frame.width),
        physical(display.screen_frame.height),
    )
    .position(
        physical(display.screen_frame.x),
        physical(display.screen_frame.y),
    )
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .resizable(false)
    .shadow(false)
    .visible(false);
    let window = builder.build()?;
    raise_overlay_window(&window);
    window.show()?;
    window.set_focus()?;
    Ok(label)
}

#[cfg(target_os = "macos")]
fn raise_overlay_window(window: &tauri::WebviewWindow) {
    use objc2_app_kit::{NSWindow, NSScreenSaverWindowLevel};
    if let Ok(ns_window) = window.ns_window() {
        let ns_window = ns_window as *mut NSWindow;
        let ns_window = unsafe { &*ns_window };
        unsafe { ns_window.setLevel(NSScreenSaverWindowLevel) };
    }
}

#[cfg(not(target_os = "macos"))]
fn raise_overlay_window(_window: &tauri::WebviewWindow) {}

#[tauri::command]
pub fn cancel_capture(app: AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let mut capture = state.capture.lock().unwrap();
    let Some(session) = capture.session.take() else {
        return Ok(());
    };
    for label in &session.overlay_labels {
        if let Some(window) = app.get_webview_window(label) {
            let _ = window.close();
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
    Ok(())
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

    for label in &session.overlay_labels {
        if let Some(window) = app.get_webview_window(label) {
            let _ = window.close();
        }
    }

    let action = request.action.clone();
    if action == "copy" {
        if let Err(error) = platform::write_image_to_clipboard(&request.png) {
            emit_error(
                &app,
                format!("Could not copy the capture to the clipboard.: {error}"),
                None,
            );
        } else {
            emit_notice(&app, "Copied to Clipboard".into(), "checkmark.circle.fill".into());
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
    emit_library_changed(&app);

    match action.as_str() {
        "save" => {
            let _ = save_file_dialog(app.clone(), "kiri.png".into());
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
                &app,
                label,
                WebviewUrl::App(format!("index.html?window=pin&id={}", imported.id).into()),
            )
            .title("kiri")
            .decorations(false)
            .always_on_top(true)
            .shadow(false)
            .visible(false);
            let _ = builder.build();
        }
        "edit" => {
            let label = format!("editor-{}", imported.id);
            let builder = WebviewWindowBuilder::new(
                &app,
                label,
                WebviewUrl::App(format!("index.html?window=editor&id={}", imported.id).into()),
            )
            .title("Kiri Editor")
            .inner_size(880.0, 620.0)
            .min_inner_size(860.0, 520.0)
            .center()
            .visible(false);
            if let Ok(window) = builder.build() {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }
        _ => {}
    }

    restore_focus(&app, &session);
    Ok(())
}

#[tauri::command]
pub fn save_file_dialog(app: AppHandle, default_name: String) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let path = app
        .dialog()
        .file()
        .set_file_name(default_name)
        .add_filter("PNG image", &["png"])
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
    if request.copy_to_clipboard {
        if platform::write_image_to_clipboard(&request.png).is_ok() {
            emit_notice(&app, "Copied to Clipboard".into(), "checkmark.circle.fill".into());
        }
    }
    emit_library_changed(&app);
    Ok(())
}

// ---------------------------------------------------------------------------
// OCR (Vision / Media.Ocr are thread-safe)
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn recognize_text(png: Vec<u8>) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || crate::ocr::recognize_text(&png))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn copy_text(text: String) -> Result<(), String> {
    platform::write_text_to_clipboard(&text).map_err(|e| e.to_string())
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
pub fn start_recording_flow(app: AppHandle, request: StartRecordingRequest) -> Result<(), String> {
    let (display_id, backing_scale, screen_frame, return_pid, was_kiri_frontmost) = {
        let state = app.state::<AppState>();
        let mut capture = state.capture.lock().unwrap();
        let Some(session) = capture.session.take() else {
            return Err("No active capture session.".into());
        };
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

    let options = request.options.normalized();
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
                backing_scale,
                options,
            }),
            ..Default::default()
        };
        emit_recording_state(&app, &recording);
    }

    let label = "countdown".to_string();
    let region_screen = Rect::new(
        screen_frame.x + request.region.x,
        screen_frame.y + request.region.y,
        request.region.width,
        request.region.height,
    );
    let physical = |v: f64| (v * backing_scale).round();
    let builder = WebviewWindowBuilder::new(
        &app,
        label.clone(),
        WebviewUrl::App("index.html?window=countdown".into()),
    )
    .title("kiri")
    .inner_size(physical(region_screen.width), physical(region_screen.height))
    .position(physical(region_screen.x), physical(region_screen.y))
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .shadow(false)
    .visible(false);
    let window = builder.build().map_err(|e| e.to_string())?;
    platform::set_window_capture_excluded(&app, &label, true);
    let _ = window.show();
    let _ = window.set_focus();
    Ok(())
}

#[tauri::command]
pub fn cancel_recording_flow(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("countdown") {
        let _ = window.close();
    }
    let state = app.state::<AppState>();
    let mut recording = state.recording.lock().unwrap();
    *recording = RecordingFlow::default();
    emit_recording_state(&app, &recording);
    Ok(())
}

fn create_control_panel(app: &AppHandle) -> Result<(), String> {
    let panel = WebviewWindowBuilder::new(
        app,
        "control-panel".to_string(),
        WebviewUrl::App("index.html?window=control-panel".into()),
    )
    .title("kiri")
    .inner_size(296.0, 64.0)
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .shadow(false)
    .visible(false)
    .build()
    .map_err(|e| e.to_string())?;
    platform::set_window_capture_excluded(app, "control-panel", true);
    let _ = panel.show();
    let _ = panel.set_focus();
    Ok(())
}

fn create_ripple_window(
    app: &AppHandle,
    configuration: &RecordingConfiguration,
) -> Result<(), String> {
    let region = configuration.region;
    let scale = configuration.backing_scale;
    let ripple = WebviewWindowBuilder::new(
        app,
        "ripple".to_string(),
        WebviewUrl::App("index.html?window=ripple".into()),
    )
    .title("kiri")
    .inner_size((region.width * scale) as f64, (region.height * scale) as f64)
    .position((region.x * scale) as f64, (region.y * scale) as f64)
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .shadow(false)
    .visible(false)
    .build()
    .map_err(|e| e.to_string())?;
    platform::set_window_click_through(app, "ripple");
    let _ = ripple.show();
    Ok(())
}

/// Converts platform-native global mouse coordinates to global top-left
/// points: macOS delivers Quartz bottom-left points, Windows delivers
/// physical pixels (top-left).
fn normalize_global_point(x: f64, y: f64, _scale: f64, main_height: f64) -> (f64, f64) {
    #[cfg(target_os = "macos")]
    {
        // Quartz bottom-left points → top-left points.
        (x, main_height - y)
    }
    #[cfg(windows)]
    {
        let _ = main_height;
        (x / scale, y / scale)
    }
}

#[cfg(target_os = "macos")]
fn main_screen_height_points() -> f64 {
    use objc2_app_kit::NSScreen;
    let mtm = objc2::MainThreadMarker::new().unwrap();
    NSScreen::mainScreen(mtm)
        .map(|s| {
            let frame = s.frame();
            frame.origin.y + frame.size.height
        })
        .unwrap_or(0.0)
}

fn start_recorder(
    app: &AppHandle,
    configuration: &RecordingConfiguration,
    video_tx: mpsc::Sender<Vec<u8>>,
    audio_tx: Option<mpsc::Sender<Vec<u8>>>,
    mic_tx: Option<mpsc::Sender<Vec<u8>>>,
) -> Result<Box<dyn crate::capture::PlatformRecorder + Send>, String> {
    let ripple_excepted = platform::window_capture_id(app, "ripple")
        .into_iter()
        .collect::<Vec<_>>();
    #[cfg(target_os = "macos")]
    {
        crate::capture::macos::MacRecordingSession::start(
            configuration.display_id,
            configuration.region,
            configuration.backing_scale,
            configuration.options,
            &ripple_excepted,
            video_tx,
            audio_tx,
            mic_tx,
        )
        .map(|recorder| Box::new(recorder) as Box<dyn crate::capture::PlatformRecorder + Send>)
        .map_err(|e| e.to_string())
    }
    #[cfg(windows)]
    {
        let _ = mic_tx;
        crate::capture::windows::WindowsRecorder::start(
            configuration.display_id,
            configuration.region,
            configuration.backing_scale,
            configuration.options,
            video_tx,
            audio_tx,
        )
        .map(|recorder| Box::new(recorder) as Box<dyn crate::capture::PlatformRecorder + Send>)
        .map_err(|e| e.to_string())
    }
}

fn start_encoder(
    app: &AppHandle,
    configuration: &RecordingConfiguration,
    out_path: PathBuf,
    video_rx: mpsc::Receiver<Vec<u8>>,
    audio_rx: Option<mpsc::Receiver<Vec<u8>>>,
    mic_rx: Option<mpsc::Receiver<Vec<u8>>>,
) -> Result<crate::record::SegmentEncoder, String> {
    let state = app.state::<AppState>();
    let ffmpeg = state.ffmpeg(&app).map_err(|e| e.to_string())?;
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
        audio: configuration
            .options
            .captures_system_audio
            .then(|| crate::record::AudioSpec {
                sample_rate: 48_000,
                channels: 2,
                is_float: true,
            }),
        mic: configuration
            .options
            .captures_microphone
            .then(|| crate::record::AudioSpec {
                sample_rate: 48_000,
                channels: 2,
                is_float: true,
            }),
        video_encoder: crate::record::pick_video_encoder(&ffmpeg),
    };
    crate::record::SegmentEncoder::start(&encoder_config, out_path, &ffmpeg, video_rx, audio_rx, mic_rx)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn begin_recording(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("countdown") {
        let _ = window.close();
    }

    let configuration = {
        let state = app.state::<AppState>();
        let recording = state.recording.lock().unwrap();
        let Some(configuration) = recording.configuration.clone() else {
            return Err("No recording configuration.".into());
        };
        configuration
    };

    create_control_panel(&app)?;
    if configuration.options.highlights_clicks {
        create_ripple_window(&app, &configuration)?;
        // Install the global click monitor; platform callbacks deliver
        // platform-native coordinates that we normalize to region-local
        // points before forwarding to the ripple window.
        let region = configuration.region;
        let scale = configuration.backing_scale;
        let main_height = main_screen_height_points();
        let app_handle = app.clone();
        let click_monitor = platform::start_click_monitor(Arc::new(move |x, y| {
            let (gx, gy) = normalize_global_point(x, y, scale, main_height);
            let payload = serde_json::json!({
                "x": gx - region.x,
                "y": gy - region.y,
            });
            let _ = app_handle.emit("ripple-click", payload);
        }))
        .ok();
        {
            let state = app.state::<AppState>();
            let mut recording = state.recording.lock().unwrap();
            recording.click_monitor = click_monitor;
        }
    }

    let out_path = std::env::temp_dir().join(format!(
        "kiri-recording-{}.mp4",
        uuid::Uuid::new_v4().to_string().to_lowercase()
    ));

    let (video_tx, video_rx) = mpsc::channel::<Vec<u8>>();
    let (audio_tx, audio_rx) = configuration
        .options
        .captures_system_audio
        .then(|| mpsc::channel::<Vec<u8>>())
        .map(|(tx, rx)| (Some(tx), Some(rx)))
        .unwrap_or((None, None));
    let (mic_tx, mic_rx) = configuration
        .options
        .captures_microphone
        .then(|| mpsc::channel::<Vec<u8>>())
        .map(|(tx, rx)| (Some(tx), Some(rx)))
        .unwrap_or((None, None));

    let recorder = start_recorder(&app, &configuration, video_tx, audio_tx, mic_tx)?;
    let encoder = start_encoder(&app, &configuration, out_path, video_rx, audio_rx, mic_rx)?;

    {
        let state = app.state::<AppState>();
        let mut recording = state.recording.lock().unwrap();
        recording.is_starting = false;
        recording.is_recording = true;
        recording.is_paused = false;
        recording.started_at = Some(std::time::Instant::now());
        recording.active = Some(ActiveRecording {
            encoder: Some(encoder),
            recorder: Some(recorder),
        });
        emit_recording_state(&app, &recording);
    }

    emit_notice(&app, "Recording Started".into(), "record.circle.fill".into());
    Ok(())
}

#[tauri::command]
pub async fn pause_recording(app: AppHandle) -> Result<(), String> {
    let mut active = {
        let state = app.state::<AppState>();
        let mut recording = state.recording.lock().unwrap();
        if !recording.is_recording || recording.is_transitioning {
            return Ok(());
        }
        recording.is_recording = false;
        recording.is_transitioning = true;
        emit_recording_state(&app, &recording);
        recording.active.take().unwrap()
    };

    let mut segment_path = None;
    let mut failure = None;
    if let Some(mut recorder) = active.recorder.take() {
        if let Err(error) = recorder.stop() {
            failure = Some(error.to_string());
        }
    }
    if failure.is_none() {
        if let Some(encoder) = active.encoder.take() {
            match encoder.finish() {
                Ok(path) => segment_path = Some(path),
                Err(error) => failure = Some(error.to_string()),
            }
        }
    }
    drop(active);

    let state = app.state::<AppState>();
    let mut recording = state.recording.lock().unwrap();
    if let Some(error) = failure {
        *recording = RecordingFlow::default();
        emit_recording_state(&app, &recording);
        return Err(error);
    }
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
pub fn resume_recording(app: AppHandle) -> Result<(), String> {
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

    let out_path = std::env::temp_dir().join(format!(
        "kiri-recording-{}.mp4",
        uuid::Uuid::new_v4().to_string().to_lowercase()
    ));

    let (video_tx, video_rx) = mpsc::channel::<Vec<u8>>();
    let (audio_tx, audio_rx) = configuration
        .options
        .captures_system_audio
        .then(|| mpsc::channel::<Vec<u8>>())
        .map(|(tx, rx)| (Some(tx), Some(rx)))
        .unwrap_or((None, None));
    let (mic_tx, mic_rx) = configuration
        .options
        .captures_microphone
        .then(|| mpsc::channel::<Vec<u8>>())
        .map(|(tx, rx)| (Some(tx), Some(rx)))
        .unwrap_or((None, None));

    let recorder = start_recorder(&app, &configuration, video_tx, audio_tx, mic_tx)?;
    let encoder = start_encoder(&app, &configuration, out_path, video_rx, audio_rx, mic_rx)?;

    {
        let state = app.state::<AppState>();
        let mut recording = state.recording.lock().unwrap();
        recording.is_paused = false;
        recording.is_transitioning = false;
        recording.is_recording = true;
        recording.started_at = Some(std::time::Instant::now());
        recording.active = Some(ActiveRecording {
            encoder: Some(encoder),
            recorder: Some(recorder),
        });
        emit_recording_state(&app, &recording);
    }
    emit_notice(&app, "Recording Resumed".into(), "play.circle.fill".into());
    Ok(())
}

#[tauri::command]
pub async fn stop_recording(app: AppHandle) -> Result<(), String> {
    let (segments, active) = {
        let state = app.state::<AppState>();
        let mut recording = state.recording.lock().unwrap();
        if !(recording.is_recording || recording.is_paused) || recording.is_transitioning {
            return Ok(());
        }
        recording.is_recording = false;
        recording.is_paused = false;
        recording.is_finalizing = true;
        recording.started_at = None;
        emit_recording_state(&app, &recording);
        (recording.segments.clone(), recording.active.take())
    };

    let mut final_segments = segments;
    if let Some(mut active) = active {
        let mut failure = None;
        if let Some(mut recorder) = active.recorder.take() {
            if let Err(error) = recorder.stop() {
                failure = Some(error.to_string());
            }
        }
        if failure.is_none() {
            if let Some(encoder) = active.encoder.take() {
                match encoder.finish() {
                    Ok(path) => final_segments.push(path),
                    Err(error) => failure = Some(error.to_string()),
                }
            }
        }
        if let Some(error) = failure {
            emit_error(&app, error, None);
        }
    }

    if let Some(window) = app.get_webview_window("control-panel") {
        let _ = window.close();
    }
    if let Some(window) = app.get_webview_window("ripple") {
        let _ = window.close();
    }

    let handle = app.clone();
    std::thread::spawn(move || {
        let result = finalize_recording(&handle, final_segments);
        {
            let state = handle.state::<AppState>();
            let mut recording = state.recording.lock().unwrap();
            *recording = RecordingFlow::default();
            emit_recording_state(&handle, &recording);
        }
        match result {
            Ok(()) => emit_notice(&handle, "Recording Saved".into(), "video.fill".into()),
            Err(error) => emit_error(&handle, error, None),
        }
        // Restore focus to the source application after the recording ends.
        let (return_pid, was_kiri_frontmost) = {
            let state = handle.state::<AppState>();
            let recording = state.recording.lock().unwrap();
            (recording.return_pid, recording.was_kiri_frontmost)
        };
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
    crate::record::merge_segments(&segments, &merged_path, &ffmpeg).map_err(|e| e.to_string())?;
    let mut library = state.library.lock().unwrap();
    let imported = library
        .import_file(
            &merged_path,
            CaptureKind::Video,
            "mp4",
            0,
            0,
            Some(0.0),
            None,
        )
        .map_err(|e| e.to_string())?;
    drop(library);
    let _ = imported;
    for segment in &segments {
        let _ = std::fs::remove_file(segment);
    }
    let _ = std::fs::remove_file(&merged_path);
    emit_library_changed(app);
    Ok(())
}

// ---------------------------------------------------------------------------
// Settings / locale / shortcuts
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn mic_supported() -> bool {
    platform::mic_supported()
}

#[tauri::command]
pub fn get_locale() -> String {
    let locale = sys_locale::get_locale().unwrap_or_else(|| "en".into());
    let lower = locale.to_lowercase();
    if lower.starts_with("zh")
        && (lower.contains("hans") || lower.contains("-cn") || lower.contains("_cn"))
    {
        "zh-Hans".into()
    } else {
        "en".into()
    }
}

#[tauri::command]
pub fn get_shortcut_label() -> String {
    if cfg!(target_os = "macos") {
        KIRI_CAPTURE.display_label()
    } else {
        "Shift+Ctrl+A".into()
    }
}

#[tauri::command]
pub fn open_settings(action: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let url = match action.as_str() {
            "openSettings" => "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture",
            "openAccessibilitySettings" => "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
            "openInputMonitoringSettings" => "x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent",
            "openMicrophoneSettings" => "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone",
            _ => return Err("unknown action".into()),
        };
        let workspace = objc2_app_kit::NSWorkspace::sharedWorkspace();
        let url = objc2_foundation::NSURL::URLWithString(&objc2_foundation::NSString::from_str(url))
            .ok_or_else(|| "bad url".to_string())?;
        unsafe { workspace.openURL(&url) };
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
