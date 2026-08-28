//! Local asset-library persistence and recoverable Trash operations.
//! Storage layout and JSON schema stay compatible with the Swift app, so an
//! existing library at ~/Library/Application Support/kiri keeps working.

#[cfg(test)]
use std::cell::Cell;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use chrono::{Local, TimeZone};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::core::annotation::AnnotationDocument;
use crate::core::asset::{CaptureAsset, CaptureKind};

#[derive(Debug, Error)]
pub enum AssetLibraryError {
    #[error("asset not found")]
    AssetNotFound,
    #[error("invalid filename")]
    InvalidFilename,
    #[error("library index contains duplicate asset ids or filenames")]
    DuplicateIndexEntry,
    #[error("the asset file is missing")]
    AssetFileMissing,
    #[error("the asset file is still present")]
    AssetFileStillPresent,
    #[error("the selected replacement does not match the asset type")]
    InvalidReplacementFile,
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("library index is corrupted: {0}")]
    CorruptIndex(#[source] serde_json::Error),
    #[error("library index was updated, but {failed_files} file(s) could not be removed")]
    CleanupFailed { failed_files: usize },
    #[error("annotation projects are available only for image assets")]
    UnsupportedAnnotationAsset,
    #[error("annotation project already exists")]
    AnnotationProjectAlreadyExists,
    #[error("annotation project files are incomplete")]
    IncompleteAnnotationProject,
    #[error("annotation project is corrupted: {0}")]
    CorruptAnnotationProject(String),
    #[error("annotation project no longer matches the stored image")]
    StaleAnnotationProject,
    #[error("annotation editor snapshot changed before it could be saved")]
    AnnotationRevisionMismatch,
}

pub type Result<T> = std::result::Result<T, AssetLibraryError>;

const ANNOTATION_PROJECT_VERSION: u32 = 1;

