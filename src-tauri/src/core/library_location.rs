//! Managed asset-library location, identity, availability, and whole-library
//! migration. Paths never cross the IPC boundary; callers receive labels and
//! backend-owned actions instead.

use std::collections::HashSet;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::core::library::{is_safe_library_filename, AssetLibrary, AssetLibraryError};

const LOCATION_SCHEMA_VERSION: u32 = 2;
const MARKER_SCHEMA_VERSION: u32 = 2;
const LEGACY_SCHEMA_VERSION: u32 = 1;
const LOCATION_CONFIG_FILENAME: &str = "library-location.json";
const LIBRARY_MARKER_FILENAME: &str = ".kiri-library.json";
pub const MANAGED_LIBRARY_DIRECTORY_NAME: &str = "Kiri Library";

#[derive(Debug, Error)]
pub enum LibraryLocationError {
    #[error("library storage is unavailable")]
    Unavailable,
    #[error("library storage is being moved")]
    Migrating,
    #[error("the selected folder is not a Kiri library")]
    InvalidLibrary,
    #[error("the selected destination cannot contain the Kiri library")]
    InvalidDestination,
    #[error("the selected library has a different identity")]
    LibraryIdentityMismatch,
    #[error("the selected library is not the current copy")]
    LibraryGenerationMismatch,
    #[error("library location configuration is invalid: {0}")]
    InvalidConfiguration(#[source] serde_json::Error),
    #[error("library location marker is invalid: {0}")]
    InvalidMarker(#[source] serde_json::Error),
    #[error(transparent)]
    Library(#[from] AssetLibraryError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, LibraryLocationError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LibraryAvailability {
    Ready,
    Unavailable,
    Migrating,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryStatusSnapshot {
    pub location_label: String,
    pub is_default: bool,
    pub availability: LibraryAvailability,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredLibraryLocation {
    schema_version: u32,
    library_id: uuid::Uuid,
    #[serde(default)]
    generation: uuid::Uuid,
    root: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LibraryMarker {
    schema_version: u32,
    library_id: uuid::Uuid,
    #[serde(default)]
    generation: uuid::Uuid,
}

pub struct LibraryContext {
    root: PathBuf,
    default_root: PathBuf,
    config_path: PathBuf,
    library_id: uuid::Uuid,
    generation: uuid::Uuid,
    library: Option<AssetLibrary>,
    availability: LibraryAvailability,
}

#[derive(Debug, Clone)]
pub struct LibraryMigrationSource {
    pub root: PathBuf,
    pub library_id: uuid::Uuid,
    pub generation: uuid::Uuid,
}

pub struct PreparedLibraryLocation {
    root: PathBuf,
    library_id: uuid::Uuid,
    generation: uuid::Uuid,
    expected_generation: uuid::Uuid,
    library: AssetLibrary,
    replaced_backup: Option<PathBuf>,
}

impl LibraryContext {
    pub fn open(default_root: PathBuf, config_dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&config_dir)?;
        let config_path = config_dir.join(LOCATION_CONFIG_FILENAME);
        Self::open_with_paths(default_root, config_path)
    }

    fn open_with_paths(default_root: PathBuf, config_path: PathBuf) -> Result<Self> {
        let stored = read_location_config(&config_path)?;
        match stored {
            Some(stored) => {
                validate_stored_location(&stored)?;
                let opened = if stored.generation.is_nil() {
                    open_and_upgrade_legacy_library(&stored.root, stored.library_id)
                } else {
                    open_library_with_identity(&stored.root, stored.library_id, stored.generation)
                        .map(|library| (library, stored.generation))
                };
                let (library, generation) = match opened {
                    Ok((library, generation)) => {
                        if stored.generation.is_nil() {
                            persist_location_config(
                                &config_path,
                                &stored.root,
                                stored.library_id,
                                generation,
                            )?;
                        }
                        (Some(library), generation)
                    }
                    Err(_) => (None, stored.generation),
                };
                let availability = if library.is_some() {
                    LibraryAvailability::Ready
                } else {
                    LibraryAvailability::Unavailable
                };
                Ok(Self {
                    root: stored.root,
                    default_root,
                    config_path,
                    library_id: stored.library_id,
                    generation,
                    library,
                    availability,
                })
            }
            None => {
                let library = AssetLibrary::open(default_root.clone())?;
                let (library_id, generation) = read_marker(&default_root)
                    .and_then(|marker| {
                        let generation = validated_marker_generation(&marker)?;
                        Ok((
                            marker.library_id,
                            if generation.is_nil() {
                                uuid::Uuid::new_v4()
                            } else {
                                generation
                            },
                        ))
                    })
                    .or_else(|error| match error {
                        LibraryLocationError::Io(ref io)
                            if io.kind() == std::io::ErrorKind::NotFound =>
                        {
                            Ok((uuid::Uuid::new_v4(), uuid::Uuid::new_v4()))
                        }
                        _ => Err(error),
                    })?;
                write_marker(&default_root, library_id, generation)?;
                persist_location_config(&config_path, &default_root, library_id, generation)?;
                Ok(Self {
                    root: default_root.clone(),
                    default_root,
                    config_path,
                    library_id,
                    generation,
                    library: Some(library),
                    availability: LibraryAvailability::Ready,
                })
            }
        }
    }

    pub fn status(&mut self) -> LibraryStatusSnapshot {
        self.refresh_ready_state();
        let is_default = self.is_default();
        LibraryStatusSnapshot {
            location_label: location_label(&self.root, is_default),
            is_default,
            availability: self.availability,
        }
    }

    pub fn library(&mut self) -> Result<&AssetLibrary> {
        self.refresh_ready_state();
        match self.availability {
            LibraryAvailability::Ready => self
                .library
                .as_ref()
                .ok_or(LibraryLocationError::Unavailable),
            LibraryAvailability::Unavailable => Err(LibraryLocationError::Unavailable),
            LibraryAvailability::Migrating => Err(LibraryLocationError::Migrating),
        }
    }

    pub fn library_mut(&mut self) -> Result<&mut AssetLibrary> {
        self.refresh_ready_state();
        match self.availability {
            LibraryAvailability::Ready => self
                .library
                .as_mut()
                .ok_or(LibraryLocationError::Unavailable),
            LibraryAvailability::Unavailable => Err(LibraryLocationError::Unavailable),
            LibraryAvailability::Migrating => Err(LibraryLocationError::Migrating),
        }
    }

    pub fn begin_migration(&mut self) -> Result<LibraryMigrationSource> {
        self.library()?;
        if self.generation.is_nil() {
            return Err(LibraryLocationError::LibraryGenerationMismatch);
        }
        let source = LibraryMigrationSource {
            root: self.root.clone(),
            library_id: self.library_id,
            generation: self.generation,
        };
        self.availability = LibraryAvailability::Migrating;
        Ok(source)
    }

    pub fn cancel_migration(&mut self) {
        if self.availability == LibraryAvailability::Migrating {
            self.availability = if self.library.is_some() {
                LibraryAvailability::Ready
            } else {
                LibraryAvailability::Unavailable
            };
        }
    }

    pub fn activate_prepared(&mut self, prepared: PreparedLibraryLocation) -> Result<()> {
        if self.availability != LibraryAvailability::Migrating {
            return Err(LibraryLocationError::Unavailable);
        }
        if prepared.library_id != self.library_id {
            self.cancel_migration();
            return Err(LibraryLocationError::LibraryIdentityMismatch);
        }
        if prepared.expected_generation != self.generation {
            self.cancel_migration();
            return Err(LibraryLocationError::LibraryGenerationMismatch);
        }
        if let Err(error) = persist_location_config(
            &self.config_path,
            &prepared.root,
            prepared.library_id,
            prepared.generation,
        ) {
            let rollback = prepared.rollback_replacement();
            self.cancel_migration();
            if let Err(rollback) = rollback {
                return Err(LibraryLocationError::Io(std::io::Error::other(format!(
                    "could not save the library location ({error}) or restore the previous destination ({rollback})"
                ))));
            }
            return Err(error);
        }
        self.root = prepared.root.clone();
        self.generation = prepared.generation;
        self.library = Some(prepared.library);
        self.availability = LibraryAvailability::Ready;
        if let Some(backup) = prepared.replaced_backup {
            match std::fs::remove_dir_all(&backup) {
                Ok(()) => {
                    if let Some(parent) = backup.parent() {
                        if let Err(error) = sync_directory(parent) {
                            log::warn!("could not sync replaced Kiri library cleanup: {error}");
                        }
                    }
                }
                Err(error) => {
                    log::warn!("could not remove replaced Kiri library backup: {error}")
                }
            }
        }
        Ok(())
    }

    pub fn begin_locating(&mut self) -> Result<()> {
        if self.availability == LibraryAvailability::Migrating {
            return Err(LibraryLocationError::Migrating);
        }
        if self.availability == LibraryAvailability::Ready {
            return Err(LibraryLocationError::InvalidDestination);
        }
        self.availability = LibraryAvailability::Migrating;
        Ok(())
    }

    pub fn cancel_locating(&mut self) {
        if self.availability == LibraryAvailability::Migrating {
            self.availability = LibraryAvailability::Unavailable;
        }
    }

    pub fn expected_library_id(&self) -> uuid::Uuid {
        self.library_id
    }

    pub fn expected_library_generation(&self) -> uuid::Uuid {
        self.generation
    }

    pub fn default_root(&self) -> &Path {
        &self.default_root
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn is_default(&self) -> bool {
        paths_refer_to_same_location(&self.root, &self.default_root)
    }

    pub fn root_matches(&self, candidate: &Path) -> bool {
        paths_refer_to_same_location(&self.root, candidate)
    }

    pub fn retry(&mut self) -> Result<()> {
        match self.availability {
            LibraryAvailability::Ready => return Ok(()),
            LibraryAvailability::Migrating => return Err(LibraryLocationError::Migrating),
            LibraryAvailability::Unavailable => {}
        }
        let (library, generation) = if self.generation.is_nil() {
            let (library, generation) =
                open_and_upgrade_legacy_library(&self.root, self.library_id)?;
            persist_location_config(&self.config_path, &self.root, self.library_id, generation)?;
            (library, generation)
        } else {
            (
                open_library_with_identity(&self.root, self.library_id, self.generation)?,
                self.generation,
            )
        };
        self.generation = generation;
        self.library = Some(library);
        self.availability = LibraryAvailability::Ready;
        Ok(())
    }

    fn refresh_ready_state(&mut self) {
        if self.availability == LibraryAvailability::Ready
            && (self.generation.is_nil()
                || validate_marker_identity(&self.root, self.library_id, self.generation).is_err()
                || self
                    .library
                    .as_ref()
                    .is_none_or(|library| library.validate_storage_layout().is_err()))
        {
            self.library = None;
            self.availability = LibraryAvailability::Unavailable;
        }
    }
}

impl PreparedLibraryLocation {
    fn rollback_replacement(&self) -> Result<()> {
        let Some(backup) = self.replaced_backup.as_ref() else {
            return Ok(());
        };
        rollback_replaced_paths(&self.root, backup)
    }
}

fn rollback_replaced_paths(root: &Path, backup: &Path) -> Result<()> {
    let rollback_copy = backup.with_file_name(format!(
        ".{}.{}.rollback",
        root.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(MANAGED_LIBRARY_DIRECTORY_NAME),
        uuid::Uuid::new_v4().simple()
    ));
    std::fs::rename(root, &rollback_copy)?;
    if let Err(error) = std::fs::rename(backup, root) {
        let restore_new = std::fs::rename(&rollback_copy, root);
        return Err(LibraryLocationError::Io(std::io::Error::other(format!(
            "could not restore the previous destination ({error}); restoring the moved copy: {}",
            restore_new
                .err()
                .map(|error| error.to_string())
                .unwrap_or_else(|| "ok".into())
        ))));
    }
    if let Some(parent) = root.parent() {
        sync_directory(parent)?;
    }
    if let Err(error) = std::fs::remove_dir_all(&rollback_copy) {
        log::warn!("could not remove rolled-back Kiri library copy: {error}");
    }
    Ok(())
}

pub fn target_root_for_container(container: &Path) -> Result<PathBuf> {
    let metadata = std::fs::symlink_metadata(container)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(LibraryLocationError::InvalidDestination);
    }
    let canonical = std::fs::canonicalize(container)?;
    if canonical
        .file_name()
        .is_some_and(|name| name == MANAGED_LIBRARY_DIRECTORY_NAME)
    {
        Ok(canonical)
    } else {
        Ok(canonical.join(MANAGED_LIBRARY_DIRECTORY_NAME))
    }
}

pub fn prepare_existing_location(
    selected: &Path,
    expected_library_id: uuid::Uuid,
    expected_generation: uuid::Uuid,
    expected_root: &Path,
) -> Result<PreparedLibraryLocation> {
    let selected = std::fs::canonicalize(selected)?;
    let nested = selected.join(MANAGED_LIBRARY_DIRECTORY_NAME);
    let root = if selected.join(LIBRARY_MARKER_FILENAME).exists() {
        selected
    } else if nested.join(LIBRARY_MARKER_FILENAME).exists() {
        nested
    } else {
        return Err(LibraryLocationError::InvalidLibrary);
    };
    let (library, generation) = if expected_generation.is_nil() {
        // A v1 configuration has no copy generation. While its remembered root
        // is offline there is no safe way to distinguish that root from an old
        // migration source carrying the same library id, so never bless an
        // arbitrary same-id copy as current.
        if !paths_refer_to_same_location(&root, expected_root) {
            return Err(LibraryLocationError::LibraryGenerationMismatch);
        }
        open_and_upgrade_legacy_library(&root, expected_library_id)?
    } else {
        (
            open_library_with_identity(&root, expected_library_id, expected_generation)?,
            expected_generation,
        )
    };
    Ok(PreparedLibraryLocation {
        root,
        library_id: expected_library_id,
        generation,
        expected_generation,
        library,
        replaced_backup: None,
    })
}

pub fn migrate_library(
    source: &LibraryMigrationSource,
    target_root: &Path,
    replace_same_library: bool,
) -> Result<PreparedLibraryLocation> {
    if source.generation.is_nil() {
        return Err(LibraryLocationError::LibraryGenerationMismatch);
    }
    validate_marker_identity(&source.root, source.library_id, source.generation)?;
    let source_root = std::fs::canonicalize(&source.root)?;
    let target_parent = target_root
        .parent()
        .ok_or(LibraryLocationError::InvalidDestination)?;
    std::fs::create_dir_all(target_parent)?;
    let target_parent = std::fs::canonicalize(target_parent)?;
    let target_name = target_root
        .file_name()
        .ok_or(LibraryLocationError::InvalidDestination)?;
    let target_root = target_parent.join(target_name);

    if target_root == source_root
        || target_parent.starts_with(&source_root)
        || source_root.starts_with(&target_root)
    {
        return Err(LibraryLocationError::InvalidDestination);
    }

    let existing_target = match std::fs::symlink_metadata(&target_root) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(LibraryLocationError::InvalidDestination);
            }
            if !replace_same_library {
                return Err(LibraryLocationError::InvalidDestination);
            }
            validate_marker_lineage(&target_root, source.library_id)?;
            true
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => return Err(error.into()),
    };

    let stem = target_name.to_string_lossy();
    let nonce = uuid::Uuid::new_v4().simple();
    let target_generation = uuid::Uuid::new_v4();
    let staging_root = target_parent.join(format!(".{stem}.{nonce}.staging"));
    let backup_root = target_parent.join(format!(".{stem}.{nonce}.backup"));

    let prepared = (|| -> Result<PreparedLibraryLocation> {
        std::fs::create_dir(&staging_root)?;
        for directory in ["Assets", "Annotations", "Thumbnails"] {
            std::fs::create_dir(staging_root.join(directory))?;
        }
        write_marker(&staging_root, source.library_id, target_generation)?;
        copy_index(&source_root, &staging_root)?;
        copy_flat_directory(&source_root, &staging_root, "Assets")?;
        copy_flat_directory(&source_root, &staging_root, "Annotations")?;
        verify_staged_copy(&source_root, &staging_root)?;
        validate_marker_identity(&staging_root, source.library_id, target_generation)?;
        let staged_library = AssetLibrary::open_existing(staging_root.clone())?;
        if staged_library.all_assets(false).len() + staged_library.all_assets(true).len()
            != source_asset_count(&source_root)?
        {
            return Err(LibraryLocationError::Io(std::io::Error::other(
                "library index count changed during migration",
            )));
        }

        let source_manifest = verify_staged_copy(&source_root, &staging_root)?;
        if existing_target {
            ensure_replace_does_not_drop_unique_content(
                &source_root,
                &target_root,
                &source_manifest,
            )?;
            std::fs::rename(&target_root, &backup_root)?;
            if let Err(error) = std::fs::rename(&staging_root, &target_root) {
                std::fs::rename(&backup_root, &target_root).map_err(|rollback| {
                    LibraryLocationError::Io(std::io::Error::other(format!(
                        "could not install the moved library ({error}) or restore the previous destination ({rollback})"
                    )))
                })?;
                return Err(error.into());
            }
        } else {
            std::fs::rename(&staging_root, &target_root)?;
        }
        let library = match sync_directory(&target_parent).and_then(|()| {
            open_library_with_identity(&target_root, source.library_id, target_generation)
        }) {
            Ok(library) => library,
            Err(error) if existing_target => {
                rollback_replaced_paths(&target_root, &backup_root)?;
                return Err(error);
            }
            Err(error) => return Err(error),
        };
        Ok(PreparedLibraryLocation {
            root: target_root,
            library_id: source.library_id,
            generation: target_generation,
            expected_generation: source.generation,
            library,
            replaced_backup: existing_target.then_some(backup_root.clone()),
        })
    })();

    if staging_root.exists() {
        let _ = std::fs::remove_dir_all(&staging_root);
    }
    prepared
}

fn verify_staged_copy(
    source_root: &Path,
    staging_root: &Path,
) -> Result<Vec<(String, u64, String)>> {
    let source_manifest = migration_manifest(source_root)?;
    if source_manifest != migration_manifest(staging_root)?
        || !files_identical(
            &source_root.join("library.json"),
            &staging_root.join("library.json"),
        )?
    {
        return Err(LibraryLocationError::Io(std::io::Error::other(
            "library migration verification failed",
        )));
    }
    Ok(source_manifest)
}

fn ensure_replace_does_not_drop_unique_content(
    source_root: &Path,
    target_root: &Path,
    source_manifest: &[(String, u64, String)],
) -> Result<()> {
    ensure_no_unknown_root_entries(target_root)?;
    let source_paths = source_manifest
        .iter()
        .map(|(path, _, _)| path.as_str())
        .collect::<HashSet<_>>();
    let target_manifest = migration_manifest(target_root)?;
    if target_manifest
        .iter()
        .any(|entry| !source_paths.contains(entry.0.as_str()) || !source_manifest.contains(entry))
    {
        return Err(LibraryLocationError::InvalidDestination);
    }

    let source_library = AssetLibrary::open_existing(source_root.to_path_buf())?;
    let target_library = AssetLibrary::open_existing(target_root.to_path_buf())?;
    let source_assets = source_library
        .all_assets(false)
        .into_iter()
        .chain(source_library.all_assets(true))
        .collect::<Vec<_>>();
    if target_library
        .all_assets(false)
        .into_iter()
        .chain(target_library.all_assets(true))
        .any(|target| !source_assets.contains(&target))
    {
        return Err(LibraryLocationError::InvalidDestination);
    }
    Ok(())
}

fn ensure_no_unknown_root_entries(root: &Path) -> Result<()> {
    const MANAGED_ENTRIES: &[&str] = &[
        LIBRARY_MARKER_FILENAME,
        "library.json",
        "Assets",
        "Annotations",
        "Thumbnails",
        ".DS_Store",
    ];
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| LibraryLocationError::InvalidDestination)?;
        if !MANAGED_ENTRIES.contains(&name.as_str()) {
            return Err(LibraryLocationError::InvalidDestination);
        }
    }
    Ok(())
}

fn source_asset_count(root: &Path) -> Result<usize> {
    let library = AssetLibrary::open_existing(root.to_path_buf())?;
    Ok(library.all_assets(false).len() + library.all_assets(true).len())
}

fn copy_index(source_root: &Path, target_root: &Path) -> Result<()> {
    let source = source_root.join("library.json");
    let target = target_root.join("library.json");
    match std::fs::symlink_metadata(&source) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(LibraryLocationError::InvalidLibrary);
            }
            let copied = std::fs::copy(&source, &target)?;
            if copied != metadata.len() {
                return Err(LibraryLocationError::Io(std::io::Error::other(
                    "library index changed while copying",
                )));
            }
            std::fs::OpenOptions::new()
                .write(true)
                .open(&target)?
                .sync_all()?;
            sync_directory(target_root)?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            atomic_write(&target, b"[]")?;
        }
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

