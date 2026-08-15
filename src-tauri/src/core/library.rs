//! AssetLibrary — port of Sources/KiriCore/AssetLibrary.swift.
//! Storage layout and JSON schema stay compatible with the Swift app, so an
//! existing library at ~/Library/Application Support/kiri keeps working.

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
}

pub type Result<T> = std::result::Result<T, AssetLibraryError>;

pub struct AssetLibrary {
    root_url: PathBuf,
    assets_url: PathBuf,
    thumbnails_url: PathBuf,
    index_url: PathBuf,
    index: Vec<CaptureAsset>,
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
            root_url,
            assets_url,
            thumbnails_url,
            index_url,
            index,
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
            .filter(|asset| {
                normalized.is_empty() || asset.searchable_text().contains(&normalized)
            })
            .collect()
    }

    pub fn asset_by_id(&self, id: &uuid::Uuid) -> Option<&CaptureAsset> {
        self.index.iter().find(|asset| &asset.id == id)
    }

    /// Mirrors `AssetLibrary.importData(...)`.
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

    pub fn permanently_delete(&mut self, id: &uuid::Uuid) -> Result<()> {
        let position = self
            .index
            .iter()
            .position(|asset| &asset.id == id)
            .ok_or(AssetLibraryError::AssetNotFound)?;
        let asset = self.index.remove(position);
        self.persist()?;
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
        self.index.retain(|asset| asset.trashed_at.is_none());
        self.persist()?;
        for asset in trashed {
            let _ = std::fs::remove_file(self.asset_url(&asset));
            let _ = std::fs::remove_file(
                self.thumbnails_url
                    .join(format!("{}.jpg", asset.id.to_string().to_lowercase())),
            );
        }
        Ok(())
    }

    fn update(
        &mut self,
        id: &uuid::Uuid,
        mutation: impl FnOnce(&mut CaptureAsset),
    ) -> Result<()> {
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

    fn persist(&self) -> Result<()> {
        let data = serde_json::to_vec_pretty(&self.index)
            .map_err(AssetLibraryError::CorruptIndex)?;
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
        let trashed_ids: Vec<_> = library
            .all_assets(true)
            .into_iter()
            .map(|a| a.id)
            .collect();

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
                .import_data(b"x", CaptureKind::Video, "mp4", 1920, 1080, Some(3.5), None, None)
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
        assert_eq!(asset.filename.len(), "20231114-221320-xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx.png".len());
        assert!(asset.filename.ends_with(".png"));
        let uuid_part = asset
            .filename
            .strip_suffix(".png")
            .unwrap()
            .rsplit('-')
            .take(5)
            .collect::<Vec<_>>()
            .join("-");
        assert!(uuid_part.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'));
    }
}