/// Test-only view of a validated annotation project.
#[cfg(test)]
#[derive(Debug, Clone, PartialEq)]
struct LoadedAnnotationProject {
    document: serde_json::Value,
    document_url: PathBuf,
    source_url: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EditorAnnotationState {
    None,
    Valid,
    Invalid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AssetAvailability {
    Ready,
    Missing,
    Unreadable,
}

/// One content-addressed editor baseline. `source` is the exact image the
/// editor must display: the immutable clean source for a valid project, or the
/// current flattened image when no usable project exists.
#[derive(Debug, Clone, PartialEq)]
pub struct LoadedEditorSnapshot {
    pub revision_sha256: String,
    pub state: EditorAnnotationState,
    pub document: Option<serde_json::Value>,
    pub source: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredAnnotationProject {
    project_version: u32,
    source_sha256: String,
    rendered_sha256: String,
    document: serde_json::Value,
}

struct EditorSnapshotFiles {
    asset: CaptureAsset,
    rendered: Vec<u8>,
    encoded: Option<Vec<u8>>,
    source: Option<Vec<u8>>,
    document_url: PathBuf,
    source_url: PathBuf,
    revision_sha256: String,
    state: EditorAnnotationState,
    valid_project: Option<StoredAnnotationProject>,
}

pub struct AssetLibrary {
    assets_url: PathBuf,
    thumbnails_url: PathBuf,
    annotations_url: PathBuf,
    index_url: PathBuf,
    index: Vec<CaptureAsset>,
    #[cfg(test)]
    persist_count: Cell<usize>,
    #[cfg(test)]
    annotation_replace_fail_at_write: Cell<Option<usize>>,
}

impl AssetLibrary {
    pub fn open(root_url: PathBuf) -> Result<Self> {
        Self::open_with_creation(root_url, true)
    }

    pub fn open_existing(root_url: PathBuf) -> Result<Self> {
        Self::open_with_creation(root_url, false)
    }

    fn open_with_creation(root_url: PathBuf, allow_creation: bool) -> Result<Self> {
        let assets_url = root_url.join("Assets");
        let thumbnails_url = root_url.join("Thumbnails");
        let annotations_url = root_url.join("Annotations");
        let index_url = root_url.join("library.json");

        if allow_creation {
            std::fs::create_dir_all(&root_url)?;
            ensure_directory(&root_url)?;
            ensure_or_create_directory(&assets_url)?;
            ensure_or_create_directory(&thumbnails_url)?;
            ensure_or_create_directory(&annotations_url)?;
        } else {
            ensure_directory(&root_url)?;
            ensure_directory(&assets_url)?;
            ensure_directory(&thumbnails_url)?;
            ensure_directory(&annotations_url)?;
        }

        let index = match std::fs::symlink_metadata(&index_url) {
            Ok(_) => {
                ensure_regular_file(&index_url)?;
                let data = std::fs::read(&index_url)?;
                serde_json::from_slice::<Vec<CaptureAsset>>(&data)
                    .map_err(AssetLibraryError::CorruptIndex)?
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && allow_creation => {
                atomic_write(&index_url, b"[]")?;
                Vec::new()
            }
            Err(error) => return Err(AssetLibraryError::Io(error)),
        };
        validate_index(&index)?;

        Ok(Self {
            assets_url,
            thumbnails_url,
            annotations_url,
            index_url,
            index,
            #[cfg(test)]
            persist_count: Cell::new(0),
            #[cfg(test)]
            annotation_replace_fail_at_write: Cell::new(None),
        })
    }

    /// Mirrors `AssetLibrary.defaultRootURL()`: `~/Library/Application
    /// Support/kiri` on macOS, `%APPDATA%/kiri` on Windows.
    pub fn default_root_url() -> Option<PathBuf> {
        dirs::data_dir().map(|dir| dir.join("kiri"))
    }

    /// Revalidates the on-disk library boundary after startup. Custom-library
    /// volumes can disappear or be replaced while Kiri is running, so callers
    /// must not rely only on the checks performed by `open_existing`.
    pub fn validate_storage_layout(&self) -> Result<()> {
        let root_url = self
            .index_url
            .parent()
            .ok_or(AssetLibraryError::InvalidFilename)?;
        ensure_directory(root_url)?;
        ensure_directory(&self.assets_url)?;
        ensure_directory(&self.thumbnails_url)?;
        ensure_directory(&self.annotations_url)?;
        ensure_regular_file(&self.index_url)?;
        Ok(())
    }

    pub fn asset_url(&self, asset: &CaptureAsset) -> PathBuf {
        self.assets_url.join(&asset.filename)
    }

    pub fn asset_availability(&self, id: &uuid::Uuid) -> Result<AssetAvailability> {
        self.validate_storage_layout()?;
        let asset = self
            .asset_by_id(id)
            .ok_or(AssetLibraryError::AssetNotFound)?;
        Ok(asset_file_availability(&self.asset_url(asset)))
    }

    pub fn readable_asset_url(&self, asset: &CaptureAsset) -> Result<PathBuf> {
        self.validate_storage_layout()?;
        let path = self.asset_url(asset);
        match asset_file_availability(&path) {
            AssetAvailability::Ready => Ok(path),
            AssetAvailability::Missing => Err(AssetLibraryError::AssetFileMissing),
            AssetAvailability::Unreadable => Err(AssetLibraryError::Io(std::io::Error::other(
                "asset is not a readable regular file",
            ))),
        }
    }

    pub fn restore_missing_asset(
        &self,
        id: &uuid::Uuid,
        source: &Path,
        proof: &ReplacementFileProof,
    ) -> Result<CaptureAsset> {
        let asset = self
            .asset_by_id(id)
            .cloned()
            .ok_or(AssetLibraryError::AssetNotFound)?;
        if self.asset_availability(id)? != AssetAvailability::Missing {
            return Err(AssetLibraryError::AssetFileStillPresent);
        }
        if proof.kind != asset.kind {
            return Err(AssetLibraryError::InvalidReplacementFile);
        }
        atomic_copy_missing(source, &self.asset_url(&asset), Some(proof))?;
        Ok(asset)
    }

    pub fn remove_missing_asset(&mut self, id: &uuid::Uuid) -> Result<()> {
        if self.asset_availability(id)? != AssetAvailability::Missing {
            return Err(AssetLibraryError::AssetFileStillPresent);
        }
        self.permanently_delete(id)
    }

    /// Mirrors `AssetLibrary.allAssets(includeTrashed:)`.
    pub fn all_assets(&self, include_trashed: bool) -> Vec<CaptureAsset> {
        let mut assets: Vec<CaptureAsset> = self
            .index
            .iter()
            .filter(|asset| {
                if include_trashed {
                    // Trash view: only deleted captures.
                    asset.trashed_at.is_some()
                } else {
                    // Library view: only active captures.
                    asset.trashed_at.is_none()
                }
            })
            .cloned()
            .collect();
        // createdAt descending.
        assets.sort_by(|a, b| b.created_at.total_cmp(&a.created_at));
        assets
    }

    /// Mirrors `AssetLibrary.search(_:includeTrashed:)`.
    pub fn search(&self, query: &str, include_trashed: bool) -> Vec<CaptureAsset> {
        let normalized = query.trim().to_lowercase();
        self.all_assets(include_trashed)
            .into_iter()
            .filter(|asset| normalized.is_empty() || asset.searchable_text().contains(&normalized))
            .collect()
    }

    pub fn asset_by_id(&self, id: &uuid::Uuid) -> Option<&CaptureAsset> {
        self.index.iter().find(|asset| &asset.id == id)
    }

    /// Mirrors `AssetLibrary.importData(...)`.
    #[allow(clippy::too_many_arguments)]
    pub fn import_data(
        &mut self,
        data: &[u8],
        kind: CaptureKind,
        file_extension: &str,
        pixel_width: i64,
        pixel_height: i64,
        duration: Option<f64>,
        source_application: Option<String>,
        created_at: Option<f64>,
    ) -> Result<CaptureAsset> {
        self.import_data_inner(
            data,
            kind,
            file_extension,
            pixel_width,
            pixel_height,
            duration,
            source_application,
            created_at,
            None,
        )
    }

    /// Imports a flattened image together with its immutable clean source and
    /// frontend-owned annotation document. The Swift-compatible index remains
    /// unchanged; the project lives entirely under `Annotations/`.
    #[allow(clippy::too_many_arguments)]
    pub fn import_data_with_annotation_project(
        &mut self,
        rendered_data: &[u8],
        kind: CaptureKind,
        file_extension: &str,
        pixel_width: i64,
        pixel_height: i64,
        duration: Option<f64>,
        source_application: Option<String>,
        created_at: Option<f64>,
        clean_source: &[u8],
        document: &serde_json::Value,
    ) -> Result<CaptureAsset> {
        self.import_data_inner(
            rendered_data,
            kind,
            file_extension,
            pixel_width,
            pixel_height,
            duration,
            source_application,
            created_at,
            Some((clean_source, document)),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn import_data_inner(
        &mut self,
        data: &[u8],
        kind: CaptureKind,
        file_extension: &str,
        pixel_width: i64,
        pixel_height: i64,
        duration: Option<f64>,
        source_application: Option<String>,
        created_at: Option<f64>,
        annotation_project: Option<(&[u8], &serde_json::Value)>,
    ) -> Result<CaptureAsset> {
        self.validate_storage_layout()?;
        if annotation_project.is_some() && kind != CaptureKind::Image {
            return Err(AssetLibraryError::UnsupportedAnnotationAsset);
        }
        let safe_extension = Self::validated_extension(file_extension)?;
        let persisted_created_at = Self::normalized_date(created_at.unwrap_or_else(now_ms));
        let asset = Self::make_asset(
            kind,
            &safe_extension,
            pixel_width,
            pixel_height,
            duration,
            source_application,
            persisted_created_at,
        );
        let file_url = self.asset_url(&asset);
        atomic_write(&file_url, data)?;

        if let Some((clean_source, document)) = annotation_project {
            if let Err(error) =
                self.write_new_annotation_project_files(&asset, clean_source, data, document)
            {
                let _ = std::fs::remove_file(&file_url);
                return Err(error);
            }
        }

        self.index.push(asset.clone());
        if let Err(error) = self.persist() {
            self.index.retain(|entry| entry.id != asset.id);
            let _ = self.cleanup_asset_files(&asset);
            return Err(error);
        }
        Ok(asset)
    }

    /// Mirrors `AssetLibrary.importFile(at:...)`.
    #[allow(clippy::too_many_arguments)]
    pub fn import_file(
        &mut self,
        source_url: &Path,
        kind: CaptureKind,
        file_extension: &str,
        pixel_width: i64,
        pixel_height: i64,
        duration: Option<f64>,
        source_application: Option<String>,
    ) -> Result<CaptureAsset> {
        let safe_extension = Self::validated_extension(file_extension)?;
        let asset = Self::make_asset_with_id(
            uuid::Uuid::new_v4(),
            kind,
            &safe_extension,
            pixel_width,
            pixel_height,
            duration,
            source_application,
            Self::normalized_date(now_ms()),
        );
        self.import_prepared_file(source_url, asset)
    }

    /// Imports a finalized recording with a stable id. Repeating the same
    /// operation after a crash returns the already-indexed asset instead of
    /// creating a duplicate.
    #[allow(clippy::too_many_arguments)]
    pub fn import_file_with_stable_id(
        &mut self,
        source_url: &Path,
        id: uuid::Uuid,
        created_at: f64,
        kind: CaptureKind,
        file_extension: &str,
        pixel_width: i64,
        pixel_height: i64,
        duration: Option<f64>,
        source_application: Option<String>,
    ) -> Result<CaptureAsset> {
        if id.is_nil() || !created_at.is_finite() {
            return Err(AssetLibraryError::InvalidFilename);
        }
        let safe_extension = Self::validated_extension(file_extension)?;
        let asset = match self.asset_by_id(&id).cloned() {
            Some(existing)
                if existing.kind == kind
                    && Path::new(&existing.filename)
                        .extension()
                        .and_then(|extension| extension.to_str())
                        .is_some_and(|extension| {
                            extension.eq_ignore_ascii_case(&safe_extension)
                        }) =>
            {
                // Existing Swift and early Rust indexes keep their original
                // timestamp-based names. Stable retries reuse that name.
                existing
            }
            Some(_) => return Err(AssetLibraryError::DuplicateIndexEntry),
            None => Self::make_stable_asset_with_id(
                id,
                kind,
                &safe_extension,
                pixel_width,
                pixel_height,
                duration,
                source_application,
                Self::normalized_date(created_at),
            ),
        };
        self.import_prepared_file(source_url, asset)
    }

    fn import_prepared_file(
        &mut self,
        source_url: &Path,
        asset: CaptureAsset,
    ) -> Result<CaptureAsset> {
        self.validate_storage_layout()?;
        ensure_regular_file(source_url)?;
        if let Some(existing) = self.asset_by_id(&asset.id).cloned() {
            if existing.kind != asset.kind || existing.filename != asset.filename {
                return Err(AssetLibraryError::DuplicateIndexEntry);
            }
            match self.asset_availability(&existing.id)? {
                AssetAvailability::Ready => {
                    if !files_equal(source_url, &self.asset_url(&existing))? {
                        return Err(AssetLibraryError::AssetFileStillPresent);
                    }
                    return Ok(existing);
                }
                AssetAvailability::Missing => {
                    atomic_copy_missing(source_url, &self.asset_url(&existing), None)?;
                    return Ok(existing);
                }
                AssetAvailability::Unreadable => {
                    return Err(AssetLibraryError::Io(std::io::Error::other(
                        "the existing asset file is unreadable",
                    )))
                }
            }
        }
        if self
            .index
            .iter()
            .any(|entry| entry.filename.eq_ignore_ascii_case(&asset.filename))
        {
            return Err(AssetLibraryError::DuplicateIndexEntry);
        }

        let file_url = self.asset_url(&asset);
        let created_file = match std::fs::symlink_metadata(&file_url) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink()
                    || !metadata.is_file()
                    || !files_equal(source_url, &file_url)?
                {
                    return Err(AssetLibraryError::AssetFileStillPresent);
                }
                false
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                copy_new_file(source_url, &file_url)?;
                true
            }
            Err(error) => return Err(AssetLibraryError::Io(error)),
        };

        self.index.push(asset.clone());
        if let Err(error) = self.persist() {
            self.index.retain(|entry| entry.id != asset.id);
            if created_file {
                let _ = std::fs::remove_file(&file_url);
                let _ = sync_directory(&self.assets_url);
            }
            return Err(error);
        }
        Ok(asset)
    }

    #[cfg(test)]
    fn replace_data(&mut self, data: &[u8], id: &uuid::Uuid) -> Result<CaptureAsset> {
        let asset = self
            .index
            .iter()
            .find(|asset| &asset.id == id)
            .ok_or(AssetLibraryError::AssetNotFound)?
            .clone();
        atomic_write(&self.asset_url(&asset), data)?;
        Ok(asset)
    }

    #[cfg(test)]
    fn save_annotation_project(
        &self,
        id: &uuid::Uuid,
        rendered: &[u8],
        document: &serde_json::Value,
    ) -> Result<CaptureAsset> {
        let asset = self.annotation_asset(id)?;
        let asset_url = self.readable_asset_url(&asset)?;
        let previous_rendered = std::fs::read(&asset_url)?;
        let existing = self.read_existing_annotation_project(&asset, &previous_rendered)?;

        match existing {
            Some(existing) => {
                let (document_url, _) = self.annotation_project_urls(&asset);
                let previous_encoded = std::fs::read(&document_url)?;
                let next = StoredAnnotationProject {
                    project_version: ANNOTATION_PROJECT_VERSION,
                    source_sha256: existing.source_sha256,
                    rendered_sha256: sha256_hex(rendered),
                    document: document.clone(),
                };
                let encoded = encode_annotation_project(&next)?;
                atomic_write(&document_url, &encoded)?;
                if let Err(error) = atomic_write(&asset_url, rendered) {
                    best_effort_restore(&document_url, Some(&previous_encoded));
                    best_effort_restore(&asset_url, Some(&previous_rendered));
                    return Err(error);
                }
            }
            None => {
                self.write_new_annotation_project_files(
                    &asset,
                    &previous_rendered,
                    rendered,
                    document,
                )?;
                if let Err(error) = atomic_write(&asset_url, rendered) {
                    let _ = self.cleanup_annotation_project_files(&asset);
                    best_effort_restore(&asset_url, Some(&previous_rendered));
                    return Err(error);
                }
            }
        }

        Ok(asset)
    }

    #[cfg(test)]
    fn load_annotation_project(&self, id: &uuid::Uuid) -> Result<Option<LoadedAnnotationProject>> {
        let asset = self.annotation_asset(id)?;
        let rendered = std::fs::read(self.readable_asset_url(&asset)?)?;
        let (document_url, source_url) = self.annotation_project_urls(&asset);
        Ok(self
            .read_existing_annotation_project(&asset, &rendered)?
            .map(|stored| LoadedAnnotationProject {
                document: stored.document,
                document_url,
                source_url,
            }))
    }

    /// Loads the exact content-addressed baseline used by the screenshot
    /// editor. Invalid projects fail open only to the current flattened image;
    /// their untrusted documents and sources are never applied.
    pub fn load_editor_snapshot(&self, id: &uuid::Uuid) -> Result<LoadedEditorSnapshot> {
        let EditorSnapshotFiles {
            rendered,
            source,
            revision_sha256,
            state,
            valid_project,
            ..
        } = self.read_editor_snapshot_files(id)?;
        let document = valid_project.map(|project| project.document);
        let source = match state {
            EditorAnnotationState::Valid => {
                source.ok_or(AssetLibraryError::IncompleteAnnotationProject)?
            }
            EditorAnnotationState::None | EditorAnnotationState::Invalid => rendered,
        };
        Ok(LoadedEditorSnapshot {
            revision_sha256,
            state,
            document,
            source,
        })
    }

    /// Compare-and-save for editor output. `Some(document)` persists an
    /// editable project; `None` updates the flattened image and removes project
    /// files. The expected revision must be the one returned at editor open.
    pub fn save_editor_snapshot(
        &self,
        id: &uuid::Uuid,
        expected_revision_sha256: &str,
        rendered: &[u8],
        document: Option<&serde_json::Value>,
    ) -> Result<CaptureAsset> {
        let snapshot = self.read_editor_snapshot_files(id)?;
        if !is_sha256_hex(expected_revision_sha256)
            || snapshot.revision_sha256 != expected_revision_sha256
        {
            return Err(AssetLibraryError::AnnotationRevisionMismatch);
        }
        match document {
            Some(document) => self.write_editor_project_snapshot(&snapshot, rendered, document)?,
            None => self.write_editor_flat_snapshot(&snapshot, rendered)?,
        }
        Ok(snapshot.asset)
    }

    #[cfg(test)]
    fn clear_annotation_project(&self, id: &uuid::Uuid) -> Result<()> {
        let asset = self.annotation_asset(id)?;
        let failed_files = self.cleanup_annotation_project_files(&asset);
        if failed_files > 0 {
            return Err(AssetLibraryError::CleanupFailed { failed_files });
        }
        Ok(())
    }

    pub fn set_favorite(&mut self, favorite: bool, id: &uuid::Uuid) -> Result<()> {
        self.update(id, |asset| asset.is_favorite = favorite)
    }

    pub fn set_title(&mut self, title: Option<String>, id: &uuid::Uuid) -> Result<()> {
        self.update(id, |asset| asset.title = title)
    }

    pub fn set_tags(&mut self, tags: Vec<String>, id: &uuid::Uuid) -> Result<()> {
        let mut deduped: Vec<String> = Vec::new();
        for tag in tags {
            let trimmed = tag.trim().to_string();
            if !trimmed.is_empty() && !deduped.iter().any(|t| t.eq_ignore_ascii_case(&trimmed)) {
                deduped.push(trimmed);
            }
        }
        self.update(id, |asset| asset.tags = deduped)
    }

    pub fn move_to_trash(&mut self, id: &uuid::Uuid) -> Result<()> {
        self.update(id, |asset| asset.trashed_at = Some(now_ms()))
    }

    pub fn restore(&mut self, id: &uuid::Uuid) -> Result<()> {
        self.update(id, |asset| asset.trashed_at = None)
    }

    pub fn batch_set_favorite(&mut self, favorite: bool, ids: &[uuid::Uuid]) -> Result<()> {
        self.batch_update(ids, |asset| asset.is_favorite = favorite)
    }

    pub fn batch_move_to_trash(&mut self, ids: &[uuid::Uuid]) -> Result<()> {
        let trashed_at = now_ms();
        self.batch_update(ids, |asset| asset.trashed_at = Some(trashed_at))
    }

    pub fn batch_restore(&mut self, ids: &[uuid::Uuid]) -> Result<()> {
        self.batch_update(ids, |asset| asset.trashed_at = None)
    }

    /// Atomically removes every requested asset from the persisted index, then
    /// cleans up its files. A cleanup error is reported only after the index is
    /// committed; callers must treat `CleanupFailed` as a committed mutation.
    pub fn batch_permanently_delete(&mut self, ids: &[uuid::Uuid]) -> Result<()> {
        let requested = self.validated_batch_ids(ids)?;
        if requested.is_empty() {
            return Ok(());
        }

        let mut removed = Vec::with_capacity(requested.len());
        let mut next_index = Vec::with_capacity(self.index.len().saturating_sub(requested.len()));
        for asset in &self.index {
            if requested.contains(&asset.id) {
                removed.push(asset.clone());
            } else {
                next_index.push(asset.clone());
            }
        }

        self.persist_index(&next_index)?;
        self.index = next_index;

        let failed_files = removed
            .iter()
            .map(|asset| self.cleanup_asset_files(asset))
            .sum();
        if failed_files > 0 {
            return Err(AssetLibraryError::CleanupFailed { failed_files });
        }
        Ok(())
    }

    pub fn permanently_delete(&mut self, id: &uuid::Uuid) -> Result<()> {
        let position = self
            .index
            .iter()
            .position(|asset| &asset.id == id)
            .ok_or(AssetLibraryError::AssetNotFound)?;
        let asset = self.index[position].clone();
        let mut next_index = self.index.clone();
        next_index.remove(position);
        self.persist_index(&next_index)?;
        self.index = next_index;
        let failed_files = self.cleanup_asset_files(&asset);
        if failed_files > 0 {
            return Err(AssetLibraryError::CleanupFailed { failed_files });
        }
        Ok(())
    }

    pub fn empty_trash(&mut self) -> Result<()> {
        let trashed: Vec<CaptureAsset> = self
            .index
            .iter()
            .filter(|asset| asset.trashed_at.is_some())
            .cloned()
            .collect();
        if trashed.is_empty() {
            return Ok(());
        }
        let next_index = self
            .index
            .iter()
            .filter(|asset| asset.trashed_at.is_none())
            .cloned()
            .collect::<Vec<_>>();
        self.persist_index(&next_index)?;
        self.index = next_index;
        let failed_files = trashed
            .iter()
            .map(|asset| self.cleanup_asset_files(asset))
            .sum();
        if failed_files > 0 {
            return Err(AssetLibraryError::CleanupFailed { failed_files });
        }
        Ok(())
    }

    fn update(&mut self, id: &uuid::Uuid, mutation: impl FnOnce(&mut CaptureAsset)) -> Result<()> {
        let position = self
            .index
            .iter()
            .position(|asset| &asset.id == id)
            .ok_or(AssetLibraryError::AssetNotFound)?;
        let previous = self.index[position].clone();
        mutation(&mut self.index[position]);
        if let Err(error) = self.persist() {
            self.index[position] = previous;
            return Err(error);
        }
        Ok(())
    }

    fn batch_update(
        &mut self,
        ids: &[uuid::Uuid],
        mut mutation: impl FnMut(&mut CaptureAsset),
    ) -> Result<()> {
        let requested = self.validated_batch_ids(ids)?;
        if requested.is_empty() {
            return Ok(());
        }

        let mut next_index = self.index.clone();
        for asset in &mut next_index {
            if requested.contains(&asset.id) {
                mutation(asset);
            }
        }
        self.persist_index(&next_index)?;
        self.index = next_index;
        Ok(())
    }

    fn validated_batch_ids(&self, ids: &[uuid::Uuid]) -> Result<HashSet<uuid::Uuid>> {
        let requested = ids.iter().copied().collect::<HashSet<_>>();
        if requested.iter().any(|id| self.asset_by_id(id).is_none()) {
            return Err(AssetLibraryError::AssetNotFound);
        }
        Ok(requested)
    }

    fn annotation_asset(&self, id: &uuid::Uuid) -> Result<CaptureAsset> {
        let asset = self
            .asset_by_id(id)
            .cloned()
            .ok_or(AssetLibraryError::AssetNotFound)?;
        if asset.kind != CaptureKind::Image {
            return Err(AssetLibraryError::UnsupportedAnnotationAsset);
        }
        Ok(asset)
    }

    fn annotation_project_urls(&self, asset: &CaptureAsset) -> (PathBuf, PathBuf) {
        let id = asset.id.to_string().to_lowercase();
        (
            self.annotations_url.join(format!("{id}.json")),
            self.annotations_url.join(format!("{id}.source.png")),
        )
    }

    #[cfg(test)]
    fn read_existing_annotation_project(
        &self,
        asset: &CaptureAsset,
        rendered: &[u8],
    ) -> Result<Option<StoredAnnotationProject>> {
        let (document_url, source_url) = self.annotation_project_urls(asset);
        let encoded = read_optional_file(&document_url)?;
        let source = read_optional_file(&source_url)?;
        validate_annotation_project_snapshot(encoded.as_deref(), source.as_deref(), rendered, None)
    }

    fn read_editor_snapshot_files(&self, id: &uuid::Uuid) -> Result<EditorSnapshotFiles> {
        let asset = self.annotation_asset(id)?;
        let rendered = std::fs::read(self.readable_asset_url(&asset)?)?;
        let (document_url, source_url) = self.annotation_project_urls(&asset);
        let encoded = read_optional_file(&document_url)?;
        let source = read_optional_file(&source_url)?;
        let validation = validate_annotation_project_snapshot(
            encoded.as_deref(),
            source.as_deref(),
            &rendered,
            Some((asset.pixel_width, asset.pixel_height)),
        );
        let (state, valid_project) = match validation {
            Ok(Some(stored)) => (EditorAnnotationState::Valid, Some(stored)),
            Ok(None) => (EditorAnnotationState::None, None),
            Err(
                AssetLibraryError::IncompleteAnnotationProject
                | AssetLibraryError::CorruptAnnotationProject(_)
                | AssetLibraryError::StaleAnnotationProject,
            ) => (EditorAnnotationState::Invalid, None),
            Err(error) => return Err(error),
        };
        let revision_sha256 =
            editor_revision_sha256(state, &rendered, encoded.as_deref(), source.as_deref());
        Ok(EditorSnapshotFiles {
            asset,
            rendered,
            encoded,
            source,
            document_url,
            source_url,
            revision_sha256,
            state,
            valid_project,
        })
    }

    fn write_editor_project_snapshot(
        &self,
        snapshot: &EditorSnapshotFiles,
        rendered: &[u8],
        document: &serde_json::Value,
    ) -> Result<()> {
        let clean_source = match snapshot.state {
            EditorAnnotationState::Valid => snapshot
                .source
                .as_deref()
                .ok_or(AssetLibraryError::IncompleteAnnotationProject)?,
            EditorAnnotationState::None | EditorAnnotationState::Invalid => &snapshot.rendered,
        };
        let stored = StoredAnnotationProject {
            project_version: ANNOTATION_PROJECT_VERSION,
            source_sha256: sha256_hex(clean_source),
            rendered_sha256: sha256_hex(rendered),
            document: document.clone(),
        };
        let encoded = encode_annotation_project(&stored)?;
        let asset_url = self.asset_url(&snapshot.asset);
        let writes = match snapshot.state {
            EditorAnnotationState::Valid => vec![
                (snapshot.document_url.as_path(), encoded.as_slice()),
                (asset_url.as_path(), rendered),
            ],
            EditorAnnotationState::None | EditorAnnotationState::Invalid => vec![
                (snapshot.source_url.as_path(), clean_source),
                (snapshot.document_url.as_path(), encoded.as_slice()),
                (asset_url.as_path(), rendered),
            ],
        };
        for (index, (path, bytes)) in writes.into_iter().enumerate() {
            if let Err(error) = self.write_annotation_replacement(index + 1, path, bytes) {
                restore_editor_snapshot(snapshot, &asset_url);
                return Err(error);
            }
        }
        Ok(())
    }

    fn write_editor_flat_snapshot(
        &self,
        snapshot: &EditorSnapshotFiles,
        rendered: &[u8],
    ) -> Result<()> {
        let asset_url = self.asset_url(&snapshot.asset);
        if let Err(error) = self.write_annotation_replacement(1, &asset_url, rendered) {
            restore_editor_snapshot(snapshot, &asset_url);
            return Err(error);
        }
        for path in [&snapshot.document_url, &snapshot.source_url] {
            match std::fs::remove_file(path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    restore_editor_snapshot(snapshot, &asset_url);
                    return Err(AssetLibraryError::Io(error));
                }
            }
        }
        Ok(())
    }

    fn write_annotation_replacement(
        &self,
        write_index: usize,
        path: &Path,
        data: &[u8],
    ) -> Result<()> {
        #[cfg(test)]
        if self.annotation_replace_fail_at_write.get() == Some(write_index) {
            return Err(AssetLibraryError::Io(std::io::Error::other(
                "injected annotation replacement failure",
            )));
        }
        #[cfg(not(test))]
        let _ = write_index;
        atomic_write(path, data)
    }

    fn write_new_annotation_project_files(
        &self,
        asset: &CaptureAsset,
        clean_source: &[u8],
        rendered: &[u8],
        document: &serde_json::Value,
    ) -> Result<()> {
        let (document_url, source_url) = self.annotation_project_urls(asset);
        if document_url.exists() || source_url.exists() {
            return Err(AssetLibraryError::AnnotationProjectAlreadyExists);
        }

        atomic_write(&source_url, clean_source)?;
        let stored = StoredAnnotationProject {
            project_version: ANNOTATION_PROJECT_VERSION,
            source_sha256: sha256_hex(clean_source),
            rendered_sha256: sha256_hex(rendered),
            document: document.clone(),
        };
        let encoded = match encode_annotation_project(&stored) {
            Ok(encoded) => encoded,
            Err(error) => {
                let _ = std::fs::remove_file(&source_url);
                return Err(error);
            }
        };
        if let Err(error) = atomic_write(&document_url, &encoded) {
            let _ = std::fs::remove_file(&source_url);
            return Err(error);
        }
        Ok(())
    }

    #[cfg(test)]
    fn cleanup_annotation_project_files(&self, asset: &CaptureAsset) -> usize {
        let (document_url, source_url) = self.annotation_project_urls(asset);
        remove_files([document_url, source_url])
    }

    fn cleanup_asset_files(&self, asset: &CaptureAsset) -> usize {
        let (annotation_document, annotation_source) = self.annotation_project_urls(asset);
        remove_files([
            self.asset_url(asset),
            self.thumbnails_url
                .join(format!("{}.jpg", asset.id.to_string().to_lowercase())),
            annotation_document,
            annotation_source,
        ])
    }

    fn persist(&self) -> Result<()> {
        self.persist_index(&self.index)
    }

    fn persist_index(&self, index: &[CaptureAsset]) -> Result<()> {
        #[cfg(test)]
        self.persist_count.set(self.persist_count.get() + 1);
        self.validate_storage_layout()?;
        let data = serde_json::to_vec_pretty(index).map_err(AssetLibraryError::CorruptIndex)?;
        atomic_write(&self.index_url, &data)
    }

    fn validated_extension(file_extension: &str) -> Result<String> {
        let safe: String = file_extension
            .trim_matches('.')
            .to_lowercase()
            .chars()
            .collect();
        if safe.is_empty() || !safe.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(AssetLibraryError::InvalidFilename);
        }
        Ok(safe)
    }

    /// Mirrors the Swift millisecond normalization used by AssetLibrary.
    fn normalized_date(value: f64) -> f64 {
        (value * 1000.0).round() / 1000.0
    }

    fn make_asset(
        kind: CaptureKind,
        file_extension: &str,
        pixel_width: i64,
        pixel_height: i64,
        duration: Option<f64>,
        source_application: Option<String>,
        created_at: f64,
    ) -> CaptureAsset {
        Self::make_asset_with_id(
            uuid::Uuid::new_v4(),
            kind,
            file_extension,
            pixel_width,
            pixel_height,
            duration,
            source_application,
            created_at,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn make_asset_with_id(
        id: uuid::Uuid,
        kind: CaptureKind,
        file_extension: &str,
        pixel_width: i64,
        pixel_height: i64,
        duration: Option<f64>,
        source_application: Option<String>,
        created_at: f64,
    ) -> CaptureAsset {
        // DateFormatter with en_US_POSIX locale and local timezone.
        let stamp = Local
            .timestamp_millis_opt(created_at as i64)
            .single()
            .map(|t| t.format("%Y%m%d-%H%M%S").to_string())
            .unwrap_or_else(|| "19700101-000000".to_string());
        let filename = format!("{stamp}-{}.{file_extension}", id.to_string().to_lowercase());
        CaptureAsset {
            id,
            kind,
            created_at,
            filename,
            title: None,
            tags: Vec::new(),
            pixel_width,
            pixel_height,
            duration,
            source_application,
            is_favorite: false,
            trashed_at: None,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn make_stable_asset_with_id(
        id: uuid::Uuid,
        kind: CaptureKind,
        file_extension: &str,
        pixel_width: i64,
        pixel_height: i64,
        duration: Option<f64>,
        source_application: Option<String>,
        created_at: f64,
    ) -> CaptureAsset {
        CaptureAsset {
            id,
            kind,
            created_at,
            filename: format!("{}.{file_extension}", id.simple()),
            title: None,
            tags: Vec::new(),
            pixel_width,
            pixel_height,
            duration,
            source_application,
            is_favorite: false,
            trashed_at: None,
        }
    }
}

fn ensure_or_create_directory(path: &Path) -> Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => ensure_directory(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            std::fs::create_dir(path)?;
            ensure_directory(path)
        }
        Err(error) => Err(AssetLibraryError::Io(error)),
    }
}

fn ensure_directory(path: &Path) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(AssetLibraryError::Io(std::io::Error::other(
            "library path is not a regular directory",
        )));
    }
    Ok(())
}

fn ensure_regular_file(path: &Path) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(AssetLibraryError::Io(std::io::Error::other(
            "library path is not a regular file",
        )));
    }
    Ok(())
}