fn copy_flat_directory(source_root: &Path, target_root: &Path, directory: &str) -> Result<()> {
    let source = source_root.join(directory);
    let target = target_root.join(directory);
    let metadata = std::fs::symlink_metadata(&source)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(LibraryLocationError::InvalidLibrary);
    }
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| LibraryLocationError::InvalidLibrary)?;
        if !is_safe_library_filename(&name) {
            return Err(LibraryLocationError::InvalidLibrary);
        }
        let metadata = std::fs::symlink_metadata(entry.path())?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(LibraryLocationError::InvalidLibrary);
        }
        let copied = std::fs::copy(entry.path(), target.join(&name))?;
        if copied != metadata.len() {
            return Err(LibraryLocationError::Io(std::io::Error::other(
                "library file changed while copying",
            )));
        }
        std::fs::OpenOptions::new()
            .write(true)
            .open(target.join(&name))?
            .sync_all()?;
    }
    sync_directory(&target)?;
    Ok(())
}

fn migration_manifest(root: &Path) -> Result<Vec<(String, u64, String)>> {
    let mut manifest = Vec::new();
    for directory in ["Assets", "Annotations"] {
        let path = root.join(directory);
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| LibraryLocationError::InvalidLibrary)?;
            let metadata = std::fs::symlink_metadata(entry.path())?;
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(LibraryLocationError::InvalidLibrary);
            }
            let (_, sha256) = file_fingerprint(&entry.path())?;
            manifest.push((format!("{directory}/{name}"), metadata.len(), sha256));
        }
    }
    manifest.sort();
    Ok(manifest)
}

