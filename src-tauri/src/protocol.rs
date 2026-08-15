//! `kiri://` custom protocol serving in-memory capture images and library
//! asset files to the webview (never exposing arbitrary disk paths).

use std::collections::HashMap;
use std::sync::Mutex;

use tauri::Manager;

use tauri::http::{Request, Response, StatusCode};

use crate::core::asset::CaptureKind;
use crate::state::AppState;

pub struct ProtocolStore {
    pub frozen_png: Mutex<Option<Vec<u8>>>,
    pub pin_images: Mutex<HashMap<String, Vec<u8>>>,
    pub video_thumbnails: Mutex<HashMap<String, Vec<u8>>>,
}

impl ProtocolStore {
    pub fn new() -> Self {
        Self {
            frozen_png: Mutex::new(None),
            pin_images: Mutex::new(HashMap::new()),
            video_thumbnails: Mutex::new(HashMap::new()),
        }
    }
}

pub fn set_frozen_png(store: &ProtocolStore, png: Vec<u8>) {
    *store.frozen_png.lock().unwrap() = Some(png);
}

pub fn handle(app: &tauri::AppHandle, request: &Request<Vec<u8>>) -> Response<Vec<u8>> {
    let uri = request.uri();
    let host = uri.host().unwrap_or("").to_string();
    let path = uri.path().trim_start_matches('/').to_string();
    let state = app.state::<AppState>();
    let store = app.state::<ProtocolStore>();

    // Frozen capture image: kiri://capture/frozen.png
    if host == "capture" && path == "frozen.png" {
        if let Some(bytes) = store.frozen_png.lock().unwrap().clone() {
            return respond_png(bytes);
        }
        return not_found();
    }

    // Pinned images: kiri://pin/<id>.png
    if host == "pin" {
        let id = path.trim_end_matches(".png");
        if let Some(bytes) = store.pin_images.lock().unwrap().get(id).cloned() {
            return respond_png(bytes);
        }
        return not_found();
    }

    // Library assets by id: kiri://asset/<id>
    if host == "asset" {
        let rest = &path;
        if let Ok(id) = uuid::Uuid::parse_str(rest) {
            // Resolve the file path while holding the lock, then drop it
            // before running ffmpeg (thumbnail generation can take ~100ms).
            let (kind, file_path) = {
                let library = state.library.lock().unwrap();
                match library.asset_by_id(&id).cloned() {
                    Some(asset) => {
                        let path = state.library_root.join("Assets").join(&asset.filename);
                        (asset.kind, path)
                    }
                    None => return not_found(),
                }
            };
            if kind == CaptureKind::Image {
                if let Ok(bytes) = std::fs::read(&file_path) {
                    return respond(bytes, "image/png");
                }
            }
            // Video / GIF: serve a first-frame thumbnail (cached in memory).
            let cache_key = id.to_string();
            if let Some(bytes) = store.video_thumbnails.lock().unwrap().get(&cache_key).cloned() {
                return respond(bytes, "image/png");
            }
            let ffmpeg = state
                .ffmpeg_path
                .get()
                .cloned()
                .or_else(|| crate::record::ensure_ffmpeg(None).ok());
            if let Some(ffmpeg) = ffmpeg {
                if let Some(thumbnail) = crate::thumbnail::video_first_frame(&ffmpeg, &file_path) {
                    store
                        .video_thumbnails
                        .lock()
                        .unwrap()
                        .insert(cache_key, thumbnail.clone());
                    return respond(thumbnail, "image/png");
                }
            }
        }
        return not_found();
    }

    not_found()
}

fn respond(bytes: Vec<u8>, content_type: &str) -> Response<Vec<u8>> {
    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", content_type)
        // Required for fetch() from the tauri://localhost page to the
        // kiri:// custom scheme (canvas-safe blob loading).
        .header("Access-Control-Allow-Origin", "*")
        .header("Cache-Control", "no-store")
        .body(bytes)
        .unwrap()
}

fn respond_png(bytes: Vec<u8>) -> Response<Vec<u8>> {
    respond(bytes, "image/png")
}

fn not_found() -> Response<Vec<u8>> {
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Vec::new())
        .unwrap()
}
