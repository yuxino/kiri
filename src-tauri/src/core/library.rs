//! Local asset-library persistence and recoverable Trash operations.
//! Storage layout and JSON schema stay compatible with the Swift app, so an
//! existing library at ~/Library/Application Support/kiri keeps working.

#[cfg(test)]
use std::cell::Cell;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use chrono::{Local, TimeZone};
use thiserror::Error;

use crate::core::asset::{CaptureAsset, CaptureKind};

#[derive(Debug, Error)]
pub enum AssetLibraryError {
    #[error("asset not found")]
    AssetNotFound,
    #[error("invalid filename")]
    InvalidFilename,
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("library index is corrupted: {0}")]
    CorruptIndex(#[source] serde_json::Error),
    #[error("library index was updated, but {failed_files} file(s) could not be removed")]
    CleanupFailed { failed_files: usize },
}

pub type Result<T> = std::result::Result<T, AssetLibraryError>;

pub struct AssetLibrary {
    assets_url: PathBuf,
    thumbnails_url: PathBuf,
    index_url: PathBuf,
    index: Vec<CaptureAsset>,
    #[cfg(test)]
    persist_count: Cell<usize>,
}

impl AssetLibrary {
    pub fn open(root_url: PathBuf) -> Result<Self> {
        let assets_url = root_url.join("Assets");
        let thumbnails_url = root_url.join("Thumbnails");
        let index_url = root_url.join("library.json");

        std::fs::create_dir_all(&assets_url)?;
        std::fs::create_dir_all(&thumbnails_url)?;

        let index = if index_url.exists() {
            let data = std::fs::read(&index_url)?;
            serde_json::from_slice::<Vec<CaptureAsset>>(&data)
                .map_err(AssetLibraryError::CorruptIndex)?
        } else {
            Vec::new()
        };

        Ok(Self {
            assets_url,
            thumbnails_url,
            index_url,
            index,
            #[cfg(test)]
            persist_count: Cell::new(0),
        })
    }

    /// Mirrors `AssetLibrary.defaultRootURL()`: `~/Library/Application
    /// Support/kiri` on macOS, `%APPDATA%/kiri` on Windows.
    pub fn default_root_url() -> Option<PathBuf> {
        dirs::data_dir().map(|dir| dir.join("kiri"))
    }

    pub fn asset_url(&self, asset: &CaptureAsset) -> PathBuf {
        self.assets_url.join(&asset.filename)
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
        std::fs::write(&file_url, data)?;

        self.index.push(asset.clone());
        if let Err(error) = self.persist() {
            let _ = std::fs::remove_file(&file_url);
            self.index.retain(|entry| entry.id != asset.id);
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
        let asset = Self::make_asset(
            kind,
            &safe_extension,
            pixel_width,
            pixel_height,
            duration,
            source_application,
            Self::normalized_date(now_ms()),
        );
        let file_url = self.asset_url(&asset);
        std::fs::copy(source_url, &file_url)?;

        self.index.push(asset.clone());
        if let Err(error) = self.persist() {
            let _ = std::fs::remove_file(&file_url);
            self.index.retain(|entry| entry.id != asset.id);
            return Err(error);
        }
        Ok(asset)
    }

    /// Mirrors `AssetLibrary.replaceData(_:for:)`.
    pub fn replace_data(&mut self, data: &[u8], id: &uuid::Uuid) -> Result<CaptureAsset> {
        let asset = self
            .index
            .iter()
            .find(|asset| &asset.id == id)
            .ok_or(AssetLibraryError::AssetNotFound)?
            .clone();
        std::fs::write(self.asset_url(&asset), data)?;
        Ok(asset)
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
        let _ = std::fs::remove_file(self.asset_url(&asset));
        let _ = std::fs::remove_file(
            self.thumbnails_url
                .join(format!("{}.jpg", asset.id.to_string().to_lowercase())),
        );
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
        for asset in trashed {
            let _ = std::fs::remove_file(self.asset_url(&asset));
            let _ = std::fs::remove_file(
                self.thumbnails_url
                    .join(format!("{}.jpg", asset.id.to_string().to_lowercase())),
            );
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

    fn cleanup_asset_files(&self, asset: &CaptureAsset) -> usize {
        let paths = [
            self.asset_url(asset),
            self.thumbnails_url
                .join(format!("{}.jpg", asset.id.to_string().to_lowercase())),
        ];
        paths
            .iter()
            .filter(|path| match std::fs::remove_file(path) {
                Ok(()) => false,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
                Err(error) => {
                    log::error!("could not remove permanently deleted asset file: {error}");
                    true
                }
            })
            .count()
    }

    fn persist(&self) -> Result<()> {
        self.persist_index(&self.index)
    }

    fn persist_index(&self, index: &[CaptureAsset]) -> Result<()> {
        #[cfg(test)]
        self.persist_count.set(self.persist_count.get() + 1);
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
        let id = uuid::Uuid::new_v4();
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
}

fn now_ms() -> f64 {
    chrono::Utc::now().timestamp_millis() as f64
}

/// Write via a temporary file and rename into place. On Windows, rename over
/// an existing file is retried after removing the target.
fn atomic_write(path: &Path, data: &[u8]) -> Result<()> {
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, data)?;
    match std::fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(first) => {
            #[cfg(windows)]
            {
                let _ = std::fs::remove_file(path);
                if std::fs::rename(&tmp, path).is_ok() {
                    return Ok(());
                }
            }
            let _ = std::fs::remove_file(&tmp);
            Err(AssetLibraryError::Io(first))
        }
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

        library.move_to_trash(&asset.id).unwrap();
        assert!(library.all_assets(false).is_empty());
        assert_eq!(library.all_assets(true).len(), 1);

        library.restore(&asset.id).unwrap();
        assert_eq!(library.all_assets(false).len(), 1);
        // Restored captures leave Trash: all_assets(true) must not include
        // them (regression: the filter used `include_trashed || trashed.is_none()`
        // which was always true for the Trash view, so restored captures kept
        // appearing in both Library and Trash).
        assert!(library.all_assets(true).is_empty());

        library.move_to_trash(&asset.id).unwrap();
        library.permanently_delete(&asset.id).unwrap();
        assert!(library.all_assets(true).is_empty());
        assert!(!library.asset_url(&asset).exists());
    }

    #[test]
    fn permanent_delete_keeps_index_and_file_when_persist_fails() {
        let (_dir, root) = temp_root();
        let mut library = AssetLibrary::open(root.clone()).unwrap();
        let asset = library
            .import_data(b"kept", CaptureKind::Image, "png", 1, 1, None, None, None)
            .unwrap();
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

        library.persist_count.set(0);
        library.batch_move_to_trash(&ids).unwrap();
        assert_eq!(library.persist_count.get(), 1);
        assert!(ids
            .iter()
            .all(|id| library.asset_by_id(id).unwrap().trashed_at.is_some()));

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
        library.move_to_trash(&gone.id).unwrap();

        library.empty_trash().unwrap();
        // Only the active capture remains; the trashed one was purged and
        // must not appear in the Trash view.
        assert!(library.all_assets(true).is_empty());
        assert_eq!(library.all_assets(false).len(), 1);
        assert_eq!(library.all_assets(false)[0].id, kept.id);
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

        let reopened = AssetLibrary::open(root).unwrap();
        assert!(reopened.asset_by_id(&active.id).is_some());
        assert!(reopened.asset_by_id(&trashed.id).is_some());
        assert!(reopened.asset_url(&active).exists());
        assert!(reopened.asset_url(&trashed).exists());
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