fn files_identical(left: &Path, right: &Path) -> Result<bool> {
    let (left_len, left_sha256) = file_fingerprint(left)?;
    let (right_len, right_sha256) = file_fingerprint(right)?;
    Ok(left_len == right_len && left_sha256 == right_sha256)
}

fn file_fingerprint(path: &Path) -> Result<(u64, String)> {
    let metadata = std::fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(LibraryLocationError::InvalidLibrary);
    }
    let mut file = std::fs::File::open(path)?;
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

fn open_library_with_identity(
    root: &Path,
    expected_id: uuid::Uuid,
    expected_generation: uuid::Uuid,
) -> Result<AssetLibrary> {
    validate_marker_identity(root, expected_id, expected_generation)?;
    AssetLibrary::open_existing(root.to_path_buf()).map_err(Into::into)
}

fn open_and_upgrade_legacy_library(
    root: &Path,
    expected_id: uuid::Uuid,
) -> Result<(AssetLibrary, uuid::Uuid)> {
    let marker = read_marker(root)?;
    let generation = validated_marker_generation(&marker)?;
    if marker.library_id != expected_id {
        return Err(LibraryLocationError::LibraryIdentityMismatch);
    }
    let library = AssetLibrary::open_existing(root.to_path_buf())?;
    let generation = if generation.is_nil() {
        let generation = uuid::Uuid::new_v4();
        write_marker(root, expected_id, generation)?;
        generation
    } else {
        generation
    };
    Ok((library, generation))
}

