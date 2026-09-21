//! Bounded, versioned, non-destructive video editing drafts. Documents never
//! carry filesystem paths; a library asset and its current copy own each draft.

use std::collections::HashSet;
use std::io::{Read, Write};
use std::path::Path;
use std::time::UNIX_EPOCH;

use base64::Engine;
use image::ImageDecoder;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use super::annotation::{AnnotationDocument, AnnotationMark, AnnotationPixelSize, AnnotationSize};
use super::asset::CaptureAsset;

pub const MAX_VIDEO_PROJECT_BYTES: usize = 48 * 1024 * 1024;
const MAX_STORED_BYTES: usize = MAX_VIDEO_PROJECT_BYTES + 4096;
const MAX_STICKER_BYTES: usize = 32 * 1024 * 1024;
const MAX_STICKER_PIXELS_BYTES: u64 = 128 * 1024 * 1024;
const MAX_ITEMS: usize = 128;

#[derive(Debug, Error)]
pub enum VideoProjectError {
    #[error("VIDEO_PROJECT_INVALID")]
    Invalid,
    #[error("VIDEO_PROJECT_SOURCE_CHANGED")]
    SourceChanged,
    #[error("VIDEO_PROJECT_CONFLICT")]
    Conflict,
    #[error("VIDEO_PROJECT_UNAVAILABLE")]
    Unavailable,
    #[error("VIDEO_PROJECT_TRASHED")]
    Trashed,
    #[error("VIDEO_PROJECT_IO")]
    Io(#[from] std::io::Error),
}

type Result<T> = std::result::Result<T, VideoProjectError>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VideoProject {
    pub schema_version: u8,
    pub source_size: AnnotationPixelSize,
    pub source_duration: f64,
    pub edit: VideoEdit,
    pub preset: VideoProjectPreset,
    /// The source-video time, matching the editor's video element.
    pub playhead: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VideoProjectPreset {
    Original,
    Share,
    Small,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VideoEdit {
    pub segments: Vec<ProjectSegment>,
    pub effects: Vec<ProjectEffect>,
    pub annotations: Vec<ProjectAnnotation>,
    pub stickers: Vec<ProjectSticker>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectSegment {
    pub start: f64,
    pub end: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speed: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProjectEffectKind {
    Zoom,
    Mask,
    Spotlight,
    Frame,
    Fade,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProjectMaskStyle {
    Solid,
    Blur,
    Pixelate,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectEffect {
    pub id: String,
    pub kind: ProjectEffectKind,
    pub start: f64,
    pub end: f64,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mask_style: Option<ProjectMaskStyle>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strength: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transition: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectAnnotation {
    pub id: String,
    pub start: f64,
    pub end: f64,
    pub mark: AnnotationMark,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectSticker {
    pub id: String,
    pub start: f64,
    pub end: f64,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub data_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer: Option<i32>,
}

impl VideoProject {
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != 1
            || self.source_size.width == 0
            || self.source_size.height == 0
            || self.source_size.width > 65_536
            || self.source_size.height > 65_536
            || !self.source_duration.is_finite()
            || !(0.0..=2_678_400.0).contains(&self.source_duration)
            || self.source_duration == 0.0
            || !self.playhead.is_finite()
            || !(0.0..=self.source_duration).contains(&self.playhead)
            || self.edit.segments.len() > MAX_ITEMS
            || self.edit.effects.len() > MAX_ITEMS
            || self.edit.annotations.len() + self.edit.stickers.len() > MAX_ITEMS
        {
            return Err(VideoProjectError::Invalid);
        }
        for segment in &self.edit.segments {
            validate_range(segment.start, segment.end, self.source_duration)?;
            let speed = segment.speed.unwrap_or(1.0);
            if !speed.is_finite() || !(0.25..=4.0).contains(&speed) {
                return Err(VideoProjectError::Invalid);
            }
        }
        let mut segments: Vec<_> = self.edit.segments.iter().collect();
        segments.sort_by(|a, b| a.start.total_cmp(&b.start));
        if segments.windows(2).any(|pair| pair[0].end > pair[1].start) {
            return Err(VideoProjectError::Invalid);
        }

        let mut ids = HashSet::new();
        for (index, effect) in self.edit.effects.iter().enumerate() {
            validate_id(&effect.id, &mut ids)?;
            validate_layer(effect.layer)?;
            validate_range(effect.start, effect.end, self.source_duration)?;
            validate_rectangle(effect.x, effect.y, effect.width, effect.height, 0.02)?;
            let strength = effect.strength.unwrap_or(0.5);
            let transition = effect.transition.unwrap_or(0.0);
            if !strength.is_finite()
                || !(0.0..=1.0).contains(&strength)
                || !transition.is_finite()
                || !(0.0..=2.0).contains(&transition)
                || effect.color.unwrap_or(0) > 0x00ff_ffff
                || (effect.kind == ProjectEffectKind::Zoom
                    && (effect.width - effect.height).abs() > 0.000001)
            {
                return Err(VideoProjectError::Invalid);
            }
            if matches!(
                effect.kind,
                ProjectEffectKind::Zoom | ProjectEffectKind::Frame | ProjectEffectKind::Fade
            ) && self.edit.effects[..index].iter().any(|other| {
                other.kind == effect.kind && effect.start < other.end && effect.end > other.start
            }) {
                return Err(VideoProjectError::Invalid);
            }
        }
        let mut annotation_points = 0usize;
        let mut annotation_text_units = 0usize;
        for annotation in &self.edit.annotations {
            validate_id(&annotation.id, &mut ids)?;
            validate_layer(annotation.layer)?;
            validate_range(annotation.start, annotation.end, self.source_duration)?;
            match &annotation.mark {
                AnnotationMark::Pen { points, .. } | AnnotationMark::Mosaic { points, .. } => {
                    annotation_points = annotation_points.saturating_add(points.len());
                }
                AnnotationMark::Text { text, .. } => {
                    // Match the shared validator's cap before cloning marks.
                    annotation_text_units = annotation_text_units
                        .saturating_add(text.encode_utf16().take(65_537).count());
                }
                _ => {}
            }
            if annotation_points > 100_000 || annotation_text_units > 65_536 {
                return Err(VideoProjectError::Invalid);
            }
        }
        // Reuse the capture/editor validator for mark geometry, enums, ids,
        // total points/text and document size instead of trusting draft JSON.
        let annotation_document = AnnotationDocument {
            schema_version: 1,
            canvas: AnnotationSize {
                width: f64::from(self.source_size.width),
                height: f64::from(self.source_size.height),
            },
            source_pixels: self.source_size,
            marks: self
                .edit
                .annotations
                .iter()
                .map(|item| item.mark.clone())
                .collect(),
        };
        annotation_document
            .to_json()
            .map_err(|_| VideoProjectError::Invalid)?;

        let mut encoded_size = 0usize;
        let mut pixels_left = MAX_STICKER_PIXELS_BYTES;
        for sticker in &self.edit.stickers {
            validate_id(&sticker.id, &mut ids)?;
            validate_layer(sticker.layer)?;
            validate_range(sticker.start, sticker.end, self.source_duration)?;
            validate_rectangle(
                sticker.x,
                sticker.y,
                sticker.width,
                sticker.height,
                f64::MIN_POSITIVE,
            )?;
            encoded_size = encoded_size
                .checked_add(sticker.data_url.len())
                .ok_or(VideoProjectError::Invalid)?;
            if encoded_size > MAX_STICKER_BYTES {
                return Err(VideoProjectError::Invalid);
            }
            let encoded = sticker
                .data_url
                .strip_prefix("data:image/png;base64,")
                .ok_or(VideoProjectError::Invalid)?;
            let png = base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .map_err(|_| VideoProjectError::Invalid)?;
            let mut limits = image::Limits::default();
            limits.max_image_width = Some(2048);
            limits.max_image_height = Some(2048);
            limits.max_alloc = Some(pixels_left);
            let decoder =
                image::codecs::png::PngDecoder::with_limits(std::io::Cursor::new(png), limits)
                    .map_err(|_| VideoProjectError::Invalid)?;
            let (width, height) = decoder.dimensions();
            let rgba_bytes = u64::from(width) * u64::from(height) * 4;
            let peak = if decoder.color_type() == image::ColorType::Rgba8 {
                rgba_bytes
            } else {
                rgba_bytes + decoder.total_bytes()
            };
            if peak > pixels_left {
                return Err(VideoProjectError::Invalid);
            }
            image::DynamicImage::from_decoder(decoder).map_err(|_| VideoProjectError::Invalid)?;
            pixels_left -= rgba_bytes;
        }
        Ok(())
    }

    pub fn to_json(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let bytes = serde_json::to_vec(self).map_err(|_| VideoProjectError::Invalid)?;
        if bytes.len() > MAX_VIDEO_PROJECT_BYTES {
            return Err(VideoProjectError::Invalid);
        }
        Ok(bytes)
    }

    fn matches_source(&self, source: &VideoSourceIdentity) -> bool {
        i64::from(self.source_size.width) == source.pixel_width
            && i64::from(self.source_size.height) == source.pixel_height
            && source
                .duration
                .is_none_or(|duration| (self.source_duration - duration).abs() <= 0.1)
    }
}

fn validate_range(start: f64, end: f64, duration: f64) -> Result<()> {
    if !start.is_finite()
        || !end.is_finite()
        || start < 0.0
        || end <= start
        || start >= duration
        || end > duration + 0.05
    {
        Err(VideoProjectError::Invalid)
    } else {
        Ok(())
    }
}

fn validate_rectangle(x: f64, y: f64, width: f64, height: f64, minimum: f64) -> Result<()> {
    if [x, y, width, height].iter().any(|v| !v.is_finite())
        || x < 0.0
        || y < 0.0
        || width < minimum
        || height < minimum
        || x + width > 1.000001
        || y + height > 1.000001
    {
        Err(VideoProjectError::Invalid)
    } else {
        Ok(())
    }
}

fn validate_id<'a>(id: &'a str, ids: &mut HashSet<&'a str>) -> Result<()> {
    if id.is_empty() || id.len() > 128 || id.chars().any(char::is_control) || !ids.insert(id) {
        Err(VideoProjectError::Invalid)
    } else {
        Ok(())
    }
}

fn validate_layer(layer: Option<i32>) -> Result<()> {
    if layer.is_some_and(|value| !(0..=1_000_000).contains(&value)) {
        Err(VideoProjectError::Invalid)
    } else {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum VideoProjectState {
    None,
    Valid,
    Invalid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum VideoProjectInvalidReason {
    InvalidDocument,
    SourceChanged,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoProjectSnapshot {
    pub state: VideoProjectState,
    pub revision: String,
    pub project: Option<VideoProject>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<VideoProjectInvalidReason>,
}

/// Cheap metadata fingerprint: autosave never reads the multi-gigabyte video.
/// Library migration preserves its modification timestamp after byte checks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct VideoSourceIdentity {
    asset_id: uuid::Uuid,
    filename: String,
    created_at: f64,
    pixel_width: i64,
    pixel_height: i64,
    duration: Option<f64>,
    bytes: u64,
    modified_seconds: u64,
    modified_nanos: u32,
}

impl VideoSourceIdentity {
    fn read(asset: &CaptureAsset, source: &Path) -> Result<Self> {
        if !asset.created_at.is_finite()
            || asset
                .duration
                .is_some_and(|duration| !duration.is_finite() || duration <= 0.0)
        {
            return Err(VideoProjectError::Unavailable);
        }
        let metadata = std::fs::symlink_metadata(source)?;
        if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() == 0 {
            return Err(VideoProjectError::Unavailable);
        }
        let modified = metadata
            .modified()?
            .duration_since(UNIX_EPOCH)
            .map_err(|_| VideoProjectError::Unavailable)?;
        Ok(Self {
            asset_id: asset.id,
            filename: asset.filename.clone(),
            created_at: asset.created_at,
            pixel_width: asset.pixel_width,
            pixel_height: asset.pixel_height,
            duration: asset.duration,
            bytes: metadata.len(),
            modified_seconds: modified.as_secs(),
            modified_nanos: modified.subsec_nanos(),
        })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredVideoProject {
    version: u8,
    library_id: uuid::Uuid,
    source: VideoSourceIdentity,
    project: VideoProject,
}

enum StoredBytes {
    Absent,
    Present(Vec<u8>),
    Oversized(Vec<u8>),
}

fn read_document(path: &Path) -> Result<StoredBytes> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(StoredBytes::Absent)
        }
        Err(error) => return Err(error.into()),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(VideoProjectError::Unavailable);
    }
    if metadata.len() > MAX_STORED_BYTES as u64 {
        // An invalid oversized file can never be overwritten through save, so
        // its size/timestamp are sufficient to identify the rejected snapshot.
        return Ok(StoredBytes::Oversized(
            format!("{}:{:?}", metadata.len(), metadata.modified()?).into_bytes(),
        ));
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    std::fs::File::open(path)?
        .take(MAX_STORED_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_STORED_BYTES {
        return Ok(StoredBytes::Oversized(
            format!("{}:grew", bytes.len()).into_bytes(),
        ));
    }
    Ok(StoredBytes::Present(bytes))
}

fn revision_for(
    source: &VideoSourceIdentity,
    library_id: uuid::Uuid,
    generation: uuid::Uuid,
    bytes: &StoredBytes,
) -> String {
    let mut digest = Sha256::new();
    digest.update(b"kiri-video-project-v1");
    digest.update(library_id.as_bytes());
    digest.update(generation.as_bytes());
    digest.update(serde_json::to_vec(source).expect("finite asset metadata"));
    match bytes {
        StoredBytes::Absent => digest.update([0]),
        StoredBytes::Oversized(metadata) => {
            digest.update([2]);
            digest.update(metadata);
        }
        StoredBytes::Present(bytes) => {
            digest.update([1]);
            digest.update(bytes);
        }
    }
    format!("{:x}", digest.finalize())
}

fn snapshot(
    source: &VideoSourceIdentity,
    library_id: uuid::Uuid,
    generation: uuid::Uuid,
    bytes: StoredBytes,
) -> VideoProjectSnapshot {
    let revision = revision_for(source, library_id, generation, &bytes);
    let mut state = VideoProjectState::None;
    let mut reason = None;
    let mut project = None;
    match bytes {
        StoredBytes::Absent => {}
        StoredBytes::Oversized(_) => {
            state = VideoProjectState::Invalid;
            reason = Some(VideoProjectInvalidReason::InvalidDocument);
        }
        StoredBytes::Present(bytes) => {
            state = VideoProjectState::Invalid;
            reason = Some(VideoProjectInvalidReason::InvalidDocument);
            if let Ok(stored) = serde_json::from_slice::<StoredVideoProject>(&bytes) {
                if stored.version == 1
                    && stored.library_id == library_id
                    && stored.project.to_json().is_ok()
                {
                    if &stored.source != source || !stored.project.matches_source(source) {
                        reason = Some(VideoProjectInvalidReason::SourceChanged);
                    } else {
                        state = VideoProjectState::Valid;
                        project = Some(stored.project);
                        reason = None;
                    }
                }
            }
        }
    }
    VideoProjectSnapshot {
        state,
        revision,
        project,
        reason,
    }
}

pub(super) fn load_project(
    asset: &CaptureAsset,
    source_path: &Path,
    project_path: &Path,
    library_id: uuid::Uuid,
    generation: uuid::Uuid,
) -> Result<VideoProjectSnapshot> {
    let source = VideoSourceIdentity::read(asset, source_path)?;
    let loaded = snapshot(
        &source,
        library_id,
        generation,
        read_document(project_path)?,
    );
    if VideoSourceIdentity::read(asset, source_path)? != source {
        return Err(VideoProjectError::SourceChanged);
    }
    Ok(loaded)
}

pub(super) fn save_project(
    asset: &CaptureAsset,
    source_path: &Path,
    project_path: &Path,
    library_id: uuid::Uuid,
    generation: uuid::Uuid,
    revision: &str,
    project: VideoProject,
) -> Result<VideoProjectSnapshot> {
    if asset.trashed_at.is_some() {
        return Err(VideoProjectError::Trashed);
    }
    project.to_json()?;
    let source = VideoSourceIdentity::read(asset, source_path)?;
    let current = snapshot(
        &source,
        library_id,
        generation,
        read_document(project_path)?,
    );
    if current.revision != revision {
        return Err(VideoProjectError::Conflict);
    }
    if current.state == VideoProjectState::Invalid {
        return Err(match current.reason {
            Some(VideoProjectInvalidReason::SourceChanged) => VideoProjectError::SourceChanged,
            _ => VideoProjectError::Invalid,
        });
    }
    if !project.matches_source(&source) {
        return Err(VideoProjectError::SourceChanged);
    }
    let stored = StoredVideoProject {
        version: 1,
        library_id,
        source: source.clone(),
        project,
    };
    let bytes = serde_json::to_vec(&stored).map_err(|_| VideoProjectError::Invalid)?;
    if bytes.len() > MAX_STORED_BYTES {
        return Err(VideoProjectError::Invalid);
    }
    let parent = project_path
        .parent()
        .ok_or(VideoProjectError::Unavailable)?;
    let mut temporary = tempfile::Builder::new()
        .prefix(".video-project-")
        .suffix(".tmp")
        .tempfile_in(parent)?;
    temporary.write_all(&bytes)?;
    temporary.flush()?;
    temporary.as_file().sync_all()?;
    // Recheck after decoding and staging: external source/document changes
    // must not be hidden by a lengthy sticker validation or disk write.
    if VideoSourceIdentity::read(asset, source_path)? != source {
        return Err(VideoProjectError::SourceChanged);
    }
    let latest = revision_for(
        &source,
        library_id,
        generation,
        &read_document(project_path)?,
    );
    if latest != revision {
        return Err(VideoProjectError::Conflict);
    }
    temporary
        .persist(project_path)
        .map_err(|error| VideoProjectError::Io(error.error))?;
    #[cfg(unix)]
    if let Err(error) = std::fs::File::open(parent).and_then(|file| file.sync_all()) {
        log::warn!("video project saved but its directory could not be synced: {error}");
    }
    Ok(VideoProjectSnapshot {
        state: VideoProjectState::Valid,
        revision: revision_for(
            &source,
            library_id,
            generation,
            &StoredBytes::Present(bytes),
        ),
        project: Some(stored.project),
        reason: None,
    })
}

#[cfg(test)]
pub(super) fn test_project() -> VideoProject {
    serde_json::from_value(serde_json::json!({
        "schemaVersion": 1, "sourceSize": {"width": 1920, "height": 1080}, "sourceDuration": 20,
        "edit": {
            "segments": [{"start": 8, "end": 18, "speed": 2}, {"start": 0, "end": 5}],
            "effects": [{"id": "mask-1", "kind": "mask", "start": 9, "end": 12, "x": 0.1, "y": 0.1, "width": 0.2, "height": 0.2, "maskStyle": "blur", "layer": 2}],
            "annotations": [{"id": "text-1", "start": 0, "end": 5, "mark": {"id": 1, "kind": "text", "text": "剪辑草稿", "rect": {"x": 100, "y": 100, "width": 200, "height": 50}, "color": "cherry", "background": "transparent", "fontSize": 24}}],
            "stickers": []
        }, "preset": "share", "playhead": 20
    })).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::asset::CaptureKind;
    use crate::core::library::AssetLibrary;

    fn fixture() -> (
        tempfile::TempDir,
        AssetLibrary,
        CaptureAsset,
        uuid::Uuid,
        uuid::Uuid,
    ) {
        let directory = tempfile::tempdir().unwrap();
        let mut library = AssetLibrary::open(directory.path().to_path_buf()).unwrap();
        let asset = library
            .import_data(
                b"video source fixture",
                CaptureKind::Video,
                "mp4",
                1920,
                1080,
                Some(20.0),
                None,
                None,
            )
            .unwrap();
        (
            directory,
            library,
            asset,
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
        )
    }

    #[test]
    fn video_project_saves_reloads_without_changing_source_or_index() {
        let (directory, library, asset, library_id, generation) = fixture();
        let index_before = std::fs::read(directory.path().join("library.json")).unwrap();
        let source_before = std::fs::read(library.asset_url(&asset)).unwrap();
        let initial = library
            .load_video_project(&asset.id, library_id, generation)
            .unwrap();
        assert_eq!(initial.state, VideoProjectState::None);
        assert_eq!(initial.revision.len(), 64);
        assert!(!directory.path().join("VideoProjects").exists());
        let saved = library
            .save_video_project(
                &asset.id,
                library_id,
                generation,
                &initial.revision,
                test_project(),
            )
            .unwrap();
        let reopened = AssetLibrary::open_existing(directory.path().to_path_buf()).unwrap();
        assert_eq!(
            reopened
                .load_video_project(&asset.id, library_id, generation)
                .unwrap(),
            saved
        );
        assert_eq!(saved.project, Some(test_project()));
        assert_eq!(
            std::fs::read(library.asset_url(&asset)).unwrap(),
            source_before
        );
        assert_eq!(
            std::fs::read(directory.path().join("library.json")).unwrap(),
            index_before
        );
    }

    #[test]
    fn video_project_stale_saves_and_changed_library_copies_cannot_overwrite() {
        let (_directory, library, asset, library_id, generation) = fixture();
        let first = library
            .load_video_project(&asset.id, library_id, generation)
            .unwrap();
        let saved = library
            .save_video_project(
                &asset.id,
                library_id,
                generation,
                &first.revision,
                test_project(),
            )
            .unwrap();
        assert!(matches!(
            library.save_video_project(
                &asset.id,
                library_id,
                generation,
                &first.revision,
                test_project()
            ),
            Err(VideoProjectError::Conflict)
        ));
        let new_generation = uuid::Uuid::new_v4();
        let moved = library
            .load_video_project(&asset.id, library_id, new_generation)
            .unwrap();
        assert_eq!(moved.state, VideoProjectState::Valid);
        assert_ne!(saved.revision, moved.revision);
        assert!(matches!(
            library.save_video_project(
                &asset.id,
                library_id,
                new_generation,
                &saved.revision,
                test_project()
            ),
            Err(VideoProjectError::Conflict)
        ));
    }

    #[test]
    fn video_project_source_drift_preserves_the_previous_draft() {
        let (directory, library, asset, library_id, generation) = fixture();
        let empty = library
            .load_video_project(&asset.id, library_id, generation)
            .unwrap();
        let saved = library
            .save_video_project(
                &asset.id,
                library_id,
                generation,
                &empty.revision,
                test_project(),
            )
            .unwrap();
        let path = directory
            .path()
            .join("VideoProjects")
            .join(format!("{}.json", asset.id));
        let previous = std::fs::read(&path).unwrap();
        std::fs::write(
            library.asset_url(&asset),
            b"replacement video with different length",
        )
        .unwrap();
        let changed = library
            .load_video_project(&asset.id, library_id, generation)
            .unwrap();
        assert_eq!(changed.state, VideoProjectState::Invalid);
        assert_eq!(
            changed.reason,
            Some(VideoProjectInvalidReason::SourceChanged)
        );
        assert!(changed.project.is_none());
        assert!(matches!(
            library.save_video_project(
                &asset.id,
                library_id,
                generation,
                &saved.revision,
                test_project()
            ),
            Err(VideoProjectError::Conflict)
        ));
        assert!(matches!(
            library.save_video_project(
                &asset.id,
                library_id,
                generation,
                &changed.revision,
                test_project()
            ),
            Err(VideoProjectError::SourceChanged)
        ));
        assert_eq!(std::fs::read(path).unwrap(), previous);
    }

    #[test]
    fn video_project_detects_same_size_source_modification() {
        let (_directory, library, asset, library_id, generation) = fixture();
        let empty = library
            .load_video_project(&asset.id, library_id, generation)
            .unwrap();
        library
            .save_video_project(
                &asset.id,
                library_id,
                generation,
                &empty.revision,
                test_project(),
            )
            .unwrap();
        let path = library.asset_url(&asset);
        let before = std::fs::metadata(&path).unwrap();
        std::fs::write(&path, vec![0; before.len() as usize]).unwrap();
        std::fs::OpenOptions::new()
            .write(true)
            .open(path)
            .unwrap()
            .set_times(
                std::fs::FileTimes::new()
                    .set_modified(before.modified().unwrap() + std::time::Duration::from_secs(1)),
            )
            .unwrap();
        let changed = library
            .load_video_project(&asset.id, library_id, generation)
            .unwrap();
        assert_eq!(
            changed.reason,
            Some(VideoProjectInvalidReason::SourceChanged)
        );
    }

    #[test]
    fn video_project_invalid_and_oversized_sidecars_are_read_only() {
        let (directory, library, asset, library_id, generation) = fixture();
        let parent = directory.path().join("VideoProjects");
        std::fs::create_dir(&parent).unwrap();
        let path = parent.join(format!("{}.json", asset.id));
        std::fs::write(&path, b"{broken").unwrap();
        let invalid = library
            .load_video_project(&asset.id, library_id, generation)
            .unwrap();
        assert_eq!(
            invalid.reason,
            Some(VideoProjectInvalidReason::InvalidDocument)
        );
        assert!(matches!(
            library.save_video_project(
                &asset.id,
                library_id,
                generation,
                &invalid.revision,
                test_project()
            ),
            Err(VideoProjectError::Invalid)
        ));
        assert_eq!(std::fs::read(&path).unwrap(), b"{broken");
        std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_len(MAX_STORED_BYTES as u64 + 1)
            .unwrap();
        let oversized = library
            .load_video_project(&asset.id, library_id, generation)
            .unwrap();
        assert_eq!(oversized.state, VideoProjectState::Invalid);
        assert!(oversized.project.is_none());
        assert_ne!(invalid.revision, oversized.revision);
    }

    #[test]
    fn video_project_survives_trash_restore_then_is_permanently_removed() {
        let (directory, mut library, asset, library_id, generation) = fixture();
        let empty = library
            .load_video_project(&asset.id, library_id, generation)
            .unwrap();
        let saved = library
            .save_video_project(
                &asset.id,
                library_id,
                generation,
                &empty.revision,
                test_project(),
            )
            .unwrap();
        library.move_to_trash(&asset.id).unwrap();
        assert_eq!(
            library
                .load_video_project(&asset.id, library_id, generation)
                .unwrap(),
            saved
        );
        assert!(matches!(
            library.save_video_project(
                &asset.id,
                library_id,
                generation,
                &saved.revision,
                test_project()
            ),
            Err(VideoProjectError::Trashed)
        ));
        library.restore(&asset.id).unwrap();
        assert_eq!(
            library
                .load_video_project(&asset.id, library_id, generation)
                .unwrap(),
            saved
        );
        library.move_to_trash(&asset.id).unwrap();
        library.permanently_delete(&asset.id).unwrap();
        assert!(!directory
            .path()
            .join("VideoProjects")
            .join(format!("{}.json", asset.id))
            .exists());
    }

    #[test]
    fn video_project_validates_edit_ranges_bounds_and_closed_schema() {
        let mut project = test_project();
        assert!(project.validate().is_ok());
        project.edit.segments.clear(); // Deleting the final clip is recoverable.
        assert!(project.validate().is_ok());
        let base = serde_json::to_value(test_project()).unwrap();
        let mut unknown = base.clone();
        unknown["futureField"] = true.into();
        assert!(serde_json::from_value::<VideoProject>(unknown).is_err());
        let mut unknown_mark = base.clone();
        unknown_mark["edit"]["annotations"][0]["mark"]["futureField"] = true.into();
        assert!(serde_json::from_value::<VideoProject>(unknown_mark).is_err());
        for (pointer, value) in [
            ("/schemaVersion", serde_json::json!(2)),
            ("/sourceSize/width", serde_json::json!(0)),
            ("/playhead", serde_json::json!(20.01)),
            ("/edit/segments/0/speed", serde_json::json!(8)),
            ("/edit/segments/1/end", serde_json::json!(10)),
            ("/edit/effects/0/width", serde_json::json!(1)),
            ("/edit/annotations/0/mark/fontSize", serde_json::json!(-1)),
        ] {
            let mut invalid = base.clone();
            *invalid.pointer_mut(pointer).unwrap() = value;
            let invalid: VideoProject = serde_json::from_value(invalid).unwrap();
            assert!(invalid.validate().is_err(), "{pointer}");
        }
        let mut too_many = test_project();
        too_many.edit.effects = vec![too_many.edit.effects[0].clone(); 129];
        assert!(too_many.validate().is_err());
    }

    #[test]
    fn video_project_decodes_stickers_and_rejects_invalid_pngs() {
        let mut project = test_project();
        let mut png = std::io::Cursor::new(Vec::new());
        image::RgbaImage::from_pixel(2, 2, image::Rgba([255, 0, 0, 255]))
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        project.edit.stickers.push(ProjectSticker {
            id: "sticker-1".into(),
            start: 0.0,
            end: 4.0,
            x: 0.0,
            y: 0.0,
            width: 0.1,
            height: 0.1,
            data_url: format!(
                "data:image/png;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(png.into_inner())
            ),
            layer: Some(1),
        });
        assert!(project.validate().is_ok());
        project.edit.stickers[0].data_url = "data:image/png;base64,ZmFrZQ==".into();
        assert!(project.validate().is_err());
        project.edit.stickers[0].data_url = "data:image/jpeg;base64,ZmFrZQ==".into();
        assert!(project.validate().is_err());
    }

    #[test]
    fn video_project_invalid_new_edit_keeps_prior_good_draft() {
        let (_directory, library, asset, library_id, generation) = fixture();
        let empty = library
            .load_video_project(&asset.id, library_id, generation)
            .unwrap();
        let saved = library
            .save_video_project(
                &asset.id,
                library_id,
                generation,
                &empty.revision,
                test_project(),
            )
            .unwrap();
        let mut bad = test_project();
        bad.edit.segments[0].speed = Some(-1.0);
        assert!(library
            .save_video_project(&asset.id, library_id, generation, &saved.revision, bad)
            .is_err());
        assert_eq!(
            library
                .load_video_project(&asset.id, library_id, generation)
                .unwrap(),
            saved
        );
    }
}
