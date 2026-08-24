//! `kiri://` custom protocol serving in-memory capture images and library
//! asset files to the webview (never exposing arbitrary disk paths).

use std::collections::{HashMap, VecDeque};
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::{Arc, Mutex};

use tauri::Manager;

use tauri::http::{Method, Request, Response, StatusCode};

use crate::core::asset::CaptureKind;
use crate::state::AppState;

const THUMBNAIL_CACHE_MAX_BYTES: usize = 32 * 1024 * 1024;
const THUMBNAIL_CACHE_MAX_ENTRIES: usize = 256;
const MEDIA_RANGE_MAX_BYTES: u64 = 1024 * 1024;

pub struct ProtocolStore {
    frozen_capture: Mutex<Option<FrozenCapture>>,
    pub pin_images: Mutex<HashMap<String, Vec<u8>>>,
    thumbnails: Mutex<ThumbnailCache>,
    thumbnail_generation: Mutex<()>,
}

impl ProtocolStore {
    pub fn new() -> Self {
        Self {
            frozen_capture: Mutex::new(None),
            pin_images: Mutex::new(HashMap::new()),
            thumbnails: Mutex::new(ThumbnailCache::with_limits(
                THUMBNAIL_CACHE_MAX_BYTES,
                THUMBNAIL_CACHE_MAX_ENTRIES,
            )),
            thumbnail_generation: Mutex::new(()),
        }
    }
}

struct ThumbnailCache {
    entries: HashMap<String, Vec<u8>>,
    least_to_most_recent: VecDeque<String>,
    total_bytes: usize,
    max_bytes: usize,
    max_entries: usize,
}

impl ThumbnailCache {
    fn with_limits(max_bytes: usize, max_entries: usize) -> Self {
        Self {
            entries: HashMap::new(),
            least_to_most_recent: VecDeque::new(),
            total_bytes: 0,
            max_bytes,
            max_entries,
        }
    }

    fn get(&mut self, key: &str) -> Option<Vec<u8>> {
        let bytes = self.entries.get(key)?.clone();
        self.touch(key);
        Some(bytes)
    }

    fn insert(&mut self, key: String, bytes: Vec<u8>) {
        self.remove(&key);
        if self.max_bytes == 0 || self.max_entries == 0 || bytes.len() > self.max_bytes {
            return;
        }

        self.total_bytes += bytes.len();
        self.least_to_most_recent.push_back(key.clone());
        self.entries.insert(key, bytes);
        self.evict_to_limits();
    }

    fn remove(&mut self, key: &str) -> Option<Vec<u8>> {
        let bytes = self.entries.remove(key)?;
        self.total_bytes = self.total_bytes.saturating_sub(bytes.len());
        if let Some(index) = self
            .least_to_most_recent
            .iter()
            .position(|candidate| candidate == key)
        {
            self.least_to_most_recent.remove(index);
        }
        Some(bytes)
    }

    fn touch(&mut self, key: &str) {
        if let Some(index) = self
            .least_to_most_recent
            .iter()
            .position(|candidate| candidate == key)
        {
            self.least_to_most_recent.remove(index);
            self.least_to_most_recent.push_back(key.to_string());
        }
    }

    fn evict_to_limits(&mut self) {
        while self.total_bytes > self.max_bytes || self.entries.len() > self.max_entries {
            let Some(key) = self.least_to_most_recent.pop_front() else {
                self.entries.clear();
                self.total_bytes = 0;
                break;
            };
            if let Some(bytes) = self.entries.remove(&key) {
                self.total_bytes = self.total_bytes.saturating_sub(bytes.len());
            }
        }
    }
}

struct FrozenCapture {
    capture_id: uuid::Uuid,
    token: String,
    png: Arc<[u8]>,
}

pub fn set_frozen_png(store: &ProtocolStore, capture_id: uuid::Uuid, png: Arc<[u8]>) -> String {
    let token = uuid::Uuid::new_v4().simple().to_string();
    *store.frozen_capture.lock().unwrap() = Some(FrozenCapture {
        capture_id,
        token: token.clone(),
        png,
    });
    token
}

