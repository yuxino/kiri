use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

use crate::core::ocr_provider::{
    CredentialMetadataChange, OcrEngineRef, OcrProviderError, OcrProviderPreset,
    OcrProviderProfileMetadata, OcrProviderProtocol, OcrProviderStore,
    PersistedOcrProviderSettings, SaveProfileMetadata, OCR_PROVIDER_SCHEMA_VERSION,
};

const CREDENTIAL_SERVICE: &str = "io.yuxino.kiri.ocr-provider";
const CREDENTIAL_JOURNAL_FILENAME: &str = "ocr-provider-credential-journal.json";
const CREDENTIAL_JOURNAL_SCHEMA_VERSION: u32 = 1;
const CREDENTIAL_WARNING: &str = "Some OCR credentials could not be read from system storage.";
const CREDENTIAL_CLEANUP_WARNING: &str =
    "An old OCR credential could not be removed from system storage.";
const SETTINGS_UNAVAILABLE_WARNING: &str =
    "Remote OCR settings are unavailable. Local OCR remains enabled.";
const PENDING_TTL: Duration = Duration::from_secs(5 * 60);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrProviderProfileDto {
    pub id: String,
    pub revision: u64,
    pub name: String,
    pub provider: OcrProviderPreset,
    pub protocol: OcrProviderProtocol,
    pub base_url: String,
    pub model: String,
    pub has_api_key: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrProviderSettingsDto {
    pub schema_version: u32,
    pub active_engine: OcrEngineRef,
    pub profiles: Vec<OcrProviderProfileDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

/// Intentionally does not derive `Debug`: it can contain an API key.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveOcrProviderProfileRequest {
    pub id: Option<String>,
    pub revision: Option<u64>,
    pub name: String,
    pub provider: OcrProviderPreset,
    pub protocol: OcrProviderProtocol,
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrProfileDisclosureDto {
    pub id: String,
    pub revision: u64,
    pub name: String,
    pub provider: OcrProviderPreset,
    pub origin: String,
    pub model: String,
    pub has_api_key: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedOcrRequestDto {
    pub request_id: String,
    pub engine: OcrEngineRef,
    pub image_width: u32,
    pub image_height: u32,
    pub byte_length: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<OcrProfileDisclosureDto>,
}

pub trait SecretStore: Send + Sync {
    fn get(&self, account: &str) -> Result<Option<SecretString>, CredentialError>;
    fn set(&self, account: &str, secret: &SecretString) -> Result<(), CredentialError>;
    fn delete(&self, account: &str) -> Result<(), CredentialError>;
}

pub struct SystemSecretStore;

impl SecretStore for SystemSecretStore {
    fn get(&self, account: &str) -> Result<Option<SecretString>, CredentialError> {
        #[cfg(any(target_os = "macos", windows))]
        {
            let entry = keyring::Entry::new(CREDENTIAL_SERVICE, account)
                .map_err(|_| CredentialError::Unavailable)?;
            match entry.get_password() {
                Ok(value) => Ok(Some(value.into())),
                Err(keyring::Error::NoEntry) => Ok(None),
                Err(_) => Err(CredentialError::Unavailable),
            }
        }
        #[cfg(not(any(target_os = "macos", windows)))]
        {
            let _ = account;
            Err(CredentialError::Unavailable)
        }
    }

    fn set(&self, account: &str, secret: &SecretString) -> Result<(), CredentialError> {
        #[cfg(any(target_os = "macos", windows))]
        {
            let entry = keyring::Entry::new(CREDENTIAL_SERVICE, account)
                .map_err(|_| CredentialError::Unavailable)?;
            entry
                .set_password(secret.expose_secret())
                .map_err(|_| CredentialError::Unavailable)
        }
        #[cfg(not(any(target_os = "macos", windows)))]
        {
            let _ = (account, secret);
            Err(CredentialError::Unavailable)
        }
    }

    fn delete(&self, account: &str) -> Result<(), CredentialError> {
        #[cfg(any(target_os = "macos", windows))]
        {
            let entry = keyring::Entry::new(CREDENTIAL_SERVICE, account)
                .map_err(|_| CredentialError::Unavailable)?;
            match entry.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
                Err(_) => Err(CredentialError::Unavailable),
            }
        }
        #[cfg(not(any(target_os = "macos", windows)))]
        {
            let _ = account;
            Err(CredentialError::Unavailable)
        }
    }
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum CredentialError {
    #[error("system credential storage is unavailable")]
    Unavailable,
}

enum ProviderManagerState {
    Ready {
        store: OcrProviderStore,
        warning: Option<String>,
    },
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CredentialJournal {
    schema_version: u32,
    operation: CredentialJournalOperation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
enum CredentialJournalOperation {
    Replace {
        profile_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        old_credential_id: Option<String>,
        new_credential_id: String,
    },
    Delete {
        profile_id: String,
        credential_id: String,
    },
}

struct CredentialJournalStore {
    path: PathBuf,
}

impl CredentialJournalStore {
    fn new(config_dir: &Path) -> Self {
        Self {
            path: config_dir.join(CREDENTIAL_JOURNAL_FILENAME),
        }
    }

    fn load(&self) -> Result<Option<CredentialJournal>, OcrManagerError> {
        let bytes = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(OcrManagerError::StorageUnavailable),
        };
        let journal: CredentialJournal =
            serde_json::from_slice(&bytes).map_err(|_| OcrManagerError::StorageUnavailable)?;
        validate_credential_journal(&journal)?;
        Ok(Some(journal))
    }

    fn write(&self, operation: CredentialJournalOperation) -> Result<(), OcrManagerError> {
        let journal = CredentialJournal {
            schema_version: CREDENTIAL_JOURNAL_SCHEMA_VERSION,
            operation,
        };
        validate_credential_journal(&journal)?;
        let parent = self
            .path
            .parent()
            .ok_or(OcrManagerError::StorageUnavailable)?;
        fs::create_dir_all(parent).map_err(|_| OcrManagerError::StorageUnavailable)?;
        let mut temporary = tempfile::NamedTempFile::new_in(parent)
            .map_err(|_| OcrManagerError::StorageUnavailable)?;
        serde_json::to_writer_pretty(temporary.as_file_mut(), &journal)
            .map_err(|_| OcrManagerError::StorageUnavailable)?;
        temporary
            .as_file_mut()
            .write_all(b"\n")
            .map_err(|_| OcrManagerError::StorageUnavailable)?;
        temporary
            .as_file_mut()
            .sync_all()
            .map_err(|_| OcrManagerError::StorageUnavailable)?;
        temporary
            .persist(&self.path)
            .map_err(|_| OcrManagerError::StorageUnavailable)?;
        sync_parent_directory(parent)?;
        Ok(())
    }

    fn clear(&self) -> Result<(), OcrManagerError> {
        match fs::remove_file(&self.path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(_) => return Err(OcrManagerError::StorageUnavailable),
        }
        let parent = self
            .path
            .parent()
            .ok_or(OcrManagerError::StorageUnavailable)?;
        sync_parent_directory(parent)
    }
}

fn sync_parent_directory(parent: &Path) -> Result<(), OcrManagerError> {
    #[cfg(unix)]
    fs::File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| OcrManagerError::StorageUnavailable)?;
    #[cfg(not(unix))]
    let _ = parent;
    Ok(())
}

fn validate_credential_journal(journal: &CredentialJournal) -> Result<(), OcrManagerError> {
    if journal.schema_version != CREDENTIAL_JOURNAL_SCHEMA_VERSION {
        return Err(OcrManagerError::StorageUnavailable);
    }
    let valid_id = |value: &str| {
        uuid::Uuid::parse_str(value)
            .map(|id| id.to_string() == value)
            .unwrap_or(false)
    };
    match &journal.operation {
        CredentialJournalOperation::Replace {
            profile_id,
            old_credential_id,
            new_credential_id,
        } => {
            if !valid_id(profile_id)
                || !valid_id(new_credential_id)
                || old_credential_id
                    .as_deref()
                    .is_some_and(|id| !valid_id(id) || id == new_credential_id)
            {
                return Err(OcrManagerError::StorageUnavailable);
            }
        }
        CredentialJournalOperation::Delete {
            profile_id,
            credential_id,
        } => {
            if !valid_id(profile_id) || !valid_id(credential_id) {
                return Err(OcrManagerError::StorageUnavailable);
            }
        }
    }
    Ok(())
}

fn reconcile_credential_journal(
    journal_store: &CredentialJournalStore,
    provider_store: &OcrProviderStore,
    secrets: &dyn SecretStore,
) -> Result<(), OcrManagerError> {
    let Some(journal) = journal_store.load()? else {
        return Ok(());
    };
    match journal.operation {
        CredentialJournalOperation::Replace {
            profile_id,
            old_credential_id,
            new_credential_id,
        } => {
            let metadata_points_to_new = provider_store
                .profile(&profile_id)
                .and_then(|profile| profile.credential_id.as_deref())
                == Some(new_credential_id.as_str());
            if metadata_points_to_new {
                if let Some(old_credential_id) = old_credential_id {
                    secrets.delete(&credential_account(&profile_id, &old_credential_id))?;
                }
            } else {
                secrets.delete(&credential_account(&profile_id, &new_credential_id))?;
            }
        }
        CredentialJournalOperation::Delete {
            profile_id,
            credential_id,
        } => {
            let metadata_still_points_to_old = provider_store
                .profile(&profile_id)
                .and_then(|profile| profile.credential_id.as_deref())
                == Some(credential_id.as_str());
            if !metadata_still_points_to_old {
                secrets.delete(&credential_account(&profile_id, &credential_id))?;
            }
        }
    }
    journal_store.clear()
}

pub struct OcrProviderManager {
    state: Mutex<ProviderManagerState>,
    secrets: Arc<dyn SecretStore>,
    journal: Option<CredentialJournalStore>,
}

impl OcrProviderManager {
    pub fn open(config_dir: &Path) -> Self {
        Self::with_secret_store(config_dir, Arc::new(SystemSecretStore))
    }

    fn with_secret_store(config_dir: &Path, secrets: Arc<dyn SecretStore>) -> Self {
        let journal = CredentialJournalStore::new(config_dir);
        let state = match OcrProviderStore::open(config_dir) {
            Ok(store) => {
                let warning = reconcile_credential_journal(&journal, &store, secrets.as_ref())
                    .err()
                    .map(|_| CREDENTIAL_CLEANUP_WARNING.into());
                ProviderManagerState::Ready { store, warning }
            }
            Err(_) => ProviderManagerState::Unavailable,
        };
        Self {
            state: Mutex::new(state),
            secrets,
            journal: Some(journal),
        }
    }

    pub fn unavailable() -> Self {
        Self {
            state: Mutex::new(ProviderManagerState::Unavailable),
            secrets: Arc::new(SystemSecretStore),
            journal: None,
        }
    }

    fn reconcile_pending_journal(&self, store: &OcrProviderStore) -> Result<(), OcrManagerError> {
        let journal = self
            .journal
            .as_ref()
            .ok_or(OcrManagerError::StorageUnavailable)?;
        reconcile_credential_journal(journal, store, self.secrets.as_ref())
    }

    pub fn settings(&self) -> OcrProviderSettingsDto {
        let mut state = self.state.lock().unwrap();
        match &mut *state {
            ProviderManagerState::Ready { store, warning } => {
                let mut settings =
                    settings_dto(store.settings(), self.secrets.as_ref(), warning.clone());
                if settings.active_engine == OcrEngineRef::Local
                    && store.settings().active_engine != OcrEngineRef::Local
                    && store
                        .plan_set_active(OcrEngineRef::Local)
                        .and_then(|next| store.commit(next))
                        .is_err()
                {
                    *warning = Some(SETTINGS_UNAVAILABLE_WARNING.into());
                    settings.warning = warning.clone();
                }
                settings
            }
            ProviderManagerState::Unavailable => unavailable_settings(),
        }
    }

    pub fn save(
        &self,
        request: SaveOcrProviderProfileRequest,
    ) -> Result<OcrProviderSettingsDto, OcrManagerError> {
        let SaveOcrProviderProfileRequest {
            id,
            revision,
            name,
            provider,
            protocol,
            base_url,
            model,
            api_key,
        } = request;
        let replacement_secret = match api_key {
            None => None,
            Some(value) if value.is_empty() => None,
            Some(value) => Some(validate_api_key(value)?),
        };
        let new_credential_id = replacement_secret
            .as_ref()
            .map(|_| uuid::Uuid::new_v4().to_string());

        let mut state = self.state.lock().unwrap();
        let ProviderManagerState::Ready { store, warning } = &mut *state else {
            return Err(OcrManagerError::SettingsUnavailable);
        };
        if let Err(error) = self.reconcile_pending_journal(store) {
            *warning = Some(CREDENTIAL_CLEANUP_WARNING.into());
            return Err(error);
        }

        let old_credential_id = id
            .as_deref()
            .and_then(|profile_id| store.profile(profile_id))
            .and_then(|profile| profile.credential_id.clone());
        let credential_change = new_credential_id
            .clone()
            .map(CredentialMetadataChange::Replace)
            .unwrap_or(CredentialMetadataChange::Preserve);
        let metadata = SaveProfileMetadata {
            id,
            revision,
            name,
            provider,
            protocol,
            base_url,
            model,
        };
        let (next, profile) = store.plan_save(metadata, credential_change)?;

        if let (Some(secret), Some(new_credential_id)) =
            (replacement_secret.as_ref(), new_credential_id.as_ref())
        {
            let journal = self
                .journal
                .as_ref()
                .ok_or(OcrManagerError::StorageUnavailable)?;
            journal.write(CredentialJournalOperation::Replace {
                profile_id: profile.id.clone(),
                old_credential_id,
                new_credential_id: new_credential_id.clone(),
            })?;

            let new_account = credential_account(&profile.id, new_credential_id);
            if let Err(error) = self.secrets.set(&new_account, secret) {
                if self.reconcile_pending_journal(store).is_err() {
                    *warning = Some(CREDENTIAL_CLEANUP_WARNING.into());
                }
                return Err(error.into());
            }

            if let Err(error) = store.commit(next) {
                if self.reconcile_pending_journal(store).is_err() {
                    *warning = Some(CREDENTIAL_CLEANUP_WARNING.into());
                }
                return Err(error.into());
            }
            if self.reconcile_pending_journal(store).is_err() {
                *warning = Some(CREDENTIAL_CLEANUP_WARNING.into());
            }
        } else {
            store.commit(next)?;
        }
        Ok(settings_dto(
            store.settings(),
            self.secrets.as_ref(),
            warning.clone(),
        ))
    }

    pub fn delete(
        &self,
        profile_id: &str,
        revision: u64,
    ) -> Result<OcrProviderSettingsDto, OcrManagerError> {
        let mut state = self.state.lock().unwrap();
        let ProviderManagerState::Ready { store, warning } = &mut *state else {
            return Err(OcrManagerError::SettingsUnavailable);
        };
        if let Err(error) = self.reconcile_pending_journal(store) {
            *warning = Some(CREDENTIAL_CLEANUP_WARNING.into());
            return Err(error);
        }
        let (next, profile) = store.plan_delete(profile_id, revision)?;
        if let Some(credential_id) = &profile.credential_id {
            let journal = self
                .journal
                .as_ref()
                .ok_or(OcrManagerError::StorageUnavailable)?;
            journal.write(CredentialJournalOperation::Delete {
                profile_id: profile.id.clone(),
                credential_id: credential_id.clone(),
            })?;
            if let Err(error) = store.commit(next) {
                if self.reconcile_pending_journal(store).is_err() {
                    *warning = Some(CREDENTIAL_CLEANUP_WARNING.into());
                }
                return Err(error.into());
            }
            if self.reconcile_pending_journal(store).is_err() {
                *warning = Some(CREDENTIAL_CLEANUP_WARNING.into());
            }
        } else {
            store.commit(next)?;
        }
        Ok(settings_dto(
            store.settings(),
            self.secrets.as_ref(),
            warning.clone(),
        ))
    }

    pub fn set_active(
        &self,
        active_engine: OcrEngineRef,
    ) -> Result<OcrProviderSettingsDto, OcrManagerError> {
        let mut state = self.state.lock().unwrap();
        let ProviderManagerState::Ready { store, warning } = &mut *state else {
            return Err(OcrManagerError::SettingsUnavailable);
        };
        if let OcrEngineRef::Profile { profile_id } = &active_engine {
            let profile = store
                .profile(profile_id)
                .ok_or(OcrManagerError::ProfileNotFound)?;
            let credential_id = profile
                .credential_id
                .as_deref()
                .ok_or(OcrManagerError::CredentialMissing)?;
            if self
                .secrets
                .get(&credential_account(&profile.id, credential_id))?
                .is_none()
            {
                return Err(OcrManagerError::CredentialMissing);
            }
        }
        let next = store.plan_set_active(active_engine)?;
        store.commit(next)?;
        Ok(settings_dto(
            store.settings(),
            self.secrets.as_ref(),
            warning.clone(),
        ))
    }

    pub fn prepared_engine(&self) -> Result<PreparedEngine, OcrManagerError> {
        let mut state = self.state.lock().unwrap();
        let ProviderManagerState::Ready { store, warning } = &mut *state else {
            return Ok(PreparedEngine::Local);
        };
        match store.settings().active_engine.clone() {
            OcrEngineRef::Local => Ok(PreparedEngine::Local),
            OcrEngineRef::Profile { profile_id } => {
                let profile = store
                    .profile(&profile_id)
                    .cloned()
                    .ok_or(OcrManagerError::ProfileNotFound)?;
                let has_api_key = match &profile.credential_id {
                    Some(credential_id) => self
                        .secrets
                        .get(&credential_account(&profile.id, credential_id))
                        .map(|secret| secret.is_some()),
                    None => Ok(false),
                };
                match has_api_key {
                    Ok(true) => Ok(PreparedEngine::Profile {
                        profile,
                        has_api_key: true,
                    }),
                    _ => {
                        *warning = Some(CREDENTIAL_WARNING.into());
                        if store
                            .plan_set_active(OcrEngineRef::Local)
                            .and_then(|next| store.commit(next))
                            .is_err()
                        {
                            *warning = Some(SETTINGS_UNAVAILABLE_WARNING.into());
                        }
                        Ok(PreparedEngine::Local)
                    }
                }
            }
        }
    }

    pub fn resolve_remote(
        &self,
        profile_id: &str,
        revision: u64,
    ) -> Result<ResolvedRemoteProfile, OcrManagerError> {
        let state = self.state.lock().unwrap();
        let ProviderManagerState::Ready { store, .. } = &*state else {
            return Err(OcrManagerError::SettingsUnavailable);
        };
        let profile = store
            .profile(profile_id)
            .filter(|profile| profile.revision == revision)
            .cloned()
            .ok_or(OcrManagerError::RevisionConflict)?;
        let credential_id = profile
            .credential_id
            .as_deref()
            .ok_or(OcrManagerError::CredentialMissing)?;
        let api_key = self
            .secrets
            .get(&credential_account(&profile.id, credential_id))?
            .ok_or(OcrManagerError::CredentialMissing)?;
        Ok(ResolvedRemoteProfile { profile, api_key })
    }
}

pub struct ResolvedRemoteProfile {
    pub profile: OcrProviderProfileMetadata,
    pub api_key: SecretString,
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum OcrManagerError {
    #[error("remote OCR settings are unavailable")]
    SettingsUnavailable,
    #[error("OCR provider profile was not found")]
    ProfileNotFound,
    #[error("OCR provider profile changed; prepare the request again")]
    RevisionConflict,
    #[error("OCR provider credential is missing")]
    CredentialMissing,
    #[error("OCR provider credential is invalid")]
    InvalidCredential,
    #[error("system credential storage is unavailable")]
    CredentialUnavailable,
    #[error("OCR provider settings could not be saved")]
    StorageUnavailable,
    #[error("OCR provider request is invalid")]
    InvalidRequest,
}

impl From<OcrProviderError> for OcrManagerError {
    fn from(value: OcrProviderError) -> Self {
        match value {
            OcrProviderError::StorageUnavailable | OcrProviderError::InvalidSettings => {
                Self::StorageUnavailable
            }
            OcrProviderError::InvalidRequest => Self::InvalidRequest,
            OcrProviderError::ProfileNotFound => Self::ProfileNotFound,
            OcrProviderError::RevisionConflict => Self::RevisionConflict,
        }
    }
}

impl From<CredentialError> for OcrManagerError {
    fn from(_: CredentialError) -> Self {
        Self::CredentialUnavailable
    }
}

fn validate_api_key(value: String) -> Result<SecretString, OcrManagerError> {
    // Windows Credential Manager limits the stored blob to 2560 bytes. Use
    // the strictest supported target limit everywhere so profiles remain
    // portable between Kiri installations.
    let utf16_bytes = value
        .encode_utf16()
        .count()
        .checked_mul(2)
        .ok_or(OcrManagerError::InvalidCredential)?;
    if value.trim() != value
        || value.is_empty()
        || utf16_bytes > 2_560
        || value.chars().any(char::is_control)
    {
        return Err(OcrManagerError::InvalidCredential);
    }
    Ok(value.into())
}

fn credential_account(profile_id: &str, credential_id: &str) -> String {
    format!("{profile_id}:{credential_id}")
}

fn unavailable_settings() -> OcrProviderSettingsDto {
    OcrProviderSettingsDto {
        schema_version: OCR_PROVIDER_SCHEMA_VERSION,
        active_engine: OcrEngineRef::Local,
        profiles: Vec::new(),
        warning: Some(SETTINGS_UNAVAILABLE_WARNING.into()),
    }
}

fn settings_dto(
    settings: &PersistedOcrProviderSettings,
    secrets: &dyn SecretStore,
    mut warning: Option<String>,
) -> OcrProviderSettingsDto {
    let profiles: Vec<OcrProviderProfileDto> = settings
        .profiles
        .iter()
        .map(|profile| {
            let has_api_key = profile
                .credential_id
                .as_ref()
                .map(|credential_id| secrets.get(&credential_account(&profile.id, credential_id)))
                .transpose()
                .map(|secret| secret.flatten().is_some())
                .unwrap_or_else(|_| {
                    warning = Some(CREDENTIAL_WARNING.into());
                    false
                });
            OcrProviderProfileDto {
                id: profile.id.clone(),
                revision: profile.revision,
                name: profile.name.clone(),
                provider: profile.provider,
                protocol: profile.protocol,
                base_url: profile.base_url.clone(),
                model: profile.model.clone(),
                has_api_key,
            }
        })
        .collect();
    let active_engine = match &settings.active_engine {
        OcrEngineRef::Profile { profile_id }
            if profiles
                .iter()
                .any(|profile| profile.id == *profile_id && profile.has_api_key) =>
        {
            settings.active_engine.clone()
        }
        OcrEngineRef::Profile { .. } => {
            warning.get_or_insert_with(|| CREDENTIAL_WARNING.into());
            OcrEngineRef::Local
        }
        OcrEngineRef::Local => OcrEngineRef::Local,
    };
    OcrProviderSettingsDto {
        schema_version: settings.schema_version,
        active_engine,
        profiles,
        warning,
    }
}

#[derive(Clone)]
pub enum PreparedEngine {
    Local,
    Profile {
        profile: OcrProviderProfileMetadata,
        has_api_key: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OcrRequestOwner {
    pub label: String,
    pub capture_id: uuid::Uuid,
}

impl PreparedEngine {
    fn dto(&self) -> Result<(OcrEngineRef, Option<OcrProfileDisclosureDto>), OcrManagerError> {
        match self {
            Self::Local => Ok((OcrEngineRef::Local, None)),
            Self::Profile {
                profile,
                has_api_key,
            } => Ok((
                OcrEngineRef::Profile {
                    profile_id: profile.id.clone(),
                },
                Some(OcrProfileDisclosureDto {
                    id: profile.id.clone(),
                    revision: profile.revision,
                    name: profile.name.clone(),
                    provider: profile.provider,
                    origin: profile.origin()?,
                    model: profile.model.clone(),
                    has_api_key: *has_api_key,
                }),
            )),
        }
    }
}

struct PendingOcrRequest {
    owner: OcrRequestOwner,
    created_at: Instant,
    in_flight: bool,
    cancellation: CancellationToken,
    png: Arc<[u8]>,
    engine: PreparedEngine,
}

pub struct OcrRequestLease {
    pub request_id: String,
    pub png: Arc<[u8]>,
    pub engine: PreparedEngine,
    pub cancellation: CancellationToken,
}

pub struct OcrRequestController {
    pending: Mutex<HashMap<String, PendingOcrRequest>>,
    ttl: Duration,
}

impl Default for OcrRequestController {
    fn default() -> Self {
        Self {
            pending: Mutex::new(HashMap::new()),
            ttl: PENDING_TTL,
        }
    }
}

impl OcrRequestController {
    pub fn prepare(
        self: &Arc<Self>,
        owner: OcrRequestOwner,
        png: Vec<u8>,
        image_width: u32,
        image_height: u32,
        engine: PreparedEngine,
    ) -> Result<PreparedOcrRequestDto, OcrRequestError> {
        if owner.label.is_empty() || png.is_empty() || image_width == 0 || image_height == 0 {
            return Err(OcrRequestError::InvalidRequest);
        }
        let request_id = uuid::Uuid::new_v4().to_string();
        let byte_length = png.len();
        let (engine_dto, profile) = engine.dto().map_err(|_| OcrRequestError::InvalidRequest)?;
        let mut pending = self.pending.lock().unwrap();
        purge_expired(&mut pending, self.ttl);
        pending.retain(|_, request| {
            let keep = request.owner != owner;
            if !keep {
                request.cancellation.cancel();
            }
            keep
        });
        pending.insert(
            request_id.clone(),
            PendingOcrRequest {
                owner,
                created_at: Instant::now(),
                in_flight: false,
                cancellation: CancellationToken::new(),
                png: png.into(),
                engine,
            },
        );
        drop(pending);
        schedule_expiry(Arc::downgrade(self), request_id.clone(), self.ttl);
        Ok(PreparedOcrRequestDto {
            request_id,
            engine: engine_dto,
            image_width,
            image_height,
            byte_length,
            profile,
        })
    }

    pub fn begin(
        &self,
        owner: &OcrRequestOwner,
        request_id: &str,
    ) -> Result<OcrRequestLease, OcrRequestError> {
        let mut pending = self.pending.lock().unwrap();
        purge_expired(&mut pending, self.ttl);
        let request = pending
            .get_mut(request_id)
            .ok_or(OcrRequestError::NotFound)?;
        if &request.owner != owner {
            return Err(OcrRequestError::WrongOwner);
        }
        if request.in_flight {
            return Err(OcrRequestError::Busy);
        }
        request.in_flight = true;
        Ok(OcrRequestLease {
            request_id: request_id.into(),
            png: request.png.clone(),
            engine: request.engine.clone(),
            cancellation: request.cancellation.clone(),
        })
    }

    pub fn complete(&self, lease: &OcrRequestLease) {
        self.pending.lock().unwrap().remove(&lease.request_id);
    }

    pub fn restore_after_failure(&self, lease: &OcrRequestLease) {
        let mut pending = self.pending.lock().unwrap();
        if let Some(request) = pending.get_mut(&lease.request_id) {
            if request.created_at.elapsed() < self.ttl {
                request.in_flight = false;
            } else {
                pending.remove(&lease.request_id);
            }
        }
    }

    pub fn cancel(&self, owner: &OcrRequestOwner, request_id: &str) -> Result<(), OcrRequestError> {
        let mut pending = self.pending.lock().unwrap();
        purge_expired(&mut pending, self.ttl);
        match pending.get(request_id) {
            Some(request) if &request.owner != owner => Err(OcrRequestError::WrongOwner),
            Some(_) => {
                if let Some(request) = pending.remove(request_id) {
                    request.cancellation.cancel();
                }
                Ok(())
            }
            None => Ok(()),
        }
    }

    pub fn clear_owner(&self, owner: &OcrRequestOwner) {
        self.pending.lock().unwrap().retain(|_, request| {
            let keep = &request.owner != owner;
            if !keep {
                request.cancellation.cancel();
            }
            keep
        });
    }

    #[cfg(test)]
    fn with_ttl(ttl: Duration) -> Arc<Self> {
        Arc::new(Self {
            pending: Mutex::new(HashMap::new()),
            ttl,
        })
    }
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum OcrRequestError {
    #[error("prepared OCR request is invalid")]
    InvalidRequest,
    #[error("prepared OCR request was not found or expired")]
    NotFound,
    #[error("prepared OCR request belongs to another window")]
    WrongOwner,
    #[error("prepared OCR request is already running")]
    Busy,
}

fn purge_expired(pending: &mut HashMap<String, PendingOcrRequest>, ttl: Duration) {
    pending.retain(|_, request| {
        let keep = request.created_at.elapsed() < ttl;
        if !keep {
            request.cancellation.cancel();
        }
        keep
    });
}

fn schedule_expiry(controller: Weak<OcrRequestController>, request_id: String, ttl: Duration) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(ttl).await;
        if let Some(controller) = controller.upgrade() {
            let mut pending = controller.pending.lock().unwrap();
            if pending
                .get(&request_id)
                .is_some_and(|request| request.created_at.elapsed() >= controller.ttl)
            {
                if let Some(request) = pending.remove(&request_id) {
                    request.cancellation.cancel();
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::ocr_provider::{OPEN_AI_BASE_URL, OPEN_AI_MODEL};
    use std::fs;

    #[derive(Default)]
    struct MemorySecrets(Mutex<HashMap<String, String>>);

    impl SecretStore for MemorySecrets {
        fn get(&self, account: &str) -> Result<Option<SecretString>, CredentialError> {
            Ok(self.0.lock().unwrap().get(account).cloned().map(Into::into))
        }

        fn set(&self, account: &str, secret: &SecretString) -> Result<(), CredentialError> {
            self.0
                .lock()
                .unwrap()
                .insert(account.into(), secret.expose_secret().into());
            Ok(())
        }

        fn delete(&self, account: &str) -> Result<(), CredentialError> {
            self.0.lock().unwrap().remove(account);
            Ok(())
        }
    }

    fn save_request(api_key: Option<&str>) -> SaveOcrProviderProfileRequest {
        SaveOcrProviderProfileRequest {
            id: None,
            revision: None,
            name: "OpenAI".into(),
            provider: OcrProviderPreset::OpenAi,
            protocol: OcrProviderProtocol::OpenAiChatCompletions,
            base_url: OPEN_AI_BASE_URL.into(),
            model: OPEN_AI_MODEL.into(),
            api_key: api_key.map(Into::into),
        }
    }

    fn seed_profile_metadata(directory: &Path, credential_id: &str) -> OcrProviderProfileMetadata {
        let mut store = OcrProviderStore::open(directory).unwrap();
        let (next, profile) = store
            .plan_save(
                SaveProfileMetadata {
                    id: None,
                    revision: None,
                    name: "OpenAI".into(),
                    provider: OcrProviderPreset::OpenAi,
                    protocol: OcrProviderProtocol::OpenAiChatCompletions,
                    base_url: OPEN_AI_BASE_URL.into(),
                    model: OPEN_AI_MODEL.into(),
                },
                CredentialMetadataChange::Replace(credential_id.into()),
            )
            .unwrap();
        store.commit(next).unwrap();
        profile
    }

    fn insert_memory_secret(
        secrets: &MemorySecrets,
        profile_id: &str,
        credential_id: &str,
        value: &str,
    ) {
        secrets
            .0
            .lock()
            .unwrap()
            .insert(credential_account(profile_id, credential_id), value.into());
    }

    fn has_memory_secret(secrets: &MemorySecrets, profile_id: &str, credential_id: &str) -> bool {
        secrets
            .0
            .lock()
            .unwrap()
            .contains_key(&credential_account(profile_id, credential_id))
    }

    fn owner() -> OcrRequestOwner {
        OcrRequestOwner {
            label: "overlay".into(),
            capture_id: uuid::Uuid::new_v4(),
        }
    }

    #[test]
    fn manager_defaults_local_and_never_serializes_credential_pointer() {
        let directory = tempfile::tempdir().unwrap();
        let secrets = Arc::new(MemorySecrets::default());
        let manager = OcrProviderManager::with_secret_store(directory.path(), secrets);
        assert_eq!(manager.settings().active_engine, OcrEngineRef::Local);

        let settings = manager.save(save_request(Some("secret-value"))).unwrap();
        assert!(settings.profiles[0].has_api_key);
        let serialized = serde_json::to_string(&settings).unwrap();
        assert!(!serialized.contains("secret-value"));
        assert!(!serialized.contains("credentialId"));
        let metadata = fs::read_to_string(directory.path().join("ocr-providers.json")).unwrap();
        assert!(!metadata.contains("secret-value"));
    }

    #[test]
    fn startup_rolls_back_an_unpublished_credential_replacement() {
        let directory = tempfile::tempdir().unwrap();
        let old_credential_id = uuid::Uuid::new_v4().to_string();
        let new_credential_id = uuid::Uuid::new_v4().to_string();
        let profile = seed_profile_metadata(directory.path(), &old_credential_id);
        let secrets = Arc::new(MemorySecrets::default());
        insert_memory_secret(&secrets, &profile.id, &old_credential_id, "old-secret");
        insert_memory_secret(&secrets, &profile.id, &new_credential_id, "new-secret");
        let journal = CredentialJournalStore::new(directory.path());
        journal
            .write(CredentialJournalOperation::Replace {
                profile_id: profile.id.clone(),
                old_credential_id: Some(old_credential_id.clone()),
                new_credential_id: new_credential_id.clone(),
            })
            .unwrap();
        let journal_json = fs::read_to_string(&journal.path).unwrap();
        assert!(!journal_json.contains("old-secret"));
        assert!(!journal_json.contains("new-secret"));

        let _manager = OcrProviderManager::with_secret_store(directory.path(), secrets.clone());
        assert!(has_memory_secret(&secrets, &profile.id, &old_credential_id));
        assert!(!has_memory_secret(
            &secrets,
            &profile.id,
            &new_credential_id
        ));
        assert!(!journal.path.exists());
    }

    #[test]
    fn startup_finishes_a_published_credential_replacement() {
        let directory = tempfile::tempdir().unwrap();
        let old_credential_id = uuid::Uuid::new_v4().to_string();
        let new_credential_id = uuid::Uuid::new_v4().to_string();
        let profile = seed_profile_metadata(directory.path(), &old_credential_id);
        let secrets = Arc::new(MemorySecrets::default());
        insert_memory_secret(&secrets, &profile.id, &old_credential_id, "old-secret");
        insert_memory_secret(&secrets, &profile.id, &new_credential_id, "new-secret");
        let journal = CredentialJournalStore::new(directory.path());
        journal
            .write(CredentialJournalOperation::Replace {
                profile_id: profile.id.clone(),
                old_credential_id: Some(old_credential_id.clone()),
                new_credential_id: new_credential_id.clone(),
            })
            .unwrap();
        let mut store = OcrProviderStore::open(directory.path()).unwrap();
        let (next, _) = store
            .plan_save(
                SaveProfileMetadata {
                    id: Some(profile.id.clone()),
                    revision: Some(profile.revision),
                    name: profile.name.clone(),
                    provider: profile.provider,
                    protocol: profile.protocol,
                    base_url: profile.base_url.clone(),
                    model: profile.model.clone(),
                },
                CredentialMetadataChange::Replace(new_credential_id.clone()),
            )
            .unwrap();
        store.commit(next).unwrap();
        drop(store);

        let _manager = OcrProviderManager::with_secret_store(directory.path(), secrets.clone());
        assert!(!has_memory_secret(
            &secrets,
            &profile.id,
            &old_credential_id
        ));
        assert!(has_memory_secret(&secrets, &profile.id, &new_credential_id));
        assert!(!journal.path.exists());
    }

    #[test]
    fn startup_preserves_a_credential_when_profile_delete_was_not_published() {
        let directory = tempfile::tempdir().unwrap();
        let credential_id = uuid::Uuid::new_v4().to_string();
        let profile = seed_profile_metadata(directory.path(), &credential_id);
        let secrets = Arc::new(MemorySecrets::default());
        insert_memory_secret(&secrets, &profile.id, &credential_id, "secret-value");
        let journal = CredentialJournalStore::new(directory.path());
        journal
            .write(CredentialJournalOperation::Delete {
                profile_id: profile.id.clone(),
                credential_id: credential_id.clone(),
            })
            .unwrap();

        let _manager = OcrProviderManager::with_secret_store(directory.path(), secrets.clone());
        assert!(has_memory_secret(&secrets, &profile.id, &credential_id));
        assert!(!journal.path.exists());
    }

    #[test]
    fn startup_finishes_a_published_profile_delete() {
        let directory = tempfile::tempdir().unwrap();
        let credential_id = uuid::Uuid::new_v4().to_string();
        let profile = seed_profile_metadata(directory.path(), &credential_id);
        let secrets = Arc::new(MemorySecrets::default());
        insert_memory_secret(&secrets, &profile.id, &credential_id, "secret-value");
        let journal = CredentialJournalStore::new(directory.path());
        journal
            .write(CredentialJournalOperation::Delete {
                profile_id: profile.id.clone(),
                credential_id: credential_id.clone(),
            })
            .unwrap();
        let mut store = OcrProviderStore::open(directory.path()).unwrap();
        let (next, _) = store.plan_delete(&profile.id, profile.revision).unwrap();
        store.commit(next).unwrap();
        drop(store);

        let _manager = OcrProviderManager::with_secret_store(directory.path(), secrets.clone());
        assert!(!has_memory_secret(&secrets, &profile.id, &credential_id));
        assert!(!journal.path.exists());
    }

    #[test]
    fn empty_key_update_preserves_existing_credential() {
        let directory = tempfile::tempdir().unwrap();
        let secrets = Arc::new(MemorySecrets::default());
        let manager = OcrProviderManager::with_secret_store(directory.path(), secrets);
        let first = manager.save(save_request(Some("secret-value"))).unwrap();
        let profile = &first.profiles[0];
        let mut update = save_request(Some(""));
        update.id = Some(profile.id.clone());
        update.revision = Some(profile.revision);
        update.model = "gpt-5".into();
        let updated = manager.save(update).unwrap();
        assert!(updated.profiles[0].has_api_key);
        assert_eq!(updated.profiles[0].revision, 2);
    }

    #[test]
    fn active_remote_profile_requires_a_system_credential() {
        let directory = tempfile::tempdir().unwrap();
        let secrets = Arc::new(MemorySecrets::default());
        let manager = OcrProviderManager::with_secret_store(directory.path(), secrets);
        let settings = manager.save(save_request(None)).unwrap();
        let profile_id = settings.profiles[0].id.clone();
        assert_eq!(
            manager
                .set_active(OcrEngineRef::Profile { profile_id })
                .err()
                .unwrap(),
            OcrManagerError::CredentialMissing
        );
        assert_eq!(manager.settings().active_engine, OcrEngineRef::Local);
    }

    #[test]
    fn externally_missing_active_credential_fails_closed_and_persists_local() {
        let directory = tempfile::tempdir().unwrap();
        let secrets = Arc::new(MemorySecrets::default());
        let manager = OcrProviderManager::with_secret_store(directory.path(), secrets.clone());
        let saved = manager.save(save_request(Some("secret-value"))).unwrap();
        manager
            .set_active(OcrEngineRef::Profile {
                profile_id: saved.profiles[0].id.clone(),
            })
            .unwrap();
        secrets.0.lock().unwrap().clear();

        assert!(matches!(
            manager.prepared_engine().unwrap(),
            PreparedEngine::Local
        ));
        assert_eq!(manager.settings().active_engine, OcrEngineRef::Local);
        let metadata: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(directory.path().join("ocr-providers.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(metadata["activeEngine"]["kind"], "local");
    }

    #[test]
    fn failed_metadata_publish_removes_the_new_credential() {
        let directory = tempfile::tempdir().unwrap();
        let secrets = Arc::new(MemorySecrets::default());
        let manager = OcrProviderManager::with_secret_store(directory.path(), secrets.clone());
        fs::create_dir(directory.path().join("ocr-providers.json")).unwrap();

        assert_eq!(
            manager
                .save(save_request(Some("secret-value")))
                .err()
                .unwrap(),
            OcrManagerError::StorageUnavailable
        );
        assert!(secrets.0.lock().unwrap().is_empty());
        assert!(manager.settings().profiles.is_empty());
    }

    #[test]
    fn failed_key_replacement_keeps_old_revision_and_removes_new_credential() {
        let directory = tempfile::tempdir().unwrap();
        let secrets = Arc::new(MemorySecrets::default());
        let manager = OcrProviderManager::with_secret_store(directory.path(), secrets.clone());
        let settings = manager.save(save_request(Some("old-secret"))).unwrap();
        let profile = settings.profiles[0].clone();
        let metadata_path = directory.path().join("ocr-providers.json");
        fs::remove_file(&metadata_path).unwrap();
        fs::create_dir(&metadata_path).unwrap();

        let mut update = save_request(Some("new-secret"));
        update.id = Some(profile.id);
        update.revision = Some(profile.revision);
        assert_eq!(
            manager.save(update).err().unwrap(),
            OcrManagerError::StorageUnavailable
        );
        let stored: Vec<String> = secrets.0.lock().unwrap().values().cloned().collect();
        assert_eq!(stored, vec!["old-secret"]);
        assert_eq!(manager.settings().profiles[0].revision, 1);
    }

    #[test]
    fn failed_delete_publish_restores_the_old_credential() {
        let directory = tempfile::tempdir().unwrap();
        let secrets = Arc::new(MemorySecrets::default());
        let manager = OcrProviderManager::with_secret_store(directory.path(), secrets.clone());
        let settings = manager.save(save_request(Some("secret-value"))).unwrap();
        let profile = settings.profiles[0].clone();
        let metadata_path = directory.path().join("ocr-providers.json");
        fs::remove_file(&metadata_path).unwrap();
        fs::create_dir(&metadata_path).unwrap();

        assert_eq!(
            manager.delete(&profile.id, profile.revision).err().unwrap(),
            OcrManagerError::StorageUnavailable
        );
        assert_eq!(secrets.0.lock().unwrap().len(), 1);
        assert!(manager.settings().profiles[0].has_api_key);
    }

    #[test]
    fn replacing_owner_request_prevents_failed_request_resurrection() {
        let controller = OcrRequestController::with_ttl(Duration::from_secs(30));
        let owner = owner();
        let first = controller
            .prepare(owner.clone(), vec![1], 1, 1, PreparedEngine::Local)
            .unwrap();
        let lease = controller.begin(&owner, &first.request_id).unwrap();
        let second = controller
            .prepare(owner.clone(), vec![2], 1, 1, PreparedEngine::Local)
            .unwrap();
        assert!(lease.cancellation.is_cancelled());
        controller.restore_after_failure(&lease);
        assert_eq!(
            controller.begin(&owner, &first.request_id).err().unwrap(),
            OcrRequestError::NotFound
        );
        assert_eq!(
            &*controller.begin(&owner, &second.request_id).unwrap().png,
            &[2]
        );
    }

    #[test]
    fn same_label_in_a_new_capture_cannot_access_the_old_request() {
        let controller = OcrRequestController::with_ttl(Duration::from_secs(30));
        let old_owner = owner();
        let new_owner = owner();
        let request = controller
            .prepare(old_owner.clone(), vec![1], 1, 1, PreparedEngine::Local)
            .unwrap();
        assert_eq!(
            controller
                .begin(&new_owner, &request.request_id)
                .err()
                .unwrap(),
            OcrRequestError::WrongOwner
        );
        assert!(controller.begin(&old_owner, &request.request_id).is_ok());
    }

    #[test]
    fn cancel_during_in_flight_prevents_restore() {
        let controller = OcrRequestController::with_ttl(Duration::from_secs(30));
        let owner = owner();
        let request = controller
            .prepare(owner.clone(), vec![1], 1, 1, PreparedEngine::Local)
            .unwrap();
        let lease = controller.begin(&owner, &request.request_id).unwrap();
        controller.cancel(&owner, &request.request_id).unwrap();
        assert!(lease.cancellation.is_cancelled());
        controller.restore_after_failure(&lease);
        assert_eq!(
            controller.begin(&owner, &request.request_id).err().unwrap(),
            OcrRequestError::NotFound
        );
    }

    #[test]
    fn pending_request_expires() {
        let controller = OcrRequestController::with_ttl(Duration::ZERO);
        let owner = owner();
        let request = controller
            .prepare(owner.clone(), vec![1], 1, 1, PreparedEngine::Local)
            .unwrap();
        assert_eq!(
            controller.begin(&owner, &request.request_id).err().unwrap(),
            OcrRequestError::NotFound
        );
    }
}