fn validate_marker_lineage(root: &Path, expected_id: uuid::Uuid) -> Result<uuid::Uuid> {
    let marker = read_marker(root)?;
    let generation = validated_marker_generation(&marker)?;
    if marker.library_id != expected_id {
        return Err(LibraryLocationError::LibraryIdentityMismatch);
    }
    Ok(generation)
}

fn validate_marker_identity(
    root: &Path,
    expected_id: uuid::Uuid,
    expected_generation: uuid::Uuid,
) -> Result<()> {
    if expected_generation.is_nil() {
        return Err(LibraryLocationError::LibraryGenerationMismatch);
    }
    let generation = validate_marker_lineage(root, expected_id)?;
    if generation != expected_generation {
        return Err(LibraryLocationError::LibraryGenerationMismatch);
    }
    Ok(())
}

fn validated_marker_generation(marker: &LibraryMarker) -> Result<uuid::Uuid> {
    if marker.library_id.is_nil() {
        return Err(LibraryLocationError::InvalidLibrary);
    }
    match marker.schema_version {
        LEGACY_SCHEMA_VERSION if marker.generation.is_nil() => Ok(uuid::Uuid::nil()),
        MARKER_SCHEMA_VERSION if !marker.generation.is_nil() => Ok(marker.generation),
        _ => Err(LibraryLocationError::InvalidLibrary),
    }
}