pub(crate) fn is_safe_library_filename(filename: &str) -> bool {
    if filename.is_empty() || filename == "." || filename == ".." {
        return false;
    }
    if filename.contains('/') || filename.contains('\\') || filename.contains('\0') {
        return false;
    }
    let mut components = Path::new(filename).components();
    matches!(components.next(), Some(std::path::Component::Normal(_)))
        && components.next().is_none()
}

fn validate_index(index: &[CaptureAsset]) -> Result<()> {
    let mut ids = HashSet::with_capacity(index.len());
    let mut filenames = HashSet::with_capacity(index.len());
    for asset in index {
        if !is_safe_library_filename(&asset.filename) {
            return Err(AssetLibraryError::InvalidFilename);
        }
        if !ids.insert(asset.id) || !filenames.insert(asset.filename.to_lowercase()) {
            return Err(AssetLibraryError::DuplicateIndexEntry);
        }
    }
    Ok(())
}

fn asset_file_availability(path: &Path) -> AssetAvailability {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return AssetAvailability::Missing
        }
        Err(_) => return AssetAvailability::Unreadable,
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return AssetAvailability::Unreadable;
    }
    match std::fs::File::open(path) {
        Ok(_) => AssetAvailability::Ready,
        Err(_) => AssetAvailability::Unreadable,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReplacementFileProof {
    kind: CaptureKind,
    byte_len: u64,
    sha256: [u8; 32],
}

pub(crate) fn replacement_file_proof(
    path: &Path,
    kind: CaptureKind,
) -> Result<ReplacementFileProof> {
    ensure_regular_file(path)?;
    let expected_extension = match kind {
        CaptureKind::Image => "png",
        CaptureKind::Video => "mp4",
        CaptureKind::Gif => "gif",
    };
    if !path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case(expected_extension))
    {
        return Err(AssetLibraryError::InvalidReplacementFile);
    }
    let mut prefix = [0_u8; 12];
    let mut source = std::fs::File::open(path)?;
    let read = std::io::Read::read(&mut source, &mut prefix)?;
    let matches_kind = match kind {
        CaptureKind::Image => read >= 8 && prefix[..8] == *b"\x89PNG\r\n\x1a\n",
        CaptureKind::Video => read >= 8 && prefix[4..8] == *b"ftyp",
        CaptureKind::Gif => read >= 6 && (prefix[..6] == *b"GIF87a" || prefix[..6] == *b"GIF89a"),
    };
    if !matches_kind {
        return Err(AssetLibraryError::InvalidReplacementFile);
    }
    let metadata = std::fs::symlink_metadata(path)?;
    Ok(ReplacementFileProof {
        kind,
        byte_len: metadata.len(),
        sha256: sha256_file(path)?,
    })
}

