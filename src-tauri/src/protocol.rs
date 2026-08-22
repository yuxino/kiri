//! `kiri://` custom protocol serving in-memory capture images and library
//! asset files to the webview (never exposing arbitrary disk paths).

use std::collections::HashMap;
use std::sync::Mutex;

use tauri::Manager;

use tauri::http::{Request, Response, StatusCode};

use crate::core::asset::CaptureKind;
use crate::state::AppState;

pub struct ProtocolStore {
    frozen_capture: Mutex<Option<FrozenCapture>>,
    pub pin_images: Mutex<HashMap<String, Vec<u8>>>,
    pub video_thumbnails: Mutex<HashMap<String, Vec<u8>>>,
}

impl ProtocolStore {
    pub fn new() -> Self {
        Self {
            frozen_capture: Mutex::new(None),
            pin_images: Mutex::new(HashMap::new()),
            video_thumbnails: Mutex::new(HashMap::new()),
        }
    }
}

struct FrozenCapture {
    capture_id: uuid::Uuid,
    token: String,
    png: Vec<u8>,
}

pub fn set_frozen_png(store: &ProtocolStore, capture_id: uuid::Uuid, png: Vec<u8>) -> String {
    let token = uuid::Uuid::new_v4().simple().to_string();
    *store.frozen_capture.lock().unwrap() = Some(FrozenCapture {
        capture_id,
        token: token.clone(),
        png,
    });
    token
}

pub fn clear_frozen_png_for_capture(store: &ProtocolStore, capture_id: uuid::Uuid) {
    let mut frozen_capture = store.frozen_capture.lock().unwrap();
    if frozen_capture
        .as_ref()
        .is_some_and(|capture| capture.capture_id == capture_id)
    {
        *frozen_capture = None;
    }
}

pub fn handle(app: &tauri::AppHandle, request: &Request<Vec<u8>>) -> Response<Vec<u8>> {
    let uri = request.uri();
    let host = uri.host().unwrap_or("").to_string();
    let path = uri.path().trim_start_matches('/').to_string();
    let state = app.state::<AppState>();
    let store = app.state::<ProtocolStore>();

    // Frozen capture image: the unguessable per-capture token is injected
    // only into the overlay URL, never returned by the public IPC context.
    if host == "capture" {
        if let Some(bytes) = frozen_png_for_path(&store, &path) {
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
            if let Some(bytes) = store
                .video_thumbnails
                .lock()
                .unwrap()
                .get(&cache_key)
                .cloned()
            {
                return respond(bytes, "image/png");
            }
            // Browsing the local library must never trigger a network
            // download. Reuse an initialized encoder, or discover only a
            // validated environment/cache/PATH installation.
            let ffmpeg = state.ffmpeg_path.get().cloned().or_else(|| {
                let path = crate::record::existing_ffmpeg()?;
                let _ = state.ffmpeg_path.set(path.clone());
                Some(path)
            });
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

    // Full media files for the in-app viewer: kiri://media/<id>
    // Serves the raw asset bytes with a media Content-Type so <img>/<video>
    // can play it. HTTP Range is honored so video seeking works.
    if host == "media" {
        let rest = &path.trim_end_matches('/');
        if let Ok(id) = uuid::Uuid::parse_str(rest) {
            let file_path = {
                let library = state.library.lock().unwrap();
                match library.asset_by_id(&id).cloned() {
                    Some(asset) => state.library_root.join("Assets").join(&asset.filename),
                    None => return not_found(),
                }
            };
            let bytes = match std::fs::read(&file_path) {
                Ok(b) => b,
                Err(_) => return not_found(),
            };
            let content_type = if file_path.extension().map(|e| e == "png").unwrap_or(false) {
                "image/png"
            } else if file_path.extension().map(|e| e == "gif").unwrap_or(false) {
                "image/gif"
            } else {
                "video/mp4"
            };
            return respond_media(request, bytes, content_type);
        }
        return not_found();
    }

    not_found()
}

fn frozen_png_for_path(store: &ProtocolStore, path: &str) -> Option<Vec<u8>> {
    let requested_token = path.strip_prefix("frozen/")?.strip_suffix(".png")?;
    if requested_token.len() != 32 || !requested_token.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return None;
    }
    let capture = store.frozen_capture.lock().unwrap();
    let capture = capture.as_ref()?;
    constant_time_eq(requested_token.as_bytes(), capture.token.as_bytes())
        .then(|| capture.png.clone())
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0_u8, |difference, (left, right)| {
            difference | (left ^ right)
        })
        == 0
}

/// Serves media bytes, honoring a single `Range: bytes=start-end` header
/// (enough for <video> seeking). Returns 206 Partial Content when ranged.
fn respond_media(
    request: &Request<Vec<u8>>,
    bytes: Vec<u8>,
    content_type: &str,
) -> Response<Vec<u8>> {
    let total = bytes.len() as u64;
    let range = request
        .headers()
        .get("range")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("bytes="))
        .and_then(|v| {
            let mut parts = v.split('-');
            let start = parts.next()?.parse::<u64>().ok();
            let end = parts.next().and_then(|e| e.parse::<u64>().ok());
            match (start, end) {
                (Some(s), e) => Some((s, e.unwrap_or(total.saturating_sub(1)))),
                (None, Some(e)) => Some((total.saturating_sub(e), total.saturating_sub(1))),
                _ => None,
            }
        });

    if let Some((start, end)) = range {
        if start >= total {
            return Response::builder()
                .status(StatusCode::RANGE_NOT_SATISFIABLE)
                .header("Content-Range", format!("bytes */{total}"))
                .body(Vec::new())
                .unwrap();
        }
        let end = end.min(total - 1);
        let slice = bytes[start as usize..=end as usize].to_vec();
        return Response::builder()
            .status(StatusCode::PARTIAL_CONTENT)
            .header("Content-Type", content_type)
            .header("Content-Length", slice.len().to_string())
            .header("Content-Range", format!("bytes {start}-{end}/{total}"))
            .header("Accept-Ranges", "bytes")
            .header("Access-Control-Allow-Origin", "*")
            .body(slice)
            .unwrap();
    }

    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", content_type)
        .header("Content-Length", total.to_string())
        .header("Accept-Ranges", "bytes")
        .header("Access-Control-Allow-Origin", "*")
        .header("Cache-Control", "no-store")
        .body(bytes)
        .unwrap()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frozen_capture_requires_exact_per_capture_token_and_clear_revokes_it() {
        let store = ProtocolStore::new();
        let capture_id = uuid::Uuid::new_v4();
        let token = set_frozen_png(&store, capture_id, vec![1, 2, 3]);
        assert_eq!(
            frozen_png_for_path(&store, &format!("frozen/{token}.png")),
            Some(vec![1, 2, 3])
        );
        assert!(frozen_png_for_path(&store, "frozen.png").is_none());
        assert!(
            frozen_png_for_path(&store, "frozen/00000000000000000000000000000000.png").is_none()
        );
        assert!(frozen_png_for_path(&store, &format!("other/{token}.png")).is_none());
        clear_frozen_png_for_capture(&store, uuid::Uuid::new_v4());
        assert_eq!(
            frozen_png_for_path(&store, &format!("frozen/{token}.png")),
            Some(vec![1, 2, 3])
        );
        clear_frozen_png_for_capture(&store, capture_id);
        assert!(frozen_png_for_path(&store, &format!("frozen/{token}.png")).is_none());
    }
}