fn validate_stored_location(stored: &StoredLibraryLocation) -> Result<()> {
    if stored.library_id.is_nil() || !stored.root.is_absolute() {
        return Err(LibraryLocationError::InvalidLibrary);
    }
    match stored.schema_version {
        LEGACY_SCHEMA_VERSION if stored.generation.is_nil() => Ok(()),
        LOCATION_SCHEMA_VERSION if !stored.generation.is_nil() => Ok(()),
        _ => Err(LibraryLocationError::InvalidLibrary),
    }
}

fn read_marker(root: &Path) -> Result<LibraryMarker> {
    let root_metadata = std::fs::symlink_metadata(root)?;
    if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
        return Err(LibraryLocationError::InvalidLibrary);
    }
    let path = root.join(LIBRARY_MARKER_FILENAME);
    let metadata = std::fs::symlink_metadata(&path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(LibraryLocationError::InvalidLibrary);
    }
    let data = std::fs::read(path)?;
    serde_json::from_slice(&data).map_err(LibraryLocationError::InvalidMarker)
}

fn write_marker(root: &Path, library_id: uuid::Uuid, generation: uuid::Uuid) -> Result<()> {
    if library_id.is_nil() || generation.is_nil() {
        return Err(LibraryLocationError::InvalidLibrary);
    }
    let marker = LibraryMarker {
        schema_version: MARKER_SCHEMA_VERSION,
        library_id,
        generation,
    };
    let data = serde_json::to_vec_pretty(&marker).map_err(LibraryLocationError::InvalidMarker)?;
    atomic_write(&root.join(LIBRARY_MARKER_FILENAME), &data)
}