/// Releases the encoded image owned by a closed `pin-<uuid>` window.
/// Returns true only when a valid pin label had a cached image to remove.
pub fn clear_pin_image_for_window_label(store: &ProtocolStore, label: &str) -> bool {
    let Some(id) = label.strip_prefix("pin-") else {
        return false;
    };
    let Ok(id) = uuid::Uuid::parse_str(id) else {
        return false;
    };
    store
        .pin_images
        .lock()
        .unwrap()
        .remove(&id.to_string())
        .is_some()
}

pub fn with_thumbnail_invalidation<T, E>(
    store: &ProtocolStore,
    id: uuid::Uuid,
    operation: impl FnOnce() -> Result<T, E>,
) -> Result<T, E> {
    with_thumbnail_invalidations(store, std::slice::from_ref(&id), operation)
}

pub fn with_thumbnail_invalidations<T, E>(
    store: &ProtocolStore,
    ids: &[uuid::Uuid],
    operation: impl FnOnce() -> Result<T, E>,
) -> Result<T, E> {
    // Use the same barrier as thumbnail generation. If an old preview is in
    // flight, wait for its insertion before replacing/deleting assets and
    // removing stale entries; if the mutation wins first, later generation
    // observes the new library state.
    let _generation = store.thumbnail_generation.lock().unwrap();
    let result = operation();
    let mut thumbnails = store.thumbnails.lock().unwrap();
    for id in ids {
        thumbnails.remove(&id.to_string());
    }
    result
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

    // Downsampled previews for the library grid. Generation is serialized so
    // several newly visible 4K captures cannot all allocate decoder surfaces
    // at once; encoded results live in the bounded LRU above.
    if host == "thumbnail" {
        if let Ok(id) = uuid::Uuid::parse_str(&path) {
            let (kind, file_path) = {
                let library = state.library.lock().unwrap();
                match library.asset_by_id(&id).cloned() {
                    Some(asset) => (
                        asset.kind,
                        state.library_root.join("Assets").join(&asset.filename),
                    ),
                    None => return not_found(),
                }
            };
            let cache_key = id.to_string();
            if let Some(bytes) = store.thumbnails.lock().unwrap().get(&cache_key) {
                return respond(bytes, "image/png");
            }

            let _generation = store.thumbnail_generation.lock().unwrap();
            // Another request may have populated the same key while this one
            // was waiting for the single thumbnail worker.
            if let Some(bytes) = store.thumbnails.lock().unwrap().get(&cache_key) {
                return respond(bytes, "image/png");
            }
            let thumbnail = if kind == CaptureKind::Image {
                crate::thumbnail::image_thumbnail(&file_path)
            } else {
                let ffmpeg = state.ffmpeg_path.get().cloned().or_else(|| {
                    let path = crate::record::existing_ffmpeg()?;
                    let _ = state.ffmpeg_path.set(path.clone());
                    Some(path)
                });
                ffmpeg.and_then(|ffmpeg| crate::thumbnail::video_first_frame(&ffmpeg, &file_path))
            };
            if let Some(thumbnail) = thumbnail {
                store
                    .thumbnails
                    .lock()
                    .unwrap()
                    .insert(cache_key, thumbnail.clone());
                return respond(thumbnail, "image/png");
            }
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
            if let Some(bytes) = store.thumbnails.lock().unwrap().get(&cache_key) {
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
                        .thumbnails
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
            let content_type = if file_path.extension().map(|e| e == "png").unwrap_or(false) {
                "image/png"
            } else if file_path.extension().map(|e| e == "gif").unwrap_or(false) {
                "image/gif"
            } else {
                "video/mp4"
            };
            return respond_media(request, &file_path, content_type).unwrap_or_else(not_found);
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
        .then(|| capture.png.as_ref().to_vec())
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MediaByteRange {
    start: u64,
    end: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MediaRangeDecision {
    Full,
    Partial(MediaByteRange),
    Unsatisfiable,
}

fn parse_media_range(header: Option<&str>, total: u64) -> MediaRangeDecision {
    let Some(header) = header else {
        return MediaRangeDecision::Full;
    };
    let Some(value) = header.strip_prefix("bytes=") else {
        // Unknown range units are ignored, as required by HTTP range
        // semantics, so the caller serves the complete representation.
        return MediaRangeDecision::Full;
    };
    if value.contains(',') {
        // This protocol intentionally implements only one range. Ignore a
        // multi-range request instead of incorrectly claiming that no range
        // is satisfiable.
        return MediaRangeDecision::Full;
    }
    let Some((start, end)) = value.split_once('-') else {
        return MediaRangeDecision::Full;
    };
    if end.contains('-') {
        return MediaRangeDecision::Full;
    }

    let range = if start.is_empty() {
        let Ok(suffix_length) = end.parse::<u64>() else {
            return MediaRangeDecision::Full;
        };
        if suffix_length == 0 {
            return MediaRangeDecision::Unsatisfiable;
        }
        if total == 0 {
            return MediaRangeDecision::Unsatisfiable;
        }
        let bounded_length = suffix_length.min(total).min(MEDIA_RANGE_MAX_BYTES);
        MediaByteRange {
            // A suffix request always selects the end of the representation.
            // When bounding memory, move the start forward rather than
            // truncating the end and accidentally omitting the requested tail.
            start: total - bounded_length,
            end: total - 1,
        }
    } else {
        let Ok(start) = start.parse::<u64>() else {
            return MediaRangeDecision::Full;
        };
        if start >= total {
            return MediaRangeDecision::Unsatisfiable;
        }
        let end = if end.is_empty() {
            total - 1
        } else {
            let Ok(end) = end.parse::<u64>() else {
                return MediaRangeDecision::Full;
            };
            if end < start {
                return MediaRangeDecision::Full;
            }
            end.min(total - 1)
        };
        MediaByteRange { start, end }
    };

    MediaRangeDecision::Partial(MediaByteRange {
        start: range.start,
        end: range
            .end
            .min(range.start.saturating_add(MEDIA_RANGE_MAX_BYTES - 1)),
    })
}

/// Serves a media file, honoring one bounded `Range: bytes=start-end` request.
/// Ranged playback reads at most 1 MiB from disk instead of materializing the
/// complete recording before slicing it.
fn respond_media(
    request: &Request<Vec<u8>>,
    path: &Path,
    content_type: &str,
) -> Option<Response<Vec<u8>>> {
    let mut file = std::fs::File::open(path).ok()?;
    let total = file.metadata().ok()?.len();
    if request.method() == Method::HEAD {
        return Some(
            Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", content_type)
                .header("Content-Length", total.to_string())
                .header("Accept-Ranges", "bytes")
                .header("Access-Control-Allow-Origin", "*")
                .header("Cache-Control", "no-store")
                .body(Vec::new())
                .unwrap(),
        );
    }
    let range = parse_media_range(
        request
            .headers()
            .get("range")
            .and_then(|value| value.to_str().ok()),
        total,
    );

    // Tauri custom-protocol responses own an in-memory byte body. Never let a
    // missing or malformed Range turn a multi-gigabyte recording into one
    // allocation. WebView video players issue byte ranges; a client that does
    // not must retry with a valid single range instead of receiving the full
    // MP4 representation.
    if content_type == "video/mp4" && matches!(range, MediaRangeDecision::Full) {
        return Some(range_not_satisfiable(total));
    }

    match range {
        MediaRangeDecision::Unsatisfiable => Some(range_not_satisfiable(total)),
        MediaRangeDecision::Partial(range) => {
            let length = range.end.checked_sub(range.start)?.checked_add(1)?;
            let capacity = usize::try_from(length).ok()?;
            let mut bytes = Vec::with_capacity(capacity);
            file.seek(SeekFrom::Start(range.start)).ok()?;
            file.take(length).read_to_end(&mut bytes).ok()?;
            if bytes.len() != capacity {
                return None;
            }
            Some(
                Response::builder()
                    .status(StatusCode::PARTIAL_CONTENT)
                    .header("Content-Type", content_type)
                    .header("Content-Length", bytes.len().to_string())
                    .header(
                        "Content-Range",
                        format!("bytes {}-{}/{total}", range.start, range.end),
                    )
                    .header("Accept-Ranges", "bytes")
                    .header("Access-Control-Allow-Origin", "*")
                    .header("Cache-Control", "no-store")
                    .body(bytes)
                    .unwrap(),
            )
        }
        MediaRangeDecision::Full => {
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes).ok()?;
            Some(
                Response::builder()
                    .status(StatusCode::OK)
                    .header("Content-Type", content_type)
                    .header("Content-Length", bytes.len().to_string())
                    .header("Accept-Ranges", "bytes")
                    .header("Access-Control-Allow-Origin", "*")
                    .header("Cache-Control", "no-store")
                    .body(bytes)
                    .unwrap(),
            )
        }
    }
}

fn range_not_satisfiable(total: u64) -> Response<Vec<u8>> {
    Response::builder()
        .status(StatusCode::RANGE_NOT_SATISFIABLE)
        .header("Content-Range", format!("bytes */{total}"))
        .header("Accept-Ranges", "bytes")
        .header("Access-Control-Allow-Origin", "*")
        .header("Cache-Control", "no-store")
        .body(Vec::new())
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
        let token = set_frozen_png(&store, capture_id, vec![1, 2, 3].into());
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

    #[test]
    fn parses_bounded_and_open_media_ranges() {
        assert_eq!(parse_media_range(None, 2_000_000), MediaRangeDecision::Full);
        assert_eq!(
            parse_media_range(Some("bytes=100-199"), 2_000_000),
            MediaRangeDecision::Partial(MediaByteRange {
                start: 100,
                end: 199,
            })
        );
        assert_eq!(
            parse_media_range(Some("bytes=100-"), 2_000_000),
            MediaRangeDecision::Partial(MediaByteRange {
                start: 100,
                end: 100 + MEDIA_RANGE_MAX_BYTES - 1,
            })
        );
        assert_eq!(
            parse_media_range(Some("bytes=1-1999999"), 2_000_000),
            MediaRangeDecision::Partial(MediaByteRange {
                start: 1,
                end: MEDIA_RANGE_MAX_BYTES,
            })
        );
    }

    #[test]
    fn parses_suffix_media_ranges() {
        assert_eq!(
            parse_media_range(Some("bytes=-500"), 2_000),
            MediaRangeDecision::Partial(MediaByteRange {
                start: 1_500,
                end: 1_999,
            })
        );
        assert_eq!(
            parse_media_range(Some("bytes=-3000"), 2_000),
            MediaRangeDecision::Partial(MediaByteRange {
                start: 0,
                end: 1_999,
            })
        );
        let total = MEDIA_RANGE_MAX_BYTES * 2 + 123;
        assert_eq!(
            parse_media_range(
                Some(&format!("bytes=-{}", MEDIA_RANGE_MAX_BYTES + 500)),
                total,
            ),
            MediaRangeDecision::Partial(MediaByteRange {
                start: total - MEDIA_RANGE_MAX_BYTES,
                end: total - 1,
            })
        );
    }

    #[test]
    fn ignores_invalid_and_multi_media_ranges() {
        for header in [
            "items=0-1",
            "bytes=100-99",
            "bytes=-",
            "bytes=0-1,4-5",
            "bytes=invalid-1",
        ] {
            assert_eq!(
                parse_media_range(Some(header), 2_000),
                MediaRangeDecision::Full,
                "header {header} should be ignored"
            );
        }
    }

    #[test]
    fn rejects_valid_but_unsatisfiable_media_ranges() {
        assert_eq!(
            parse_media_range(Some("bytes=2000-"), 2_000),
            MediaRangeDecision::Unsatisfiable
        );
        assert_eq!(
            parse_media_range(Some("bytes=-0"), 2_000),
            MediaRangeDecision::Unsatisfiable
        );
        assert_eq!(
            parse_media_range(Some("bytes=0-"), 0),
            MediaRangeDecision::Unsatisfiable
        );
    }

    #[test]
    fn media_response_reads_only_the_bounded_requested_range() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let data = vec![7; MEDIA_RANGE_MAX_BYTES as usize + 128];
        std::fs::write(file.path(), &data).unwrap();
        let request = Request::builder()
            .uri("kiri://media/test")
            .header("Range", "bytes=0-")
            .body(Vec::new())
            .unwrap();

        let response = respond_media(&request, file.path(), "video/mp4").unwrap();

        assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(response.body().len(), MEDIA_RANGE_MAX_BYTES as usize);
        assert_eq!(
            response.headers().get("Content-Range").unwrap(),
            &format!("bytes 0-{}/{}", MEDIA_RANGE_MAX_BYTES - 1, data.len())
        );
        assert_eq!(response.body(), &data[..MEDIA_RANGE_MAX_BYTES as usize]);
    }

    #[test]
    fn media_response_bounds_large_suffix_to_the_file_tail() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let data: Vec<u8> = (0..MEDIA_RANGE_MAX_BYTES as usize + 257)
            .map(|index| (index % 251) as u8)
            .collect();
        std::fs::write(file.path(), &data).unwrap();
        let request = Request::builder()
            .uri("kiri://media/test")
            .header("Range", format!("bytes=-{}", MEDIA_RANGE_MAX_BYTES + 128))
            .body(Vec::new())
            .unwrap();

        let response = respond_media(&request, file.path(), "video/mp4").unwrap();
        let expected_start = data.len() - MEDIA_RANGE_MAX_BYTES as usize;

        assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(response.body(), &data[expected_start..]);
        assert_eq!(
            response.headers().get("Content-Range").unwrap(),
            &format!("bytes {}-{}/{}", expected_start, data.len() - 1, data.len())
        );
    }

    #[test]
    fn video_response_rejects_unbounded_and_invalid_range_requests() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let data = b"complete representation";
        std::fs::write(file.path(), data).unwrap();

        for range in [
            None,
            Some("items=0-1"),
            Some("bytes=0-1,4-5"),
            Some("bytes=5-4"),
        ] {
            let mut request = Request::builder().uri("kiri://media/test");
            if let Some(range) = range {
                request = request.header("Range", range);
            }
            let request = request.body(Vec::new()).unwrap();
            let response = respond_media(&request, file.path(), "video/mp4").unwrap();

            assert_eq!(
                response.status(),
                StatusCode::RANGE_NOT_SATISFIABLE,
                "range {range:?}"
            );
            assert!(response.body().is_empty(), "range {range:?}");
        }
    }

    #[test]
    fn media_response_keeps_full_image_and_gif_compatibility() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let data = b"GIF89a-small-fixture";
        std::fs::write(file.path(), data).unwrap();
        let request = Request::builder()
            .uri("kiri://media/test")
            .body(Vec::new())
            .unwrap();

        let response = respond_media(&request, file.path(), "image/gif").unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers().get("Content-Type").unwrap(), "image/gif");
        assert_eq!(response.body(), data);
    }

    #[test]
    fn media_head_response_returns_metadata_without_a_body() {
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), b"1234").unwrap();
        let request = Request::builder()
            .method(Method::HEAD)
            .uri("kiri://media/test")
            .body(Vec::new())
            .unwrap();

        let response = respond_media(&request, file.path(), "video/mp4").unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers().get("Content-Length").unwrap(), "4");
        assert_eq!(response.headers().get("Accept-Ranges").unwrap(), "bytes");
        assert_eq!(response.headers().get("Content-Type").unwrap(), "video/mp4");
        assert!(response.body().is_empty());
    }

    #[test]
    fn media_response_reports_unsatisfiable_ranges() {
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), b"1234").unwrap();
        let request = Request::builder()
            .uri("kiri://media/test")
            .header("Range", "bytes=4-")
            .body(Vec::new())
            .unwrap();

        let response = respond_media(&request, file.path(), "video/mp4").unwrap();

        assert_eq!(response.status(), StatusCode::RANGE_NOT_SATISFIABLE);
        assert_eq!(
            response.headers().get("Content-Range").unwrap(),
            "bytes */4"
        );
        assert_eq!(response.headers().get("Accept-Ranges").unwrap(), "bytes");
        assert!(response.body().is_empty());
    }

    #[test]
    fn thumbnail_cache_evicts_least_recent_entries_within_budgets() {
        let mut cache = ThumbnailCache::with_limits(6, 3);
        cache.insert("a".into(), vec![1; 2]);
        cache.insert("b".into(), vec![2; 2]);
        cache.insert("c".into(), vec![3; 2]);
        assert_eq!(cache.get("a"), Some(vec![1; 2]));

        cache.insert("d".into(), vec![4; 2]);

        assert!(!cache.entries.contains_key("b"));
        assert!(cache.entries.contains_key("a"));
        assert!(cache.entries.contains_key("c"));
        assert!(cache.entries.contains_key("d"));
        assert_eq!(cache.total_bytes, 6);

        cache.insert("oversized".into(), vec![0; 7]);
        assert!(!cache.entries.contains_key("oversized"));
        assert_eq!(cache.total_bytes, 6);
    }

    #[test]
    fn thumbnail_cache_enforces_entry_limit_and_refreshes_hits() {
        let mut cache = ThumbnailCache::with_limits(100, 2);
        cache.insert("a".into(), vec![1]);
        cache.insert("b".into(), vec![2]);
        assert_eq!(cache.get("a"), Some(vec![1]));
        cache.insert("c".into(), vec![3]);

        assert!(cache.entries.contains_key("a"));
        assert!(!cache.entries.contains_key("b"));
        assert!(cache.entries.contains_key("c"));
    }

    #[test]
    fn deleting_assets_evicts_only_their_cached_thumbnails() {
        let store = ProtocolStore::new();
        let first = uuid::Uuid::new_v4();
        let second = uuid::Uuid::new_v4();
        let retained = uuid::Uuid::new_v4();
        {
            let mut cache = store.thumbnails.lock().unwrap();
            cache.insert(first.to_string(), vec![1]);
            cache.insert(second.to_string(), vec![2]);
            cache.insert(retained.to_string(), vec![3]);
        }

        with_thumbnail_invalidations(&store, &[first, second], || Ok::<_, ()>(())).unwrap();

        let cache = store.thumbnails.lock().unwrap();
        assert!(!cache.entries.contains_key(&first.to_string()));
        assert!(!cache.entries.contains_key(&second.to_string()));
        assert!(cache.entries.contains_key(&retained.to_string()));
    }

    #[test]
    fn clearing_a_destroyed_pin_window_releases_only_its_image() {
        let store = ProtocolStore::new();
        let id = uuid::Uuid::new_v4();
        store
            .pin_images
            .lock()
            .unwrap()
            .insert(id.to_string(), vec![1, 2, 3]);

        assert!(!clear_pin_image_for_window_label(
            &store,
            "viewer-not-a-pin"
        ));
        assert!(!clear_pin_image_for_window_label(&store, "pin-not-a-uuid"));
        assert!(clear_pin_image_for_window_label(
            &store,
            &format!("pin-{id}")
        ));
        assert!(!clear_pin_image_for_window_label(
            &store,
            &format!("pin-{id}")
        ));
    }

    #[test]
    fn clearing_an_edited_asset_invalidates_its_thumbnail() {
        let store = ProtocolStore::new();
        let id = uuid::Uuid::new_v4();
        store
            .thumbnails
            .lock()
            .unwrap()
            .insert(id.to_string(), vec![1, 2, 3]);

        with_thumbnail_invalidation(&store, id, || Ok::<_, ()>(())).unwrap();

        assert!(store
            .thumbnails
            .lock()
            .unwrap()
            .get(&id.to_string())
            .is_none());
    }
}
