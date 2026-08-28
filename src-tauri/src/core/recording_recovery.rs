//! Durable holding area for completed recordings that could not be imported.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::core::asset::{CaptureAsset, CaptureKind};
use crate::core::library::{is_safe_library_filename, AssetLibrary, AssetLibraryError};

const MANIFEST_SCHEMA_VERSION: u8 = 1;
const MAX_MANIFEST_BYTES: u64 = 64 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum RecordingRecoveryError {
    #[error("recording recovery I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("recording recovery manifest is invalid")]
    InvalidManifest,
    #[error("recording recovery manifest could not be encoded: {0}")]
    Encode(#[from] serde_json::Error),
    #[error(transparent)]
    Library(#[from] AssetLibraryError),
}

pub type Result<T> = std::result::Result<T, RecordingRecoveryError>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingRecording {
    pub schema_version: u8,
    pub id: uuid::Uuid,
    pub filename: String,
    pub created_at: f64,
    pub pixel_width: i64,
    pub pixel_height: i64,
    pub duration: Option<f64>,
    pub byte_len: u64,
    pub sha256: String,
    #[serde(default)]
    pub imported: bool,
    #[serde(default)]
    pub imported_asset: Option<ImportedAssetProof>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImportedAssetProof {
    pub kind: CaptureKind,
    pub filename: String,
    pub byte_len: u64,
    pub sha256: String,
}

impl PendingRecording {
    fn validate(&self, manifest_path: &Path) -> Result<()> {
        let expected_filename = format!("{}.mp4", self.id.simple());
        let expected_manifest = format!("{}.json", self.id.simple());
        if self.schema_version != MANIFEST_SCHEMA_VERSION
            || self.filename != expected_filename
            || manifest_path.file_name().and_then(|name| name.to_str())
                != Some(expected_manifest.as_str())
            || self.pixel_width < 0
            || self.pixel_height < 0
            || self
                .duration
                .is_some_and(|duration| !duration.is_finite() || duration < 0.0)
            || !self.created_at.is_finite()
            || self.byte_len == 0
            || !is_sha256(&self.sha256)
            || self.imported_asset.as_ref().is_some_and(|proof| {
                let extension = match proof.kind {
                    CaptureKind::Image => "png",
                    CaptureKind::Video => "mp4",
                    CaptureKind::Gif => "gif",
                };
                !is_safe_library_filename(&proof.filename)
                    || Path::new(&proof.filename)
                        .extension()
                        .and_then(|value| value.to_str())
                        .is_none_or(|value| !value.eq_ignore_ascii_case(extension))
                    || proof.byte_len == 0
                    || !is_sha256(&proof.sha256)
            })
            || (self.imported && self.imported_asset.is_none())
        {
            return Err(RecordingRecoveryError::InvalidManifest);
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct RecordingRecoveryStore {
    root: PathBuf,
}

impl RecordingRecoveryStore {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn persist(
        &self,
        source: &Path,
        pixel_width: i64,
        pixel_height: i64,
        duration: Option<f64>,
    ) -> Result<PendingRecording> {
        let source_metadata = std::fs::symlink_metadata(source)?;
        if !source_metadata.file_type().is_file() || source_metadata.len() == 0 {
            return Err(RecordingRecoveryError::InvalidManifest);
        }
        std::fs::create_dir_all(&self.root)?;
        ensure_recovery_directory(&self.root)?;

        let id = uuid::Uuid::new_v4();
        let filename = format!("{}.mp4", id.simple());
        let video_path = self.root.join(&filename);
        let manifest_path = self.root.join(format!("{}.json", id.simple()));

        let mut staged = tempfile::Builder::new()
            .prefix(".recording-video-")
            .suffix(".tmp")
            .tempfile_in(&self.root)?;
        let mut input = std::fs::File::open(source)?;
        let copied = std::io::copy(&mut input, staged.as_file_mut())?;
        staged.flush()?;
        staged.as_file().sync_all()?;
        if copied != source_metadata.len() {
            return Err(RecordingRecoveryError::InvalidManifest);
        }
        let (byte_len, sha256) = file_fingerprint(staged.path())?;
        staged
            .persist_noclobber(&video_path)
            .map_err(|error| RecordingRecoveryError::Io(error.error))?;
        if let Err(error) = sync_directory(&self.root) {
            log::warn!(
                "recording recovery video committed but its directory could not be synced: {error}"
            );
        }

        let pending = PendingRecording {
            schema_version: MANIFEST_SCHEMA_VERSION,
            id,
            filename,
            created_at: chrono::Utc::now().timestamp_millis() as f64,
            pixel_width,
            pixel_height,
            duration,
            byte_len,
            sha256,
            imported: false,
            imported_asset: None,
        };
        let encoded = serde_json::to_vec_pretty(&pending)?;
        if let Err(error) = write_new_file(&manifest_path, &encoded) {
            // The synced MP4 is already recoverable: `list` reconstructs a
            // manifest from an unaccompanied recovery video on the next run.
            log::warn!("could not write recording recovery manifest: {error}");
        }
        Ok(pending)
    }

    pub fn list(&self) -> Vec<PendingRecording> {
        if ensure_recovery_directory(&self.root).is_err() {
            return Vec::new();
        }
        let Ok(entries) = std::fs::read_dir(&self.root) else {
            return Vec::new();
        };
        let mut pending = entries
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| {
                let video_path = entry.path();
                if video_path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_none_or(|extension| !extension.eq_ignore_ascii_case("mp4"))
                {
                    return None;
                }
                let metadata = std::fs::symlink_metadata(&video_path).ok()?;
                if !metadata.file_type().is_file() || metadata.len() == 0 {
                    return None;
                }
                let stem = video_path.file_stem()?.to_str()?;
                let id = uuid::Uuid::parse_str(stem).ok()?;
                if stem != id.simple().to_string() {
                    return None;
                }
                let manifest_path = self.root.join(format!("{}.json", id.simple()));
                if let Some(item) = read_manifest(&manifest_path) {
                    if item.filename == entry.file_name().to_string_lossy()
                        && item.byte_len == metadata.len()
                    {
                        return Some(item);
                    }
                }

                let (byte_len, sha256) = file_fingerprint(&video_path).ok()?;
                Some(PendingRecording {
                    schema_version: MANIFEST_SCHEMA_VERSION,
                    id,
                    filename: entry.file_name().to_string_lossy().into_owned(),
                    created_at: modified_at_ms(&metadata),
                    pixel_width: 0,
                    pixel_height: 0,
                    duration: None,
                    byte_len,
                    sha256,
                    imported: false,
                    imported_asset: None,
                })
            })
            .collect::<Vec<_>>();
        pending.sort_by(|left, right| left.created_at.total_cmp(&right.created_at));
        pending
    }

    pub fn video_path(&self, pending: &PendingRecording) -> Result<PathBuf> {
        ensure_recovery_directory(&self.root)?;
        let manifest_path = self.root.join(format!("{}.json", pending.id.simple()));
        pending.validate(&manifest_path)?;
        let video_path = self.root.join(&pending.filename);
        let metadata = std::fs::symlink_metadata(&video_path)?;
        if !metadata.file_type().is_file() || metadata.len() != pending.byte_len {
            return Err(RecordingRecoveryError::InvalidManifest);
        }
        Ok(video_path)
    }

    pub fn validate_video(&self, pending: &PendingRecording) -> Result<PathBuf> {
        let video_path = self.video_path(pending)?;
        let (_, actual_sha256) = file_fingerprint(&video_path)?;
        if actual_sha256 != pending.sha256 {
            return Err(RecordingRecoveryError::InvalidManifest);
        }
        Ok(video_path)
    }

    /// Persists the exact output Kiri is about to import. If the process exits
    /// after the library commit but before cleanup, Retry can prove that the
    /// active library owns the same bytes before removing the recovery MP4.
    pub fn prepare_import(
        &self,
        pending: &mut PendingRecording,
        kind: CaptureKind,
        source: &Path,
    ) -> Result<()> {
        self.validate_video(pending)?;
        let (byte_len, sha256) = file_fingerprint(source)?;
        if kind == CaptureKind::Video && (byte_len != pending.byte_len || sha256 != pending.sha256)
        {
            return Err(RecordingRecoveryError::InvalidManifest);
        }
        let extension = match kind {
            CaptureKind::Image => "png",
            CaptureKind::Video => "mp4",
            CaptureKind::Gif => "gif",
        };
        pending.imported = false;
        pending.imported_asset = Some(ImportedAssetProof {
            kind,
            filename: format!("{}.{}", pending.id.simple(), extension),
            byte_len,
            sha256,
        });
        self.write_manifest(pending)
    }

    pub fn imported_asset_matches(
        &self,
        pending: &PendingRecording,
        asset: &CaptureAsset,
        asset_path: &Path,
    ) -> Result<bool> {
        self.video_path(pending)?;
        let Some(proof) = pending.imported_asset.as_ref() else {
            return Ok(false);
        };
        if asset.kind != proof.kind {
            return Ok(false);
        }
        let (byte_len, sha256) = file_fingerprint(asset_path)?;
        Ok(byte_len == proof.byte_len && sha256 == proof.sha256)
    }

    pub fn recovery_matches_import_proof(&self, pending: &PendingRecording) -> Result<bool> {
        let Some(proof) = pending.imported_asset.as_ref() else {
            return Ok(false);
        };
        self.validate_video(pending)?;
        Ok(proof.kind == CaptureKind::Video
            && pending.byte_len == proof.byte_len
            && pending.sha256 == proof.sha256)
    }

    pub fn finish_verified_import(
        &self,
        library: &AssetLibrary,
        pending: &mut PendingRecording,
    ) -> Result<()> {
        let asset = library
            .asset_by_id(&pending.id)
            .ok_or(RecordingRecoveryError::InvalidManifest)?;
        let path = library.readable_asset_url(asset)?;
        if !self.imported_asset_matches(pending, asset, &path)? {
            return Err(RecordingRecoveryError::InvalidManifest);
        }
        if !pending.imported {
            self.mark_imported(pending)?;
        }
        self.remove(pending)
    }

    fn mark_imported(&self, pending: &mut PendingRecording) -> Result<()> {
        self.video_path(pending)?;
        if pending.imported_asset.is_none() {
            return Err(RecordingRecoveryError::InvalidManifest);
        }
        pending.imported = true;
        self.write_manifest(pending)
    }

    fn write_manifest(&self, pending: &PendingRecording) -> Result<()> {
        let manifest_path = self.root.join(format!("{}.json", pending.id.simple()));
        let encoded = serde_json::to_vec_pretty(pending)?;
        write_file_atomic(&manifest_path, &encoded, true)
    }

    /// Called only after `mark_imported` records that the active library owns
    /// a durable copy. If media cleanup fails, the manifest remains so Retry
    /// can finish cleanup without importing a duplicate.
    fn remove(&self, pending: &PendingRecording) -> Result<()> {
        if !pending.imported {
            return Err(RecordingRecoveryError::InvalidManifest);
        }
        let video_path = self.video_path(pending)?;
        let manifest_path = self.root.join(format!("{}.json", pending.id.simple()));
        match std::fs::remove_file(video_path) {
            Ok(()) => sync_directory(&self.root)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        match std::fs::remove_file(manifest_path) {
            Ok(()) => sync_directory(&self.root),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    }
}

fn write_new_file(path: &Path, bytes: &[u8]) -> Result<()> {
    write_file_atomic(path, bytes, false)
}

fn write_file_atomic(path: &Path, bytes: &[u8], replace: bool) -> Result<()> {
    let parent = path
        .parent()
        .ok_or(RecordingRecoveryError::InvalidManifest)?;
    let mut temporary = tempfile::Builder::new()
        .prefix(".recording-recovery-")
        .suffix(".tmp")
        .tempfile_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.flush()?;
    temporary.as_file().sync_all()?;
    if replace {
        temporary
            .persist(path)
            .map_err(|error| RecordingRecoveryError::Io(error.error))?;
    } else {
        temporary
            .persist_noclobber(path)
            .map_err(|error| RecordingRecoveryError::Io(error.error))?;
    }
    if let Err(error) = sync_directory(parent) {
        log::warn!(
            "recording recovery manifest committed but its directory could not be synced: {error}"
        );
    }
    Ok(())
}

fn read_manifest(path: &Path) -> Option<PendingRecording> {
    let metadata = std::fs::symlink_metadata(path).ok()?;
    if !metadata.file_type().is_file() || metadata.len() > MAX_MANIFEST_BYTES {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    let pending: PendingRecording = serde_json::from_slice(&bytes).ok()?;
    pending.validate(path).ok()?;
    Some(pending)
}

fn ensure_recovery_directory(path: &Path) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(RecordingRecoveryError::InvalidManifest);
    }
    Ok(())
}

fn file_fingerprint(path: &Path) -> Result<(u64, String)> {
    let link_metadata = std::fs::symlink_metadata(path)?;
    if link_metadata.file_type().is_symlink() || !link_metadata.is_file() {
        return Err(RecordingRecoveryError::InvalidManifest);
    }
    let mut file = std::fs::File::open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() == 0 {
        return Err(RecordingRecoveryError::InvalidManifest);
    }
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok((metadata.len(), format!("{:x}", digest.finalize())))
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn modified_at_ms(metadata: &std::fs::Metadata) -> f64 {
    metadata
        .modified()
        .ok()
        .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis() as f64)
        .unwrap_or_else(|| chrono::Utc::now().timestamp_millis() as f64)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persists_lists_and_removes_a_completed_recording() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("merged.mp4");
        std::fs::write(&source, b"valid merged recording").unwrap();
        let store = RecordingRecoveryStore::new(directory.path().join("Recovery"));

        let pending = store.persist(&source, 1920, 1080, Some(4.25)).unwrap();

        assert!(
            source.exists(),
            "persistence must not consume the caller's only copy"
        );
        assert_eq!(store.list(), vec![pending.clone()]);
        assert_eq!(
            std::fs::read(store.video_path(&pending).unwrap()).unwrap(),
            b"valid merged recording"
        );

        let mut pending = pending;
        store
            .prepare_import(&mut pending, CaptureKind::Video, &source)
            .unwrap();
        let mut library = AssetLibrary::open(directory.path().join("Library")).unwrap();
        library
            .import_file_with_stable_id(
                &source,
                pending.id,
                pending.created_at,
                CaptureKind::Video,
                "mp4",
                1920,
                1080,
                Some(4.25),
                None,
            )
            .unwrap();
        store
            .finish_verified_import(&library, &mut pending)
            .unwrap();
        assert!(store.list().is_empty());
    }

    #[test]
    fn cleanup_proof_rejects_a_different_library_file() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("merged.mp4");
        let imported = directory.path().join("imported.mp4");
        std::fs::write(&source, b"valid merged recording").unwrap();
        std::fs::write(&imported, b"valid merged recording").unwrap();
        let store = RecordingRecoveryStore::new(directory.path().join("Recovery"));
        let mut pending = store.persist(&source, 1920, 1080, Some(4.25)).unwrap();
        store
            .prepare_import(&mut pending, CaptureKind::Video, &source)
            .unwrap();
        let mut asset = CaptureAsset {
            id: pending.id,
            kind: CaptureKind::Video,
            created_at: pending.created_at,
            filename: format!("{}.mp4", pending.id.simple()),
            title: None,
            tags: Vec::new(),
            pixel_width: 1920,
            pixel_height: 1080,
            duration: Some(4.25),
            source_application: None,
            is_favorite: false,
            trashed_at: None,
        };

        assert!(store
            .imported_asset_matches(&pending, &asset, &imported)
            .unwrap());
        asset.filename = format!("2026-08-29-120000-{}.mp4", pending.id.simple());
        assert!(store
            .imported_asset_matches(&pending, &asset, &imported)
            .unwrap());
        std::fs::write(&imported, b"different recording").unwrap();
        assert!(!store
            .imported_asset_matches(&pending, &asset, &imported)
            .unwrap());
        assert!(store.video_path(&pending).unwrap().exists());
    }

    #[test]
    fn video_import_proof_rejects_a_changed_source() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("merged.mp4");
        std::fs::write(&source, b"original recording").unwrap();
        let store = RecordingRecoveryStore::new(directory.path().join("Recovery"));
        let mut pending = store.persist(&source, 1920, 1080, Some(4.25)).unwrap();

        std::fs::write(&source, b"different recording").unwrap();

        assert!(matches!(
            store.prepare_import(&mut pending, CaptureKind::Video, &source),
            Err(RecordingRecoveryError::InvalidManifest)
        ));
        assert!(pending.imported_asset.is_none());
        assert!(store.validate_video(&pending).unwrap().exists());
    }

    #[test]
    fn verified_cleanup_keeps_recovery_when_the_library_copy_changed() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("merged.mp4");
        std::fs::write(&source, b"valid merged recording").unwrap();
        let store = RecordingRecoveryStore::new(directory.path().join("Recovery"));
        let mut pending = store.persist(&source, 1920, 1080, Some(4.25)).unwrap();
        store
            .prepare_import(&mut pending, CaptureKind::Video, &source)
            .unwrap();
        let mut library = AssetLibrary::open(directory.path().join("Library")).unwrap();
        let asset = library
            .import_file_with_stable_id(
                &source,
                pending.id,
                pending.created_at,
                CaptureKind::Video,
                "mp4",
                1920,
                1080,
                Some(4.25),
                None,
            )
            .unwrap();
        std::fs::write(library.asset_url(&asset), b"different recording").unwrap();

        assert!(store
            .finish_verified_import(&library, &mut pending)
            .is_err());
        assert!(store.video_path(&pending).unwrap().exists());
    }

    #[test]
    fn ignores_invalid_manifests_and_symlinked_media() {
        let directory = tempfile::tempdir().unwrap();
        let recovery = directory.path().join("Recovery");
        std::fs::create_dir_all(&recovery).unwrap();
        std::fs::write(recovery.join("invalid.json"), b"{}").unwrap();

        #[cfg(unix)]
        {
            let id = uuid::Uuid::new_v4();
            let filename = format!("{}.mp4", id.simple());
            let source = directory.path().join("outside.mp4");
            std::fs::write(&source, b"outside").unwrap();
            std::os::unix::fs::symlink(&source, recovery.join(&filename)).unwrap();
            let pending = PendingRecording {
                schema_version: MANIFEST_SCHEMA_VERSION,
                id,
                filename,
                created_at: 1.0,
                pixel_width: 1,
                pixel_height: 1,
                duration: Some(1.0),
                byte_len: 7,
                sha256: format!("{:x}", Sha256::digest(b"outside")),
                imported: false,
                imported_asset: None,
            };
            std::fs::write(
                recovery.join(format!("{}.json", id.simple())),
                serde_json::to_vec(&pending).unwrap(),
            )
            .unwrap();
        }

        let store = RecordingRecoveryStore::new(recovery);
        assert!(store.list().is_empty());
    }

    #[test]
    fn discovers_a_fully_copied_video_when_its_manifest_is_missing() {
        let directory = tempfile::tempdir().unwrap();
        let recovery = directory.path().join("Recovery");
        std::fs::create_dir_all(&recovery).unwrap();
        let id = uuid::Uuid::new_v4();
        let video = recovery.join(format!("{}.mp4", id.simple()));
        std::fs::write(&video, b"completed recording").unwrap();

        let store = RecordingRecoveryStore::new(recovery);
        let pending = store.list();

        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, id);
        assert_eq!(store.validate_video(&pending[0]).unwrap(), video);
    }
}