fn read_location_config(path: &Path) -> Result<Option<StoredLibraryLocation>> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(LibraryLocationError::InvalidLibrary);
            }
            let data = std::fs::read(path)?;
            serde_json::from_slice(&data)
                .map(Some)
                .map_err(LibraryLocationError::InvalidConfiguration)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn persist_location_config(
    path: &Path,
    root: &Path,
    library_id: uuid::Uuid,
    generation: uuid::Uuid,
) -> Result<()> {
    if library_id.is_nil() || generation.is_nil() || !root.is_absolute() {
        return Err(LibraryLocationError::InvalidLibrary);
    }
    let stored = StoredLibraryLocation {
        schema_version: LOCATION_SCHEMA_VERSION,
        library_id,
        generation,
        root: root.to_path_buf(),
    };
    let data =
        serde_json::to_vec_pretty(&stored).map_err(LibraryLocationError::InvalidConfiguration)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    atomic_write(path, &data)
}

fn atomic_write(path: &Path, data: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or(LibraryLocationError::InvalidDestination)?;
    let mut temporary = tempfile::Builder::new()
        .prefix(".library-location-")
        .suffix(".tmp")
        .tempfile_in(parent)?;
    std::io::Write::write_all(temporary.as_file_mut(), data)?;
    std::io::Write::flush(temporary.as_file_mut())?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(path)
        .map_err(|error| LibraryLocationError::Io(error.error))?;
    if let Err(error) = sync_directory(parent) {
        log::warn!("could not sync library location configuration directory: {error}");
    }
    Ok(())
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

fn location_label(root: &Path, is_default: bool) -> String {
    if is_default {
        return "Default".into();
    }
    root.parent()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or(MANAGED_LIBRARY_DIRECTORY_NAME)
        .to_string()
}

fn paths_refer_to_same_location(left: &Path, right: &Path) -> bool {
    left == right
        || std::fs::canonicalize(left)
            .ok()
            .zip(std::fs::canonicalize(right).ok())
            .is_some_and(|(left, right)| left == right)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::asset::CaptureKind;

    fn create_library(root: &Path, id: uuid::Uuid) -> AssetLibrary {
        let mut library = AssetLibrary::open(root.to_path_buf()).unwrap();
        write_marker(root, id, uuid::Uuid::new_v4()).unwrap();
        library
            .import_data(
                b"\x89PNG\r\n\x1a\nfixture",
                CaptureKind::Image,
                "png",
                1,
                1,
                None,
                None,
                Some(1_700_000_000_000.0),
            )
            .unwrap();
        library
    }

    fn current_generation(root: &Path) -> uuid::Uuid {
        let marker = read_marker(root).unwrap();
        validated_marker_generation(&marker).unwrap()
    }

    fn migration_source(root: &Path, id: uuid::Uuid) -> LibraryMigrationSource {
        LibraryMigrationSource {
            root: root.to_path_buf(),
            library_id: id,
            generation: current_generation(root),
        }
    }

    fn persist_legacy_location_config(path: &Path, root: &Path, library_id: uuid::Uuid) {
        let stored = StoredLibraryLocation {
            schema_version: LEGACY_SCHEMA_VERSION,
            library_id,
            generation: uuid::Uuid::nil(),
            root: root.to_path_buf(),
        };
        let data = serde_json::to_vec_pretty(&stored).unwrap();
        atomic_write(path, &data).unwrap();
    }

    fn create_legacy_library(root: &Path, id: uuid::Uuid) -> AssetLibrary {
        let library = AssetLibrary::open(root.to_path_buf()).unwrap();
        let marker = LibraryMarker {
            schema_version: LEGACY_SCHEMA_VERSION,
            library_id: id,
            generation: uuid::Uuid::nil(),
        };
        let data = serde_json::to_vec_pretty(&marker).unwrap();
        atomic_write(&root.join(LIBRARY_MARKER_FILENAME), &data).unwrap();
        library
    }

    #[test]
    fn missing_remembered_custom_root_stays_unavailable() {
        let directory = tempfile::tempdir().unwrap();
        let default_root = directory.path().join("default");
        let missing_root = directory
            .path()
            .join("external")
            .join(MANAGED_LIBRARY_DIRECTORY_NAME);
        let id = uuid::Uuid::new_v4();
        let generation = uuid::Uuid::new_v4();
        let config = directory.path().join("config.json");
        persist_location_config(&config, &missing_root, id, generation).unwrap();

        let mut context = LibraryContext::open_with_paths(default_root.clone(), config).unwrap();
        assert_eq!(
            context.status().availability,
            LibraryAvailability::Unavailable
        );
        assert!(!missing_root.exists());
        assert!(!default_root.exists());
    }

    #[test]
    fn missing_remembered_default_root_stays_unavailable() {
        let directory = tempfile::tempdir().unwrap();
        let default_root = directory.path().join("default");
        let config = directory.path().join("config.json");
        let id = uuid::Uuid::new_v4();
        let generation = uuid::Uuid::new_v4();
        persist_location_config(&config, &default_root, id, generation).unwrap();

        let mut context = LibraryContext::open_with_paths(default_root.clone(), config).unwrap();

        assert_eq!(
            context.status().availability,
            LibraryAvailability::Unavailable
        );
        assert!(context.is_default());
        assert!(!default_root.exists());
    }

    #[test]
    fn marker_identity_is_required_for_existing_locations() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join(MANAGED_LIBRARY_DIRECTORY_NAME);
        let id = uuid::Uuid::new_v4();
        create_library(&root, id);
        let generation = current_generation(&root);

        assert!(prepare_existing_location(&root, id, generation, &root).is_ok());
        assert!(matches!(
            prepare_existing_location(&root, uuid::Uuid::new_v4(), generation, &root),
            Err(LibraryLocationError::LibraryIdentityMismatch)
        ));
    }

    #[test]
    fn cancelled_migration_keeps_the_source_context_ready() {
        let directory = tempfile::tempdir().unwrap();
        let default_root = directory.path().join("default");
        let config = directory.path().join("config.json");
        let mut context = LibraryContext::open_with_paths(default_root, config).unwrap();
        let source_root = context.root().to_path_buf();

        context.begin_migration().unwrap();
        assert_eq!(
            context.status().availability,
            LibraryAvailability::Migrating
        );
        context.cancel_migration();

        assert_eq!(context.status().availability, LibraryAvailability::Ready);
        assert_eq!(context.root(), source_root);
        assert!(context.library().is_ok());
    }

    #[test]
    fn migration_copies_and_verifies_the_whole_library() {
        let directory = tempfile::tempdir().unwrap();
        let source_root = directory.path().join("source");
        let destination = directory.path().join("destination");
        std::fs::create_dir(&destination).unwrap();
        let target = destination.join(MANAGED_LIBRARY_DIRECTORY_NAME);
        let id = uuid::Uuid::new_v4();
        let source_library = create_library(&source_root, id);
        let source_asset = source_library.all_assets(false).remove(0);
        let source_bytes = std::fs::read(source_library.asset_url(&source_asset)).unwrap();

        let source = migration_source(&source_root, id);
        let source_generation = source.generation;
        let prepared = migrate_library(&source, &target, false).unwrap();

        assert!(source_root.exists());
        assert_eq!(prepared.root, std::fs::canonicalize(&target).unwrap());
        assert_eq!(
            std::fs::read(prepared.library.asset_url(&source_asset)).unwrap(),
            source_bytes
        );
        validate_marker_identity(&prepared.root, id, prepared.generation).unwrap();
        assert_ne!(prepared.generation, source_generation);
        validate_marker_identity(&source_root, id, source_generation).unwrap();
    }

    #[test]
    fn replacing_a_same_identity_library_never_drops_unique_assets() {
        let directory = tempfile::tempdir().unwrap();
        let source_root = directory.path().join("source");
        let target_root = directory.path().join("target");
        let id = uuid::Uuid::new_v4();
        create_library(&source_root, id);
        let target_library = create_library(&target_root, id);
        let target_asset = target_library.all_assets(false).remove(0);
        let target_file = target_library.asset_url(&target_asset);

        let result = migrate_library(&migration_source(&source_root, id), &target_root, true);

        assert!(matches!(
            result,
            Err(LibraryLocationError::InvalidDestination)
        ));
        assert!(target_file.exists());
        assert!(target_library.asset_by_id(&target_asset.id).is_some());
    }

    #[test]
    fn replacing_a_same_identity_library_rejects_divergent_asset_bytes() {
        let directory = tempfile::tempdir().unwrap();
        let source_root = directory.path().join("source");
        let target_root = directory.path().join("target");
        let id = uuid::Uuid::new_v4();
        let source_library = create_library(&source_root, id);
        let asset = source_library.all_assets(false).remove(0);
        migrate_library(&migration_source(&source_root, id), &target_root, false).unwrap();
        let target_bytes = std::fs::read(target_root.join("Assets").join(&asset.filename)).unwrap();
        let mut changed = target_bytes.clone();
        *changed.last_mut().unwrap() ^= 0xff;
        std::fs::write(source_library.asset_url(&asset), changed).unwrap();

        let result = migrate_library(&migration_source(&source_root, id), &target_root, true);

        assert!(matches!(
            result,
            Err(LibraryLocationError::InvalidDestination)
        ));
        assert_eq!(
            std::fs::read(target_root.join("Assets").join(asset.filename)).unwrap(),
            target_bytes
        );
    }

    #[test]
    fn replacing_a_same_identity_library_preserves_unknown_target_files() {
        let directory = tempfile::tempdir().unwrap();
        let source_root = directory.path().join("source");
        let target_root = directory.path().join("target");
        let id = uuid::Uuid::new_v4();
        create_library(&source_root, id);
        migrate_library(&migration_source(&source_root, id), &target_root, false).unwrap();
        let unknown = target_root.join("notes.txt");
        std::fs::write(&unknown, b"keep me").unwrap();

        assert!(matches!(
            migrate_library(&migration_source(&source_root, id), &target_root, true,),
            Err(LibraryLocationError::InvalidDestination)
        ));
        assert_eq!(std::fs::read(unknown).unwrap(), b"keep me");
    }

    #[test]
    fn migration_rejects_nested_destinations_and_non_regular_files() {
        let directory = tempfile::tempdir().unwrap();
        let source_root = directory.path().join("source");
        let id = uuid::Uuid::new_v4();
        create_library(&source_root, id);
        let source = migration_source(&source_root, id);
        assert!(matches!(
            migrate_library(&source, &source_root.join("nested"), false),
            Err(LibraryLocationError::InvalidDestination)
        ));

        std::fs::create_dir(source_root.join("Assets").join("unexpected-directory")).unwrap();
        let target = directory.path().join("target");
        assert!(migrate_library(&source, &target, false).is_err());
        assert!(!target.exists());
    }

    #[test]
    fn located_library_can_replace_a_missing_path_without_fallback() {
        let directory = tempfile::tempdir().unwrap();
        let default_root = directory.path().join("default");
        let old_root = directory
            .path()
            .join("missing")
            .join(MANAGED_LIBRARY_DIRECTORY_NAME);
        let new_root = directory
            .path()
            .join("attached")
            .join(MANAGED_LIBRARY_DIRECTORY_NAME);
        let id = uuid::Uuid::new_v4();
        create_library(&new_root, id);
        let generation = current_generation(&new_root);
        let config = directory.path().join("config.json");
        persist_location_config(&config, &old_root, id, generation).unwrap();
        let mut context = LibraryContext::open_with_paths(default_root, config).unwrap();
        context.begin_locating().unwrap();
        let prepared = prepare_existing_location(&new_root, id, generation, &old_root).unwrap();
        context.activate_prepared(prepared).unwrap();

        assert_eq!(context.status().availability, LibraryAvailability::Ready);
        assert_eq!(context.root(), std::fs::canonicalize(&new_root).unwrap());
    }

    #[test]
    fn migration_generation_rejects_the_old_source_during_locate() {
        let directory = tempfile::tempdir().unwrap();
        let source_root = directory.path().join("source");
        let target_root = directory.path().join("target");
        let id = uuid::Uuid::new_v4();
        create_library(&source_root, id);
        let source = migration_source(&source_root, id);
        let prepared = migrate_library(&source, &target_root, false).unwrap();

        assert!(
            prepare_existing_location(&target_root, id, prepared.generation, &target_root).is_ok()
        );
        assert!(matches!(
            prepare_existing_location(&source_root, id, prepared.generation, &target_root),
            Err(LibraryLocationError::LibraryGenerationMismatch)
        ));
    }

    #[test]
    fn online_v1_location_is_upgraded_to_a_durable_generation() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("legacy");
        let default_root = directory.path().join("default");
        let config = directory.path().join("config.json");
        let id = uuid::Uuid::new_v4();
        create_legacy_library(&root, id);
        persist_legacy_location_config(&config, &root, id);

        let mut context = LibraryContext::open_with_paths(default_root, config.clone()).unwrap();
        assert_eq!(context.status().availability, LibraryAvailability::Ready);
        assert!(!context.expected_library_generation().is_nil());
        validate_marker_identity(&root, id, context.expected_library_generation()).unwrap();
        let stored = read_location_config(&config).unwrap().unwrap();
        assert_eq!(stored.schema_version, LOCATION_SCHEMA_VERSION);
        assert_eq!(stored.generation, context.expected_library_generation());
    }

    #[test]
    fn offline_v1_location_rejects_an_arbitrary_same_id_copy() {
        let directory = tempfile::tempdir().unwrap();
        let missing_root = directory.path().join("missing");
        let candidate = directory.path().join("old-copy");
        let default_root = directory.path().join("default");
        let config = directory.path().join("config.json");
        let id = uuid::Uuid::new_v4();
        create_legacy_library(&candidate, id);
        persist_legacy_location_config(&config, &missing_root, id);
        let mut context = LibraryContext::open_with_paths(default_root, config).unwrap();
        assert_eq!(
            context.status().availability,
            LibraryAvailability::Unavailable
        );
        assert!(context.expected_library_generation().is_nil());

        assert!(matches!(
            prepare_existing_location(
                &candidate,
                id,
                context.expected_library_generation(),
                context.root()
            ),
            Err(LibraryLocationError::LibraryGenerationMismatch)
        ));
    }
}