fn atomic_copy_missing(
    source: &Path,
    destination: &Path,
    expected: Option<&ReplacementFileProof>,
) -> Result<()> {
    if asset_file_availability(destination) != AssetAvailability::Missing {
        return Err(AssetLibraryError::AssetFileStillPresent);
    }
    let parent = destination
        .parent()
        .ok_or(AssetLibraryError::InvalidFilename)?;
    let staged = stage_verified_copy(source, parent, "restore", expected)?;
    install_staged_new_file(staged, destination)
}

fn copy_new_file(source: &Path, destination: &Path) -> Result<()> {
    let parent = destination
        .parent()
        .ok_or(AssetLibraryError::InvalidFilename)?;
    let staged = stage_verified_copy(source, parent, "import", None)?;
    install_staged_new_file(staged, destination)
}

fn stage_verified_copy(
    source: &Path,
    parent: &Path,
    label: &str,
    expected: Option<&ReplacementFileProof>,
) -> Result<tempfile::NamedTempFile> {
    ensure_regular_file(source)?;
    let source_size = std::fs::symlink_metadata(source)?.len();
    let source_hash = sha256_file(source)?;
    if expected.is_some_and(|proof| proof.byte_len != source_size || proof.sha256 != source_hash) {
        return Err(AssetLibraryError::InvalidReplacementFile);
    }
    let mut staged = tempfile::Builder::new()
        .prefix(&format!(".{label}-"))
        .suffix(".tmp")
        .tempfile_in(parent)?;
    let mut input = std::fs::File::open(source)?;
    let copied = std::io::copy(&mut input, staged.as_file_mut())?;
    std::io::Write::flush(staged.as_file_mut())?;
    staged.as_file().sync_all()?;
    if copied != source_size {
        return Err(AssetLibraryError::Io(std::io::Error::other(
            "source file changed while copying",
        )));
    }
    if std::fs::symlink_metadata(staged.path())?.len() != source_size
        || sha256_file(staged.path())? != source_hash
    {
        return Err(AssetLibraryError::Io(std::io::Error::other(
            "source file changed while copying",
        )));
    }
    Ok(staged)
}

fn install_staged_new_file(staged: tempfile::NamedTempFile, destination: &Path) -> Result<()> {
    let parent = destination
        .parent()
        .ok_or(AssetLibraryError::InvalidFilename)?;
    match staged.persist_noclobber(destination) {
        Ok(_) => {
            sync_directory_after_commit(parent, "asset file install");
            Ok(())
        }
        Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
            Err(AssetLibraryError::AssetFileStillPresent)
        }
        Err(error) => Err(AssetLibraryError::Io(error.error)),
    }
}

fn files_equal(left: &Path, right: &Path) -> Result<bool> {
    let left_metadata = std::fs::symlink_metadata(left)?;
    let right_metadata = std::fs::symlink_metadata(right)?;
    if !left_metadata.is_file()
        || !right_metadata.is_file()
        || left_metadata.len() != right_metadata.len()
    {
        return Ok(false);
    }
    Ok(sha256_file(left)? == sha256_file(right)?)
}

fn sha256_file(path: &Path) -> Result<[u8; 32]> {
    let mut file = std::fs::File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = std::io::Read::read(&mut file, &mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(digest.finalize().into())
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<()> {
    std::fs::File::open(path)?.sync_all()?;
    Ok(())
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> Result<()> {
    Ok(())
}

fn now_ms() -> f64 {
    chrono::Utc::now().timestamp_millis() as f64
}

fn encode_annotation_project(project: &StoredAnnotationProject) -> Result<Vec<u8>> {
    serde_json::to_vec_pretty(project)
        .map_err(|error| AssetLibraryError::CorruptAnnotationProject(error.to_string()))
}

fn editor_revision_sha256(
    state: EditorAnnotationState,
    rendered: &[u8],
    encoded: Option<&[u8]>,
    source: Option<&[u8]>,
) -> String {
    let mut digest = Sha256::new();
    digest.update(b"kiri-editor-snapshot-v1\0");
    digest.update([match state {
        EditorAnnotationState::None => 0,
        EditorAnnotationState::Valid => 1,
        EditorAnnotationState::Invalid => 2,
    }]);
    update_revision_part(&mut digest, b"rendered", Some(rendered));
    update_revision_part(&mut digest, b"document", encoded);
    update_revision_part(&mut digest, b"source", source);
    format!("{:x}", digest.finalize())
}

fn update_revision_part(digest: &mut Sha256, label: &[u8], bytes: Option<&[u8]>) {
    digest.update((label.len() as u64).to_be_bytes());
    digest.update(label);
    match bytes {
        Some(bytes) => {
            digest.update([1]);
            digest.update((bytes.len() as u64).to_be_bytes());
            digest.update(bytes);
        }
        None => digest.update([0]),
    }
}

fn restore_editor_snapshot(snapshot: &EditorSnapshotFiles, asset_url: &Path) {
    best_effort_restore(&snapshot.source_url, snapshot.source.as_deref());
    best_effort_restore(&snapshot.document_url, snapshot.encoded.as_deref());
    best_effort_restore(asset_url, Some(&snapshot.rendered));
}

fn read_optional_file(path: &Path) -> Result<Option<Vec<u8>>> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err(AssetLibraryError::Io(std::io::Error::other(
                "annotation path is not a regular file",
            )))
        }
        Ok(_) => std::fs::read(path).map(Some).map_err(AssetLibraryError::Io),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(AssetLibraryError::Io(error)),
    }
}

fn validate_annotation_project_snapshot(
    encoded: Option<&[u8]>,
    source: Option<&[u8]>,
    rendered: &[u8],
    expected_image_pixels: Option<(i64, i64)>,
) -> Result<Option<StoredAnnotationProject>> {
    let (encoded, source) = match (encoded, source) {
        (None, None) => return Ok(None),
        (Some(encoded), Some(source)) => (encoded, source),
        _ => return Err(AssetLibraryError::IncompleteAnnotationProject),
    };
    let stored: StoredAnnotationProject = serde_json::from_slice(encoded)
        .map_err(|error| AssetLibraryError::CorruptAnnotationProject(error.to_string()))?;
    if stored.project_version != ANNOTATION_PROJECT_VERSION
        || !is_sha256_hex(&stored.source_sha256)
        || !is_sha256_hex(&stored.rendered_sha256)
    {
        return Err(AssetLibraryError::CorruptAnnotationProject(
            "unsupported project wrapper".into(),
        ));
    }
    if sha256_hex(source) != stored.source_sha256 || sha256_hex(rendered) != stored.rendered_sha256
    {
        return Err(AssetLibraryError::StaleAnnotationProject);
    }
    let document_json = serde_json::to_string(&stored.document)
        .map_err(|error| AssetLibraryError::CorruptAnnotationProject(error.to_string()))?;
    let document = AnnotationDocument::from_json(&document_json)
        .map_err(AssetLibraryError::CorruptAnnotationProject)?;
    if let Some((width, height)) = expected_image_pixels {
        document
            .validate_for_image_pixels(width, height)
            .map_err(AssetLibraryError::CorruptAnnotationProject)?;
    }

    Ok(Some(stored))
}

fn sha256_hex(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn remove_files<const N: usize>(paths: [PathBuf; N]) -> usize {
    paths
        .iter()
        .filter(|path| match std::fs::remove_file(path) {
            Ok(()) => false,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(error) => {
                log::error!("could not remove asset library file: {error}");
                true
            }
        })
        .count()
}

fn best_effort_restore(path: &Path, previous: Option<&[u8]>) {
    let result = match previous {
        Some(bytes) => atomic_write(path, bytes),
        None => match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(AssetLibraryError::Io(error)),
        },
    };
    if let Err(error) = result {
        log::error!("could not roll back asset library write: {error}");
    }
}

/// Write via a fully-synced temporary file and rename it into place. The
/// parent directory is synced after the namespace change where supported.
fn atomic_write(path: &Path, data: &[u8]) -> Result<()> {
    let parent = path.parent().ok_or(AssetLibraryError::InvalidFilename)?;
    let mut temporary = tempfile::Builder::new()
        .prefix(".library-write-")
        .suffix(".tmp")
        .tempfile_in(parent)?;
    std::io::Write::write_all(temporary.as_file_mut(), data)?;
    std::io::Write::flush(temporary.as_file_mut())?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(path)
        .map_err(|error| AssetLibraryError::Io(error.error))?;
    sync_directory_after_commit(parent, "atomic write");
    Ok(())
}

fn sync_directory_after_commit(path: &Path, operation: &str) {
    if let Err(error) = sync_directory(path) {
        log::warn!("{operation} committed but its directory could not be synced: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_path_buf();
        (dir, path)
    }

    fn import_test_assets(library: &mut AssetLibrary, count: usize) -> Vec<CaptureAsset> {
        (0..count)
            .map(|index| {
                library
                    .import_data(
                        format!("asset-{index}").as_bytes(),
                        CaptureKind::Image,
                        "png",
                        1,
                        1,
                        None,
                        None,
                        None,
                    )
                    .unwrap()
            })
            .collect()
    }

    fn annotation_document(label: &str) -> serde_json::Value {
        serde_json::json!({
            "schemaVersion": 1,
            "canvas": { "width": 100, "height": 80 },
            "sourcePixels": { "width": 100, "height": 80 },
            "marks": [{
                "kind": "text",
                "id": 1,
                "text": label,
                "rect": { "x": 1, "y": 1, "width": 40, "height": 20 },
                "color": "white",
                "background": "transparent",
                "fontSize": 14
            }]
        })
    }

    fn save_test_project(library: &AssetLibrary, asset: &CaptureAsset, label: &str) {
        let rendered = std::fs::read(library.asset_url(asset)).unwrap();
        library
            .save_annotation_project(&asset.id, &rendered, &annotation_document(label))
            .unwrap();
    }

    #[test]
    fn imports_and_lists_assets() {
        let (_dir, root) = temp_root();
        let mut library = AssetLibrary::open(root).unwrap();
        let asset = library
            .import_data(
                b"png-bytes",
                CaptureKind::Image,
                "png",
                100,
                200,
                None,
                Some("Safari".into()),
                Some(1_700_000_000_123.0),
            )
            .unwrap();

        assert!(asset.filename.ends_with(".png"));
        assert!(asset.filename.starts_with("2023"));
        assert_eq!(library.all_assets(false).len(), 1);
        assert!(library.asset_url(&asset).exists());
    }

    #[test]
    fn stable_recording_import_is_idempotent_across_retries() {
        let (directory, root) = temp_root();
        let source = directory.path().join("recovered.mp4");
        std::fs::write(&source, b"recovered recording").unwrap();
        let id = uuid::Uuid::new_v4();
        let created_at = 1_700_000_000_123.0;
        let mut library = AssetLibrary::open(root.clone()).unwrap();

        let first = library
            .import_file_with_stable_id(
                &source,
                id,
                created_at,
                CaptureKind::Video,
                "mp4",
                1920,
                1080,
                Some(3.0),
                None,
            )
            .unwrap();
        let second = library
            .import_file_with_stable_id(
                &source,
                id,
                created_at,
                CaptureKind::Video,
                "mp4",
                1920,
                1080,
                Some(3.0),
                None,
            )
            .unwrap();

        assert_eq!(first, second);
        assert_eq!(first.filename, format!("{}.mp4", id.simple()));
        assert_eq!(library.all_assets(false).len(), 1);
        drop(library);

        let mut reopened = AssetLibrary::open(root).unwrap();
        let third = reopened
            .import_file_with_stable_id(
                &source,
                id,
                created_at,
                CaptureKind::Video,
                "mp4",
                1920,
                1080,
                Some(3.0),
                None,
            )
            .unwrap();
        assert_eq!(third.id, id);
        assert_eq!(reopened.all_assets(false).len(), 1);
    }

    #[test]
    fn stable_import_recovers_a_synced_file_left_before_index_persist() {
        let (directory, root) = temp_root();
        let source = directory.path().join("recovered.mp4");
        std::fs::write(&source, b"recovered recording").unwrap();
        let id = uuid::Uuid::new_v4();
        let created_at = 1_700_000_000_123.0;
        let mut library = AssetLibrary::open(root).unwrap();
        let prepared = AssetLibrary::make_stable_asset_with_id(
            id,
            CaptureKind::Video,
            "mp4",
            1920,
            1080,
            Some(3.0),
            None,
            created_at,
        );
        copy_new_file(&source, &library.asset_url(&prepared)).unwrap();

        let imported = library
            .import_file_with_stable_id(
                &source,
                id,
                created_at,
                CaptureKind::Video,
                "mp4",
                1920,
                1080,
                Some(3.0),
                None,
            )
            .unwrap();

        assert_eq!(imported.id, id);
        assert_eq!(library.all_assets(false), vec![imported]);
    }

    #[test]
    fn stable_import_reuses_a_legacy_index_filename() {
        let (directory, root) = temp_root();
        let source = directory.path().join("recovered.mp4");
        std::fs::write(&source, b"recovered recording").unwrap();
        let id = uuid::Uuid::new_v4();
        let created_at = 1_700_000_000_123.0;
        let mut library = AssetLibrary::open(root).unwrap();
        let legacy = AssetLibrary::make_asset_with_id(
            id,
            CaptureKind::Video,
            "mp4",
            1920,
            1080,
            Some(3.0),
            None,
            created_at,
        );
        library
            .import_prepared_file(&source, legacy.clone())
            .unwrap();

        let retried = library
            .import_file_with_stable_id(
                &source,
                id,
                created_at,
                CaptureKind::Video,
                "mp4",
                1920,
                1080,
                Some(3.0),
                None,
            )
            .unwrap();

        assert_eq!(retried, legacy);
        assert_ne!(retried.filename, format!("{}.mp4", id.simple()));
        assert_eq!(library.all_assets(false).len(), 1);
    }

    #[test]
    fn stable_import_rejects_ready_content_that_does_not_match_the_source() {
        let (directory, root) = temp_root();
        let source = directory.path().join("recovered.mp4");
        std::fs::write(&source, b"first recording").unwrap();
        let id = uuid::Uuid::new_v4();
        let mut library = AssetLibrary::open(root).unwrap();
        let asset = library
            .import_file_with_stable_id(
                &source,
                id,
                1_700_000_000_123.0,
                CaptureKind::Video,
                "mp4",
                1920,
                1080,
                Some(3.0),
                None,
            )
            .unwrap();
        std::fs::write(&source, b"other recording").unwrap();

        assert!(matches!(
            library.import_file_with_stable_id(
                &source,
                id,
                1_700_000_000_123.0,
                CaptureKind::Video,
                "mp4",
                1920,
                1080,
                Some(3.0),
                None,
            ),
            Err(AssetLibraryError::AssetFileStillPresent)
        ));
        assert_eq!(
            std::fs::read(library.asset_url(&asset)).unwrap(),
            b"first recording"
        );
        assert_eq!(std::fs::read(&source).unwrap(), b"other recording");
    }

    #[test]
    fn exclusive_copies_preserve_an_existing_destination_and_the_source() {
        let (directory, root) = temp_root();
        let source = directory.path().join("source.mp4");
        let destination = root.join("destination.mp4");
        std::fs::write(&source, b"source").unwrap();
        std::fs::write(&destination, b"winner").unwrap();

        assert!(matches!(
            copy_new_file(&source, &destination),
            Err(AssetLibraryError::AssetFileStillPresent)
        ));
        assert!(matches!(
            atomic_copy_missing(&source, &destination, None),
            Err(AssetLibraryError::AssetFileStillPresent)
        ));
        assert_eq!(std::fs::read(&destination).unwrap(), b"winner");
        assert_eq!(std::fs::read(&source).unwrap(), b"source");
    }

    #[test]
    fn rejects_unsafe_and_duplicate_index_entries() {
        let (_dir, root) = temp_root();
        let library = AssetLibrary::open(root.clone()).unwrap();
        let first = AssetLibrary::make_asset(
            CaptureKind::Image,
            "png",
            1,
            1,
            None,
            None,
            1_700_000_000_000.0,
        );

        let mut unsafe_asset = first.clone();
        unsafe_asset.filename = "../outside.png".into();
        std::fs::write(
            root.join("library.json"),
            serde_json::to_vec(&vec![unsafe_asset]).unwrap(),
        )
        .unwrap();
        assert!(matches!(
            AssetLibrary::open(root.clone()),
            Err(AssetLibraryError::InvalidFilename)
        ));

        let mut duplicate = first.clone();
        duplicate.id = uuid::Uuid::new_v4();
        std::fs::write(
            root.join("library.json"),
            serde_json::to_vec(&vec![first, duplicate]).unwrap(),
        )
        .unwrap();
        assert!(matches!(
            AssetLibrary::open(root),
            Err(AssetLibraryError::DuplicateIndexEntry)
        ));
        drop(library);
    }

    #[test]
    fn restores_only_a_still_missing_asset_and_preserves_metadata() {
        let (_dir, root) = temp_root();
        let mut library = AssetLibrary::open(root.clone()).unwrap();
        let asset = library
            .import_data(
                b"\x89PNG\r\n\x1a\nold",
                CaptureKind::Image,
                "png",
                10,
                20,
                None,
                Some("Safari".into()),
                None,
            )
            .unwrap();
        std::fs::remove_file(library.asset_url(&asset)).unwrap();
        assert_eq!(
            library.asset_availability(&asset.id).unwrap(),
            AssetAvailability::Missing
        );
        let replacement = root.join("replacement.png");
        std::fs::write(&replacement, b"\x89PNG\r\n\x1a\nrestored").unwrap();
        let proof = replacement_file_proof(&replacement, CaptureKind::Image).unwrap();

        let restored = library
            .restore_missing_asset(&asset.id, &replacement, &proof)
            .unwrap();
        assert_eq!(restored, asset);
        assert_eq!(
            std::fs::read(library.asset_url(&asset)).unwrap(),
            b"\x89PNG\r\n\x1a\nrestored"
        );
        assert!(matches!(
            library.restore_missing_asset(&asset.id, &replacement, &proof),
            Err(AssetLibraryError::AssetFileStillPresent)
        ));
        assert!(matches!(
            library.remove_missing_asset(&asset.id),
            Err(AssetLibraryError::AssetFileStillPresent)
        ));
    }

    #[test]
    fn guarded_remove_succeeds_only_for_a_missing_asset() {
        let (_dir, root) = temp_root();
        let mut library = AssetLibrary::open(root).unwrap();
        let asset = import_test_assets(&mut library, 1).remove(0);
        std::fs::remove_file(library.asset_url(&asset)).unwrap();

        library.remove_missing_asset(&asset.id).unwrap();
        assert!(library.asset_by_id(&asset.id).is_none());
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_asset_is_unreadable_and_never_followed() {
        use std::os::unix::fs::symlink;

        let (_dir, root) = temp_root();
        let mut library = AssetLibrary::open(root.clone()).unwrap();
        let asset = import_test_assets(&mut library, 1).remove(0);
        let asset_path = library.asset_url(&asset);
        std::fs::remove_file(&asset_path).unwrap();
        let outside = root.join("outside.png");
        std::fs::write(&outside, b"outside").unwrap();
        symlink(&outside, &asset_path).unwrap();

        assert_eq!(
            library.asset_availability(&asset.id).unwrap(),
            AssetAvailability::Unreadable
        );
        assert!(library.readable_asset_url(&asset).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn storage_layout_rejects_a_symlinked_managed_directory() {
        use std::os::unix::fs::symlink;

        let (_dir, root) = temp_root();
        let library = AssetLibrary::open(root.clone()).unwrap();
        let assets = root.join("Assets");
        std::fs::remove_dir(&assets).unwrap();
        let outside = root.join("outside-assets");
        std::fs::create_dir(&outside).unwrap();
        symlink(&outside, &assets).unwrap();

        assert!(matches!(
            library.validate_storage_layout(),
            Err(AssetLibraryError::Io(_))
        ));
    }

    #[cfg(unix)]
    #[test]
    fn annotation_reads_reject_symlinked_optional_files() {
        use std::os::unix::fs::symlink;

        let (_dir, root) = temp_root();
        let mut library = AssetLibrary::open(root.clone()).unwrap();
        let asset = library
            .import_data_with_annotation_project(
                b"flattened",
                CaptureKind::Image,
                "png",
                100,
                80,
                None,
                None,
                None,
                b"clean-source",
                &annotation_document("capture"),
            )
            .unwrap();
        let project = library.load_annotation_project(&asset.id).unwrap().unwrap();
        let outside = root.join("outside.json");
        std::fs::write(&outside, std::fs::read(&project.document_url).unwrap()).unwrap();
        std::fs::remove_file(&project.document_url).unwrap();
        symlink(&outside, &project.document_url).unwrap();

        assert!(matches!(
            library.load_editor_snapshot(&asset.id),
            Err(AssetLibraryError::Io(_))
        ));
    }

    #[test]
    fn atomic_write_replaces_existing_content_without_temporary_leaks() {
        let (_dir, root) = temp_root();
        let path = root.join("library.json");
        std::fs::write(&path, b"old").unwrap();

        atomic_write(&path, b"new").unwrap();

        assert_eq!(std::fs::read(&path).unwrap(), b"new");
        assert!(std::fs::read_dir(&root).unwrap().all(|entry| {
            let name = entry.unwrap().file_name();
            let name = name.to_string_lossy();
            !name.ends_with(".tmp") && !name.ends_with(".backup")
        }));
    }

    #[test]
    fn atomic_write_cleans_its_temporary_file_when_replacement_fails() {
        let (_dir, root) = temp_root();
        let path = root.join("blocked");
        std::fs::create_dir(&path).unwrap();

        assert!(matches!(
            atomic_write(&path, b"must not replace the directory"),
            Err(AssetLibraryError::Io(_))
        ));
        assert!(path.is_dir());
        assert!(std::fs::read_dir(&root).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".blocked.")
        }));
    }

    #[test]
    fn annotated_import_keeps_the_swift_index_shape_and_loads_valid_project() {
        let (_dir, root) = temp_root();
        let mut library = AssetLibrary::open(root.clone()).unwrap();
        assert!(root.join("Annotations").is_dir());
        let document = annotation_document("capture");
        let asset = library
            .import_data_with_annotation_project(
                b"flattened-capture",
                CaptureKind::Image,
                "png",
                100,
                80,
                None,
                Some("Safari".into()),
                Some(1_700_000_000_123.0),
                b"clean-capture",
                &document,
            )
            .unwrap();

        let loaded = library.load_annotation_project(&asset.id).unwrap().unwrap();
        assert_eq!(loaded.document, document);
        assert_eq!(std::fs::read(&loaded.source_url).unwrap(), b"clean-capture");
        assert_eq!(
            loaded.document_url.file_name().unwrap().to_str().unwrap(),
            format!("{}.json", asset.id.to_string().to_lowercase())
        );
        assert_eq!(
            loaded.source_url.file_name().unwrap().to_str().unwrap(),
            format!("{}.source.png", asset.id.to_string().to_lowercase())
        );
        assert_eq!(
            std::fs::read(library.asset_url(&asset)).unwrap(),
            b"flattened-capture"
        );

        let index: serde_json::Value =
            serde_json::from_slice(&std::fs::read(root.join("library.json")).unwrap()).unwrap();
        let entry = index.as_array().unwrap()[0].as_object().unwrap();
        for forbidden in [
            "annotationProject",
            "sourceSha256",
            "renderedSha256",
            "document",
        ] {
            assert!(entry.get(forbidden).is_none());
        }

        let reopened = AssetLibrary::open(root).unwrap();
        assert_eq!(
            reopened
                .load_annotation_project(&asset.id)
                .unwrap()
                .unwrap()
                .document,
            annotation_document("capture")
        );
    }

    #[test]
    fn valid_editor_snapshot_returns_the_matching_document_and_clean_source() {
        let (_dir, root) = temp_root();
        let mut library = AssetLibrary::open(root).unwrap();
        let asset = library
            .import_data_with_annotation_project(
                b"flattened",
                CaptureKind::Image,
                "png",
                100,
                80,
                None,
                None,
                None,
                b"clean-source",
                &annotation_document("first"),
            )
            .unwrap();
        let snapshot = library.load_editor_snapshot(&asset.id).unwrap();
        assert_eq!(snapshot.state, EditorAnnotationState::Valid);
        assert_eq!(snapshot.document, Some(annotation_document("first")));
        assert_eq!(snapshot.source, b"clean-source");
        assert!(is_sha256_hex(&snapshot.revision_sha256));
    }

    #[test]
    fn semantically_invalid_document_falls_back_to_the_current_flat() {
        let (_dir, root) = temp_root();
        let mut library = AssetLibrary::open(root).unwrap();
        let asset = library
            .import_data_with_annotation_project(
                b"flattened",
                CaptureKind::Image,
                "png",
                100,
                80,
                None,
                None,
                None,
                b"clean-source",
                &annotation_document("valid"),
            )
            .unwrap();
        let project = library.load_annotation_project(&asset.id).unwrap().unwrap();
        let mut wrapper: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&project.document_url).unwrap()).unwrap();
        wrapper["document"]["sourcePixels"]["width"] = serde_json::json!(0);
        std::fs::write(
            &project.document_url,
            serde_json::to_vec_pretty(&wrapper).unwrap(),
        )
        .unwrap();

        assert!(matches!(
            library.load_annotation_project(&asset.id),
            Err(AssetLibraryError::CorruptAnnotationProject(_))
        ));
        let snapshot = library.load_editor_snapshot(&asset.id).unwrap();
        assert_eq!(snapshot.state, EditorAnnotationState::Invalid);
        assert_eq!(snapshot.document, None);
        assert_eq!(snapshot.source, b"flattened");
    }

    #[test]
    fn schema_valid_but_asset_mismatched_documents_fall_back_to_the_current_flat() {
        let (_dir, root) = temp_root();
        let mut library = AssetLibrary::open(root).unwrap();
        let asset = library
            .import_data_with_annotation_project(
                b"flattened",
                CaptureKind::Image,
                "png",
                100,
                80,
                None,
                None,
                None,
                b"clean-source",
                &annotation_document("valid"),
            )
            .unwrap();
        let project = library.load_annotation_project(&asset.id).unwrap().unwrap();
        let original: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&project.document_url).unwrap()).unwrap();

        let mut wrong_pixels = original.clone();
        wrong_pixels["document"]["sourcePixels"]["width"] = serde_json::json!(99);
        std::fs::write(
            &project.document_url,
            serde_json::to_vec_pretty(&wrong_pixels).unwrap(),
        )
        .unwrap();
        let pixel_mismatch = library.load_editor_snapshot(&asset.id).unwrap();
        assert_eq!(pixel_mismatch.state, EditorAnnotationState::Invalid);
        assert_eq!(pixel_mismatch.document, None);
        assert_eq!(pixel_mismatch.source, b"flattened");

        let mut wrong_ratio = original;
        wrong_ratio["document"]["canvas"]["width"] = serde_json::json!(120);
        std::fs::write(
            &project.document_url,
            serde_json::to_vec_pretty(&wrong_ratio).unwrap(),
        )
        .unwrap();
        let ratio_mismatch = library.load_editor_snapshot(&asset.id).unwrap();
        assert_eq!(ratio_mismatch.state, EditorAnnotationState::Invalid);
        assert_eq!(ratio_mismatch.document, None);
        assert_eq!(ratio_mismatch.source, b"flattened");
    }

    #[test]
    fn editor_snapshots_use_the_current_flat_for_none_and_invalid_projects() {
        let (_dir, root) = temp_root();
        let mut library = AssetLibrary::open(root).unwrap();
        let asset = library
            .import_data(
                b"legacy-flat",
                CaptureKind::Image,
                "png",
                100,
                80,
                None,
                None,
                None,
            )
            .unwrap();

        let none = library.load_editor_snapshot(&asset.id).unwrap();
        assert_eq!(none.state, EditorAnnotationState::None);
        assert_eq!(none.document, None);
        assert_eq!(none.source, b"legacy-flat");

        library
            .save_annotation_project(&asset.id, b"valid-flat", &annotation_document("valid"))
            .unwrap();
        let valid = library.load_editor_snapshot(&asset.id).unwrap();
        assert_eq!(valid.state, EditorAnnotationState::Valid);
        assert_eq!(valid.source, b"legacy-flat");

        library.replace_data(b"external-flat", &asset.id).unwrap();
        let invalid = library.load_editor_snapshot(&asset.id).unwrap();
        assert_eq!(invalid.state, EditorAnnotationState::Invalid);
        assert_eq!(invalid.document, None);
        assert_eq!(invalid.source, b"external-flat");
        assert_ne!(invalid.revision_sha256, valid.revision_sha256);
    }

    #[test]
    fn editor_revision_cas_rejects_changes_to_flat_document_or_source() {
        let (_dir, root) = temp_root();
        let mut library = AssetLibrary::open(root).unwrap();
        let asset = library
            .import_data_with_annotation_project(
                b"flat",
                CaptureKind::Image,
                "png",
                100,
                80,
                None,
                None,
                None,
                b"source",
                &annotation_document("initial"),
            )
            .unwrap();
        let baseline = library.load_editor_snapshot(&asset.id).unwrap();
        let project = library.load_annotation_project(&asset.id).unwrap().unwrap();
        let asset_url = library.asset_url(&asset);
        let original_flat = std::fs::read(&asset_url).unwrap();
        let original_document = std::fs::read(&project.document_url).unwrap();
        let original_source = std::fs::read(&project.source_url).unwrap();

        let assert_rejected = |library: &AssetLibrary| {
            let changed = library.load_editor_snapshot(&asset.id).unwrap();
            assert_ne!(changed.revision_sha256, baseline.revision_sha256);
            assert!(matches!(
                library.save_editor_snapshot(
                    &asset.id,
                    &baseline.revision_sha256,
                    b"must-not-write",
                    Some(&annotation_document("must-not-write")),
                ),
                Err(AssetLibraryError::AnnotationRevisionMismatch)
            ));
        };

        std::fs::write(&asset_url, b"changed-flat").unwrap();
        assert_rejected(&library);
        std::fs::write(&asset_url, &original_flat).unwrap();

        let mut reformatted_document = original_document.clone();
        reformatted_document.push(b'\n');
        std::fs::write(&project.document_url, reformatted_document).unwrap();
        assert_rejected(&library);
        std::fs::write(&project.document_url, &original_document).unwrap();

        std::fs::write(&project.source_url, b"changed-source").unwrap();
        assert_rejected(&library);
        std::fs::write(&project.source_url, &original_source).unwrap();

        let restored = library.load_editor_snapshot(&asset.id).unwrap();
        assert_eq!(restored.revision_sha256, baseline.revision_sha256);
        assert_eq!(std::fs::read(asset_url).unwrap(), original_flat);
        assert_eq!(
            std::fs::read(project.document_url).unwrap(),
            original_document
        );
        assert_eq!(std::fs::read(project.source_url).unwrap(), original_source);
    }

    #[test]
    fn editor_cas_saves_none_valid_and_invalid_projects() {
        let (_dir, root) = temp_root();
        let mut library = AssetLibrary::open(root).unwrap();
        let asset = library
            .import_data(
                b"none-flat",
                CaptureKind::Image,
                "png",
                100,
                80,
                None,
                None,
                None,
            )
            .unwrap();

        let none = library.load_editor_snapshot(&asset.id).unwrap();
        library
            .save_editor_snapshot(
                &asset.id,
                &none.revision_sha256,
                b"first-rendered",
                Some(&annotation_document("first")),
            )
            .unwrap();
        let first = library.load_editor_snapshot(&asset.id).unwrap();
        assert_eq!(first.state, EditorAnnotationState::Valid);
        assert_eq!(first.source, b"none-flat");
        assert_eq!(first.document, Some(annotation_document("first")));

        library
            .save_editor_snapshot(
                &asset.id,
                &first.revision_sha256,
                b"second-rendered",
                Some(&annotation_document("second")),
            )
            .unwrap();
        let second = library.load_editor_snapshot(&asset.id).unwrap();
        assert_eq!(second.state, EditorAnnotationState::Valid);
        assert_eq!(second.source, b"none-flat");
        assert_eq!(second.document, Some(annotation_document("second")));

        library.replace_data(b"external-flat", &asset.id).unwrap();
        let invalid = library.load_editor_snapshot(&asset.id).unwrap();
        assert_eq!(invalid.state, EditorAnnotationState::Invalid);
        library
            .save_editor_snapshot(
                &asset.id,
                &invalid.revision_sha256,
                b"repaired-rendered",
                Some(&annotation_document("repaired")),
            )
            .unwrap();
        let repaired = library.load_editor_snapshot(&asset.id).unwrap();
        assert_eq!(repaired.state, EditorAnnotationState::Valid);
        assert_eq!(repaired.source, b"external-flat");
        assert_eq!(repaired.document, Some(annotation_document("repaired")));
        assert_eq!(
            std::fs::read(library.asset_url(&asset)).unwrap(),
            b"repaired-rendered"
        );
    }

    #[test]
    fn editor_cas_clear_updates_flat_and_removes_projects_in_all_states() {
        let (_dir, root) = temp_root();
        let mut library = AssetLibrary::open(root).unwrap();

        let none = library
            .import_data(b"none", CaptureKind::Image, "png", 1, 1, None, None, None)
            .unwrap();
        let none_snapshot = library.load_editor_snapshot(&none.id).unwrap();
        library
            .save_editor_snapshot(&none.id, &none_snapshot.revision_sha256, b"none-new", None)
            .unwrap();
        let none_after = library.load_editor_snapshot(&none.id).unwrap();
        assert_eq!(none_after.state, EditorAnnotationState::None);
        assert_eq!(none_after.source, b"none-new");

        let valid = library
            .import_data_with_annotation_project(
                b"valid-flat",
                CaptureKind::Image,
                "png",
                100,
                80,
                None,
                None,
                None,
                b"valid-source",
                &annotation_document("valid"),
            )
            .unwrap();
        let valid_snapshot = library.load_editor_snapshot(&valid.id).unwrap();
        library
            .save_editor_snapshot(
                &valid.id,
                &valid_snapshot.revision_sha256,
                b"valid-new",
                None,
            )
            .unwrap();
        let valid_after = library.load_editor_snapshot(&valid.id).unwrap();
        assert_eq!(valid_after.state, EditorAnnotationState::None);
        assert_eq!(valid_after.source, b"valid-new");

        let invalid = library
            .import_data_with_annotation_project(
                b"invalid-flat",
                CaptureKind::Image,
                "png",
                100,
                80,
                None,
                None,
                None,
                b"invalid-source",
                &annotation_document("invalid"),
            )
            .unwrap();
        library
            .replace_data(b"external-invalid", &invalid.id)
            .unwrap();
        let invalid_snapshot = library.load_editor_snapshot(&invalid.id).unwrap();
        assert_eq!(invalid_snapshot.state, EditorAnnotationState::Invalid);
        library
            .save_editor_snapshot(
                &invalid.id,
                &invalid_snapshot.revision_sha256,
                b"invalid-new",
                None,
            )
            .unwrap();
        let invalid_after = library.load_editor_snapshot(&invalid.id).unwrap();
        assert_eq!(invalid_after.state, EditorAnnotationState::None);
        assert_eq!(invalid_after.source, b"invalid-new");
    }

    #[test]
    fn legacy_first_save_snapshots_source_and_later_saves_keep_it_immutable() {
        let (_dir, root) = temp_root();
        let mut library = AssetLibrary::open(root).unwrap();
        let asset = library
            .import_data(
                b"legacy-flat",
                CaptureKind::Image,
                "png",
                100,
                80,
                None,
                None,
                None,
            )
            .unwrap();
        assert!(library
            .load_annotation_project(&asset.id)
            .unwrap()
            .is_none());

        library
            .save_annotation_project(&asset.id, b"rendered-one", &annotation_document("one"))
            .unwrap();
        let first = library.load_annotation_project(&asset.id).unwrap().unwrap();
        assert_eq!(std::fs::read(&first.source_url).unwrap(), b"legacy-flat");
        assert_eq!(first.document, annotation_document("one"));

        library
            .save_annotation_project(&asset.id, b"rendered-two", &annotation_document("two"))
            .unwrap();
        let second = library.load_annotation_project(&asset.id).unwrap().unwrap();
        assert_eq!(std::fs::read(&second.source_url).unwrap(), b"legacy-flat");
        assert_eq!(second.document, annotation_document("two"));
        assert_eq!(
            std::fs::read(library.asset_url(&asset)).unwrap(),
            b"rendered-two"
        );
    }

    #[test]
    fn stale_or_incomplete_projects_fail_closed_and_can_be_cleared() {
        let (_dir, root) = temp_root();
        let mut library = AssetLibrary::open(root).unwrap();
        let asset = library
            .import_data(
                b"flat",
                CaptureKind::Image,
                "png",
                100,
                80,
                None,
                None,
                None,
            )
            .unwrap();
        save_test_project(&library, &asset, "attached");

        library
            .replace_data(b"external-replacement", &asset.id)
            .unwrap();
        assert!(matches!(
            library.load_annotation_project(&asset.id),
            Err(AssetLibraryError::StaleAnnotationProject)
        ));
        assert!(matches!(
            library.save_annotation_project(
                &asset.id,
                b"new-render",
                &annotation_document("must-not-apply")
            ),
            Err(AssetLibraryError::StaleAnnotationProject)
        ));

        library.clear_annotation_project(&asset.id).unwrap();
        assert!(library
            .load_annotation_project(&asset.id)
            .unwrap()
            .is_none());
        library
            .save_annotation_project(&asset.id, b"new-render", &annotation_document("fresh"))
            .unwrap();
        let fresh = library.load_annotation_project(&asset.id).unwrap().unwrap();
        assert_eq!(
            std::fs::read(&fresh.source_url).unwrap(),
            b"external-replacement"
        );

        std::fs::remove_file(&fresh.source_url).unwrap();
        assert!(matches!(
            library.load_annotation_project(&asset.id),
            Err(AssetLibraryError::IncompleteAnnotationProject)
        ));
    }

    #[test]
    fn corrupt_projects_are_reported_and_explicit_clear_recovers() {
        let (_dir, root) = temp_root();
        let mut library = AssetLibrary::open(root).unwrap();
        let asset = library
            .import_data(b"flat", CaptureKind::Image, "png", 1, 1, None, None, None)
            .unwrap();
        save_test_project(&library, &asset, "corrupt");
        let project = library.load_annotation_project(&asset.id).unwrap().unwrap();
        std::fs::write(&project.document_url, b"{broken").unwrap();

        assert!(matches!(
            library.load_annotation_project(&asset.id),
            Err(AssetLibraryError::CorruptAnnotationProject(_))
        ));
        library.clear_annotation_project(&asset.id).unwrap();
        assert!(library
            .load_annotation_project(&asset.id)
            .unwrap()
            .is_none());
    }

    #[test]
    fn editor_cas_restores_all_files_after_invalid_project_write_failure() {
        let (_dir, root) = temp_root();
        let mut library = AssetLibrary::open(root).unwrap();
        let asset = library
            .import_data(
                b"flat",
                CaptureKind::Image,
                "png",
                100,
                80,
                None,
                None,
                None,
            )
            .unwrap();
        save_test_project(&library, &asset, "old");
        library.replace_data(b"external-flat", &asset.id).unwrap();

        let (document_url, source_url) = library.annotation_project_urls(&asset);
        let asset_url = library.asset_url(&asset);
        let previous_document = std::fs::read(&document_url).unwrap();
        let previous_source = std::fs::read(&source_url).unwrap();
        let previous_rendered = std::fs::read(&asset_url).unwrap();
        let invalid = library.load_editor_snapshot(&asset.id).unwrap();
        assert_eq!(invalid.state, EditorAnnotationState::Invalid);
        library.annotation_replace_fail_at_write.set(Some(3));

        assert!(matches!(
            library.save_editor_snapshot(
                &asset.id,
                &invalid.revision_sha256,
                b"new-rendered",
                Some(&annotation_document("new"))
            ),
            Err(AssetLibraryError::Io(_))
        ));
        library.annotation_replace_fail_at_write.set(None);
        assert_eq!(std::fs::read(document_url).unwrap(), previous_document);
        assert_eq!(std::fs::read(source_url).unwrap(), previous_source);
        assert_eq!(std::fs::read(asset_url).unwrap(), previous_rendered);
        assert!(matches!(
            library.load_annotation_project(&asset.id),
            Err(AssetLibraryError::StaleAnnotationProject)
        ));
    }

    #[test]
    fn annotation_projects_reject_non_image_assets_before_writing_files() {
        let (_dir, root) = temp_root();
        let mut library = AssetLibrary::open(root.clone()).unwrap();

        assert!(matches!(
            library.import_data_with_annotation_project(
                b"video",
                CaptureKind::Video,
                "mp4",
                1,
                1,
                Some(1.0),
                None,
                None,
                b"source",
                &annotation_document("video"),
            ),
            Err(AssetLibraryError::UnsupportedAnnotationAsset)
        ));
        assert!(library.index.is_empty());
        assert_eq!(std::fs::read_dir(root.join("Assets")).unwrap().count(), 0);
        assert_eq!(
            std::fs::read_dir(root.join("Annotations")).unwrap().count(),
            0
        );
    }

    #[test]
    fn annotated_import_rolls_back_all_files_when_index_persistence_fails() {
        let (_dir, root) = temp_root();
        let mut library = AssetLibrary::open(root.clone()).unwrap();
        let blocked_index = root.join("blocked-index");
        std::fs::create_dir(&blocked_index).unwrap();
        library.index_url = blocked_index;

        assert!(matches!(
            library.import_data_with_annotation_project(
                b"flat",
                CaptureKind::Image,
                "png",
                1,
                1,
                None,
                None,
                None,
                b"clean",
                &annotation_document("rollback"),
            ),
            Err(AssetLibraryError::Io(_))
        ));
        assert!(library.index.is_empty());
        assert_eq!(std::fs::read_dir(root.join("Assets")).unwrap().count(), 0);
        assert_eq!(
            std::fs::read_dir(root.join("Annotations")).unwrap().count(),
            0
        );
    }

    #[test]
    fn rejects_invalid_extensions() {
        let (_dir, root) = temp_root();
        let mut library = AssetLibrary::open(root).unwrap();
        assert!(matches!(
            library.import_data(b"x", CaptureKind::Image, "../evil", 1, 1, None, None, None),
            Err(AssetLibraryError::InvalidFilename)
        ));
    }

    #[test]
    fn trash_restore_and_permanent_delete() {
        let (_dir, root) = temp_root();
        let mut library = AssetLibrary::open(root).unwrap();
        let asset = library
            .import_data(b"x", CaptureKind::Image, "png", 1, 1, None, None, None)
            .unwrap();
        save_test_project(&library, &asset, "trash");
        let project = library.load_annotation_project(&asset.id).unwrap().unwrap();

        library.move_to_trash(&asset.id).unwrap();
        assert!(library.all_assets(false).is_empty());
        assert_eq!(library.all_assets(true).len(), 1);
        assert!(project.document_url.exists());
        assert!(project.source_url.exists());

        library.restore(&asset.id).unwrap();
        assert_eq!(library.all_assets(false).len(), 1);
        // Restored captures leave Trash: all_assets(true) must not include
        // them (regression: the filter used `include_trashed || trashed.is_none()`
        // which was always true for the Trash view, so restored captures kept
        // appearing in both Library and Trash).
        assert!(library.all_assets(true).is_empty());
        assert_eq!(
            library
                .load_annotation_project(&asset.id)
                .unwrap()
                .unwrap()
                .document,
            annotation_document("trash")
        );

        library.move_to_trash(&asset.id).unwrap();
        library.permanently_delete(&asset.id).unwrap();
        assert!(library.all_assets(true).is_empty());
        assert!(!library.asset_url(&asset).exists());
        assert!(!project.document_url.exists());
        assert!(!project.source_url.exists());
    }

    #[test]
    fn permanent_delete_keeps_index_and_file_when_persist_fails() {
        let (_dir, root) = temp_root();
        let mut library = AssetLibrary::open(root.clone()).unwrap();
        let asset = library
            .import_data(b"kept", CaptureKind::Image, "png", 1, 1, None, None, None)
            .unwrap();
        save_test_project(&library, &asset, "kept");
        let project = library.load_annotation_project(&asset.id).unwrap().unwrap();
        library.move_to_trash(&asset.id).unwrap();
        let asset_path = library.asset_url(&asset);

        let blocked_index = root.join("blocked-index");
        std::fs::create_dir(&blocked_index).unwrap();
        library.index_url = blocked_index;

        assert!(matches!(
            library.permanently_delete(&asset.id),
            Err(AssetLibraryError::Io(_))
        ));
        assert!(library.asset_by_id(&asset.id).is_some());
        assert!(asset_path.exists());
        assert!(project.document_url.exists());
        assert!(project.source_url.exists());

        let reopened = AssetLibrary::open(root).unwrap();
        assert!(reopened.asset_by_id(&asset.id).is_some());
        assert!(reopened.asset_url(&asset).exists());
    }

    #[test]
    fn batch_mutations_persist_once_for_multiple_assets() {
        let (_dir, root) = temp_root();
        let mut library = AssetLibrary::open(root).unwrap();
        let assets = import_test_assets(&mut library, 3);
        let ids = assets.iter().map(|asset| asset.id).collect::<Vec<_>>();
        for (index, asset) in assets.iter().enumerate() {
            save_test_project(&library, asset, &format!("batch-{index}"));
        }
        let projects = ids
            .iter()
            .map(|id| library.load_annotation_project(id).unwrap().unwrap())
            .collect::<Vec<_>>();

        library.persist_count.set(0);
        library.batch_move_to_trash(&ids).unwrap();
        assert_eq!(library.persist_count.get(), 1);
        assert!(ids
            .iter()
            .all(|id| library.asset_by_id(id).unwrap().trashed_at.is_some()));
        assert!(projects
            .iter()
            .all(|project| project.document_url.exists() && project.source_url.exists()));

        library.persist_count.set(0);
        library.batch_restore(&ids).unwrap();
        assert_eq!(library.persist_count.get(), 1);
        assert!(ids
            .iter()
            .all(|id| library.asset_by_id(id).unwrap().trashed_at.is_none()));

        library.persist_count.set(0);
        library.batch_set_favorite(true, &ids).unwrap();
        assert_eq!(library.persist_count.get(), 1);
        assert!(ids
            .iter()
            .all(|id| library.asset_by_id(id).unwrap().is_favorite));

        library.persist_count.set(0);
        library.batch_permanently_delete(&ids).unwrap();
        assert_eq!(library.persist_count.get(), 1);
        assert!(ids.iter().all(|id| library.asset_by_id(id).is_none()));
        assert!(assets
            .iter()
            .all(|asset| !library.asset_url(asset).exists()));
        assert!(projects
            .iter()
            .all(|project| !project.document_url.exists() && !project.source_url.exists()));
    }

    #[test]
    fn batch_validation_failure_has_no_partial_mutation_or_persist() {
        let (_dir, root) = temp_root();
        let mut library = AssetLibrary::open(root).unwrap();
        let assets = import_test_assets(&mut library, 2);
        let missing = uuid::Uuid::new_v4();
        let ids = vec![assets[0].id, missing, assets[1].id];

        library.persist_count.set(0);
        assert!(matches!(
            library.batch_move_to_trash(&ids),
            Err(AssetLibraryError::AssetNotFound)
        ));
        assert_eq!(library.persist_count.get(), 0);
        assert!(assets.iter().all(|asset| library
            .asset_by_id(&asset.id)
            .unwrap()
            .trashed_at
            .is_none()));

        assert!(matches!(
            library.batch_set_favorite(true, &ids),
            Err(AssetLibraryError::AssetNotFound)
        ));
        assert_eq!(library.persist_count.get(), 0);
        assert!(assets
            .iter()
            .all(|asset| !library.asset_by_id(&asset.id).unwrap().is_favorite));

        for asset in &assets {
            library.move_to_trash(&asset.id).unwrap();
        }
        library.persist_count.set(0);
        assert!(matches!(
            library.batch_restore(&ids),
            Err(AssetLibraryError::AssetNotFound)
        ));
        assert_eq!(library.persist_count.get(), 0);
        assert!(assets.iter().all(|asset| library
            .asset_by_id(&asset.id)
            .unwrap()
            .trashed_at
            .is_some()));

        assert!(matches!(
            library.batch_permanently_delete(&ids),
            Err(AssetLibraryError::AssetNotFound)
        ));
        assert_eq!(library.persist_count.get(), 0);
        assert!(assets
            .iter()
            .all(|asset| library.asset_by_id(&asset.id).is_some()));
        assert!(assets.iter().all(|asset| library.asset_url(asset).exists()));
    }

    #[test]
    fn batch_persist_failure_keeps_memory_disk_and_files() {
        let (_dir, root) = temp_root();
        let mut library = AssetLibrary::open(root.clone()).unwrap();
        let assets = import_test_assets(&mut library, 2);
        let ids = assets.iter().map(|asset| asset.id).collect::<Vec<_>>();

        let blocked_index = root.join("blocked-index");
        std::fs::create_dir(&blocked_index).unwrap();
        library.index_url = blocked_index;

        library.persist_count.set(0);
        assert!(matches!(
            library.batch_move_to_trash(&ids),
            Err(AssetLibraryError::Io(_))
        ));
        assert_eq!(library.persist_count.get(), 1);
        assert!(assets.iter().all(|asset| library
            .asset_by_id(&asset.id)
            .unwrap()
            .trashed_at
            .is_none()));

        library.persist_count.set(0);
        assert!(matches!(
            library.batch_permanently_delete(&ids),
            Err(AssetLibraryError::Io(_))
        ));
        assert_eq!(library.persist_count.get(), 1);
        assert!(assets
            .iter()
            .all(|asset| library.asset_by_id(&asset.id).is_some()));
        assert!(assets.iter().all(|asset| library.asset_url(asset).exists()));

        let reopened = AssetLibrary::open(root).unwrap();
        assert!(assets
            .iter()
            .all(|asset| reopened.asset_by_id(&asset.id).is_some()));
    }

    #[test]
    fn batch_delete_reports_cleanup_failure_after_committing_index() {
        let (_dir, root) = temp_root();
        let mut library = AssetLibrary::open(root.clone()).unwrap();
        let assets = import_test_assets(&mut library, 2);
        let ids = assets.iter().map(|asset| asset.id).collect::<Vec<_>>();
        library.batch_move_to_trash(&ids).unwrap();

        let blocked_asset_path = library.asset_url(&assets[0]);
        std::fs::remove_file(&blocked_asset_path).unwrap();
        std::fs::create_dir(&blocked_asset_path).unwrap();

        library.persist_count.set(0);
        assert!(matches!(
            library.batch_permanently_delete(&ids),
            Err(AssetLibraryError::CleanupFailed { failed_files: 1 })
        ));
        assert_eq!(library.persist_count.get(), 1);
        assert!(ids.iter().all(|id| library.asset_by_id(id).is_none()));
        assert!(blocked_asset_path.is_dir());
        assert!(!library.asset_url(&assets[1]).exists());

        let reopened = AssetLibrary::open(root).unwrap();
        assert!(ids.iter().all(|id| reopened.asset_by_id(id).is_none()));
    }

    #[test]
    fn permanent_delete_reports_annotation_cleanup_failure_after_committing_index() {
        let (_dir, root) = temp_root();
        let mut library = AssetLibrary::open(root.clone()).unwrap();
        let asset = library
            .import_data(b"flat", CaptureKind::Image, "png", 1, 1, None, None, None)
            .unwrap();
        save_test_project(&library, &asset, "single-cleanup");
        let project = library.load_annotation_project(&asset.id).unwrap().unwrap();
        library.move_to_trash(&asset.id).unwrap();
        std::fs::remove_file(&project.source_url).unwrap();
        std::fs::create_dir(&project.source_url).unwrap();

        assert!(matches!(
            library.permanently_delete(&asset.id),
            Err(AssetLibraryError::CleanupFailed { failed_files: 1 })
        ));
        assert!(library.asset_by_id(&asset.id).is_none());
        assert!(!library.asset_url(&asset).exists());
        assert!(!project.document_url.exists());
        assert!(project.source_url.is_dir());

        let reopened = AssetLibrary::open(root).unwrap();
        assert!(reopened.asset_by_id(&asset.id).is_none());
    }

    #[test]
    fn all_assets_splits_trashed_from_active() {
        let (_dir, root) = temp_root();
        let mut library = AssetLibrary::open(root).unwrap();
        let active = library
            .import_data(b"a", CaptureKind::Image, "png", 1, 1, None, None, None)
            .unwrap();
        let trashed = library
            .import_data(b"t", CaptureKind::Image, "png", 1, 1, None, None, None)
            .unwrap();
        library.move_to_trash(&trashed.id).unwrap();

        let active_ids: Vec<_> = library
            .all_assets(false)
            .into_iter()
            .map(|a| a.id)
            .collect();
        let trashed_ids: Vec<_> = library.all_assets(true).into_iter().map(|a| a.id).collect();

        assert_eq!(active_ids, vec![active.id]);
        assert_eq!(trashed_ids, vec![trashed.id]);
        assert!(!trashed_ids.contains(&active.id));
    }

    #[test]
    fn empty_trash_removes_only_trashed() {
        let (_dir, root) = temp_root();
        let mut library = AssetLibrary::open(root).unwrap();
        let kept = library
            .import_data(b"k", CaptureKind::Image, "png", 1, 1, None, None, None)
            .unwrap();
        let gone = library
            .import_data(b"g", CaptureKind::Image, "png", 1, 1, None, None, None)
            .unwrap();
        save_test_project(&library, &gone, "empty-trash");
        let gone_project = library.load_annotation_project(&gone.id).unwrap().unwrap();
        library.move_to_trash(&gone.id).unwrap();

        library.empty_trash().unwrap();
        // Only the active capture remains; the trashed one was purged and
        // must not appear in the Trash view.
        assert!(library.all_assets(true).is_empty());
        assert_eq!(library.all_assets(false).len(), 1);
        assert_eq!(library.all_assets(false)[0].id, kept.id);
        assert!(!gone_project.document_url.exists());
        assert!(!gone_project.source_url.exists());
    }

    #[test]
    fn empty_trash_keeps_index_and_files_when_persist_fails() {
        let (_dir, root) = temp_root();
        let mut library = AssetLibrary::open(root.clone()).unwrap();
        let active = library
            .import_data(b"active", CaptureKind::Image, "png", 1, 1, None, None, None)
            .unwrap();
        let trashed = library
            .import_data(
                b"trashed",
                CaptureKind::Image,
                "png",
                1,
                1,
                None,
                None,
                None,
            )
            .unwrap();
        save_test_project(&library, &trashed, "persist-failure");
        let trashed_project = library
            .load_annotation_project(&trashed.id)
            .unwrap()
            .unwrap();
        library.move_to_trash(&trashed.id).unwrap();
        let active_path = library.asset_url(&active);
        let trashed_path = library.asset_url(&trashed);

        let blocked_index = root.join("blocked-index");
        std::fs::create_dir(&blocked_index).unwrap();
        library.index_url = blocked_index;

        assert!(matches!(
            library.empty_trash(),
            Err(AssetLibraryError::Io(_))
        ));
        assert_eq!(library.all_assets(false)[0].id, active.id);
        assert_eq!(library.all_assets(true)[0].id, trashed.id);
        assert!(active_path.exists());
        assert!(trashed_path.exists());
        assert!(trashed_project.document_url.exists());
        assert!(trashed_project.source_url.exists());

        let reopened = AssetLibrary::open(root).unwrap();
        assert!(reopened.asset_by_id(&active.id).is_some());
        assert!(reopened.asset_by_id(&trashed.id).is_some());
        assert!(reopened.asset_url(&active).exists());
        assert!(reopened.asset_url(&trashed).exists());
    }

    #[test]
    fn empty_trash_reports_annotation_cleanup_failure_after_committing_index() {
        let (_dir, root) = temp_root();
        let mut library = AssetLibrary::open(root.clone()).unwrap();
        let active = library
            .import_data(b"active", CaptureKind::Image, "png", 1, 1, None, None, None)
            .unwrap();
        let trashed = library
            .import_data(
                b"trashed",
                CaptureKind::Image,
                "png",
                1,
                1,
                None,
                None,
                None,
            )
            .unwrap();
        save_test_project(&library, &trashed, "empty-cleanup");
        let project = library
            .load_annotation_project(&trashed.id)
            .unwrap()
            .unwrap();
        library.move_to_trash(&trashed.id).unwrap();
        std::fs::remove_file(&project.source_url).unwrap();
        std::fs::create_dir(&project.source_url).unwrap();

        assert!(matches!(
            library.empty_trash(),
            Err(AssetLibraryError::CleanupFailed { failed_files: 1 })
        ));
        assert_eq!(library.all_assets(false)[0].id, active.id);
        assert!(library.all_assets(true).is_empty());
        assert!(!library.asset_url(&trashed).exists());
        assert!(!project.document_url.exists());
        assert!(project.source_url.is_dir());

        let reopened = AssetLibrary::open(root).unwrap();
        assert_eq!(reopened.all_assets(false)[0].id, active.id);
        assert!(reopened.all_assets(true).is_empty());
    }

    #[test]
    fn search_matches_swift_fields() {
        let (_dir, root) = temp_root();
        let mut library = AssetLibrary::open(root).unwrap();
        let asset = library
            .import_data(
                b"x",
                CaptureKind::Image,
                "png",
                1,
                1,
                None,
                Some("Safari".into()),
                None,
            )
            .unwrap();
        assert_eq!(library.search("safari", false).len(), 1);
        assert_eq!(library.search("png", false).len(), 1);
        assert!(library.search("nomatch", false).is_empty());
        assert_eq!(library.search("", false).len(), 1);
        assert_eq!(library.search("", false)[0].id, asset.id);
    }

    #[test]
    fn index_round_trips_with_corrupt_fallback() {
        let (_dir, root) = temp_root();
        {
            let mut library = AssetLibrary::open(root.clone()).unwrap();
            library
                .import_data(
                    b"x",
                    CaptureKind::Video,
                    "mp4",
                    1920,
                    1080,
                    Some(3.5),
                    None,
                    None,
                )
                .unwrap();
        }
        let reloaded = AssetLibrary::open(root.clone()).unwrap();
        assert_eq!(reloaded.all_assets(false).len(), 1);
        assert_eq!(reloaded.all_assets(false)[0].duration, Some(3.5));

        std::fs::write(root.join("library.json"), "{broken").unwrap();
        assert!(matches!(
            AssetLibrary::open(root),
            Err(AssetLibraryError::CorruptIndex(_))
        ));
    }

    #[test]
    fn filenames_use_local_timestamp_and_lowercase_uuid() {
        let asset = AssetLibrary::make_asset(
            CaptureKind::Image,
            "png",
            1,
            1,
            None,
            None,
            1_700_000_000_000.0,
        );
        assert_eq!(
            asset.filename.len(),
            "20231114-221320-xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx.png".len()
        );
        assert!(asset.filename.ends_with(".png"));
        let uuid_part = asset
            .filename
            .strip_suffix(".png")
            .unwrap()
            .rsplit('-')
            .take(5)
            .collect::<Vec<_>>()
            .join("-");
        assert!(uuid_part
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'));
    }
}
