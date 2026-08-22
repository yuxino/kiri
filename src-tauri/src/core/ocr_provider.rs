use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::{Host, Url};

pub const OCR_PROVIDER_SCHEMA_VERSION: u32 = 1;
pub const OCR_PROVIDER_FILENAME: &str = "ocr-providers.json";

pub const ALIYUN_BAILIAN_BASE_URL: &str = "https://dashscope.aliyuncs.com/compatible-mode/v1";
pub const ALIYUN_BAILIAN_MODEL: &str = "qwen3.5-ocr";
pub const OPEN_AI_BASE_URL: &str = "https://api.openai.com/v1";
pub const OPEN_AI_MODEL: &str = "gpt-5-mini";

const MAX_NAME_LEN: usize = 80;
const MAX_MODEL_LEN: usize = 256;
const MAX_BASE_URL_LEN: usize = 2_048;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OcrProviderPreset {
    AliyunBailian,
    OpenAi,
    CustomOpenAi,
}

impl OcrProviderPreset {
    pub fn defaults(self) -> (&'static str, &'static str) {
        match self {
            Self::AliyunBailian => (ALIYUN_BAILIAN_BASE_URL, ALIYUN_BAILIAN_MODEL),
            Self::OpenAi => (OPEN_AI_BASE_URL, OPEN_AI_MODEL),
            Self::CustomOpenAi => ("", ""),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OcrProviderProtocol {
    OpenAiChatCompletions,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum OcrEngineRef {
    #[default]
    Local,
    Profile {
        #[serde(rename = "profileId")]
        profile_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OcrProviderProfileMetadata {
    pub id: String,
    pub revision: u64,
    pub name: String,
    pub provider: OcrProviderPreset,
    pub protocol: OcrProviderProtocol,
    pub base_url: String,
    pub model: String,
    /// Opaque pointer into the OS credential store. This value is never
    /// included in frontend DTOs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_id: Option<String>,
}

impl OcrProviderProfileMetadata {
    pub fn endpoint(&self) -> Result<Url, OcrProviderError> {
        let mut url = validate_base_url(&self.base_url)?;
        let path = format!("{}/chat/completions", url.path().trim_end_matches('/'));
        url.set_path(&path);
        Ok(url)
    }

    pub fn origin(&self) -> Result<String, OcrProviderError> {
        let url = validate_base_url(&self.base_url)?;
        Ok(url.origin().ascii_serialization())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PersistedOcrProviderSettings {
    pub schema_version: u32,
    pub active_engine: OcrEngineRef,
    pub profiles: Vec<OcrProviderProfileMetadata>,
}

impl Default for PersistedOcrProviderSettings {
    fn default() -> Self {
        Self {
            schema_version: OCR_PROVIDER_SCHEMA_VERSION,
            active_engine: OcrEngineRef::Local,
            profiles: Vec::new(),
        }
    }
}

#[derive(Clone)]
pub struct SaveProfileMetadata {
    pub id: Option<String>,
    pub revision: Option<u64>,
    pub name: String,
    pub provider: OcrProviderPreset,
    pub protocol: OcrProviderProtocol,
    pub base_url: String,
    pub model: String,
}

#[derive(Debug, Clone)]
pub enum CredentialMetadataChange {
    Preserve,
    Replace(String),
}

pub struct OcrProviderStore {
    path: PathBuf,
    settings: PersistedOcrProviderSettings,
}

impl OcrProviderStore {
    pub fn open(config_dir: &Path) -> Result<Self, OcrProviderError> {
        fs::create_dir_all(config_dir).map_err(|_| OcrProviderError::StorageUnavailable)?;
        let path = config_dir.join(OCR_PROVIDER_FILENAME);
        let settings = if path.exists() {
            let bytes = fs::read(&path).map_err(|_| OcrProviderError::StorageUnavailable)?;
            let settings: PersistedOcrProviderSettings =
                serde_json::from_slice(&bytes).map_err(|_| OcrProviderError::InvalidSettings)?;
            validate_settings(&settings)?;
            settings
        } else {
            PersistedOcrProviderSettings::default()
        };
        Ok(Self { path, settings })
    }

    pub fn settings(&self) -> &PersistedOcrProviderSettings {
        &self.settings
    }

    pub fn profile(&self, id: &str) -> Option<&OcrProviderProfileMetadata> {
        self.settings
            .profiles
            .iter()
            .find(|profile| profile.id == id)
    }

    pub fn plan_save(
        &self,
        request: SaveProfileMetadata,
        credential_change: CredentialMetadataChange,
    ) -> Result<(PersistedOcrProviderSettings, OcrProviderProfileMetadata), OcrProviderError> {
        let name = validate_text(request.name, MAX_NAME_LEN)?;
        let (default_base_url, default_model) = request.provider.defaults();
        let model_source = if request.model.trim().is_empty() && !default_model.is_empty() {
            default_model.to_string()
        } else {
            request.model
        };
        let base_url_source = if request.base_url.trim().is_empty() && !default_base_url.is_empty()
        {
            default_base_url.to_string()
        } else {
            request.base_url
        };
        let model = validate_text(model_source, MAX_MODEL_LEN)?;
        let base_url = normalize_base_url(&base_url_source)?;

        let mut next = self.settings.clone();
        let profile = match (&request.id, request.revision) {
            (None, None) => {
                let id = uuid::Uuid::new_v4().to_string();
                OcrProviderProfileMetadata {
                    id,
                    revision: 1,
                    name,
                    provider: request.provider,
                    protocol: request.protocol,
                    base_url,
                    model,
                    credential_id: match credential_change {
                        CredentialMetadataChange::Preserve => None,
                        CredentialMetadataChange::Replace(id) => Some(validate_opaque_id(id)?),
                    },
                }
            }
            (Some(id), Some(revision)) => {
                let current = self.profile(id).ok_or(OcrProviderError::ProfileNotFound)?;
                if current.revision != revision {
                    return Err(OcrProviderError::RevisionConflict);
                }
                OcrProviderProfileMetadata {
                    id: current.id.clone(),
                    revision: current
                        .revision
                        .checked_add(1)
                        .ok_or(OcrProviderError::RevisionConflict)?,
                    name,
                    provider: request.provider,
                    protocol: request.protocol,
                    base_url,
                    model,
                    credential_id: match credential_change {
                        CredentialMetadataChange::Preserve => current.credential_id.clone(),
                        CredentialMetadataChange::Replace(id) => Some(validate_opaque_id(id)?),
                    },
                }
            }
            _ => return Err(OcrProviderError::InvalidRequest),
        };

        if let Some(index) = next.profiles.iter().position(|item| item.id == profile.id) {
            next.profiles[index] = profile.clone();
        } else {
            next.profiles.push(profile.clone());
        }
        validate_settings(&next)?;
        Ok((next, profile))
    }

    pub fn plan_delete(
        &self,
        profile_id: &str,
        revision: u64,
    ) -> Result<(PersistedOcrProviderSettings, OcrProviderProfileMetadata), OcrProviderError> {
        let current = self
            .profile(profile_id)
            .cloned()
            .ok_or(OcrProviderError::ProfileNotFound)?;
        if current.revision != revision {
            return Err(OcrProviderError::RevisionConflict);
        }
        let mut next = self.settings.clone();
        next.profiles.retain(|profile| profile.id != profile_id);
        if matches!(
            &next.active_engine,
            OcrEngineRef::Profile { profile_id: active } if active == profile_id
        ) {
            next.active_engine = OcrEngineRef::Local;
        }
        validate_settings(&next)?;
        Ok((next, current))
    }

    pub fn plan_set_active(
        &self,
        active_engine: OcrEngineRef,
    ) -> Result<PersistedOcrProviderSettings, OcrProviderError> {
        if let OcrEngineRef::Profile { profile_id } = &active_engine {
            if self.profile(profile_id).is_none() {
                return Err(OcrProviderError::ProfileNotFound);
            }
        }
        let mut next = self.settings.clone();
        next.active_engine = active_engine;
        validate_settings(&next)?;
        Ok(next)
    }

    pub fn commit(
        &mut self,
        settings: PersistedOcrProviderSettings,
    ) -> Result<(), OcrProviderError> {
        validate_settings(&settings)?;
        atomic_write_json(&self.path, &settings)?;
        self.settings = settings;
        Ok(())
    }
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum OcrProviderError {
    #[error("OCR provider storage is unavailable")]
    StorageUnavailable,
    #[error("OCR provider settings are invalid")]
    InvalidSettings,
    #[error("OCR provider request is invalid")]
    InvalidRequest,
    #[error("OCR provider profile was not found")]
    ProfileNotFound,
    #[error("OCR provider profile changed; reload settings")]
    RevisionConflict,
}

fn atomic_write_json(
    path: &Path,
    settings: &PersistedOcrProviderSettings,
) -> Result<(), OcrProviderError> {
    let parent = path.parent().ok_or(OcrProviderError::StorageUnavailable)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .map_err(|_| OcrProviderError::StorageUnavailable)?;
    serde_json::to_writer_pretty(temporary.as_file_mut(), settings)
        .map_err(|_| OcrProviderError::StorageUnavailable)?;
    temporary
        .as_file_mut()
        .write_all(b"\n")
        .map_err(|_| OcrProviderError::StorageUnavailable)?;
    temporary
        .as_file_mut()
        .sync_all()
        .map_err(|_| OcrProviderError::StorageUnavailable)?;
    temporary
        .persist(path)
        .map_err(|_| OcrProviderError::StorageUnavailable)?;

    #[cfg(unix)]
    {
        if let Ok(directory) = fs::File::open(parent) {
            let _ = directory.sync_all();
        }
    }
    Ok(())
}

fn validate_settings(settings: &PersistedOcrProviderSettings) -> Result<(), OcrProviderError> {
    if settings.schema_version != OCR_PROVIDER_SCHEMA_VERSION {
        return Err(OcrProviderError::InvalidSettings);
    }
    let mut ids = HashSet::new();
    for profile in &settings.profiles {
        validate_opaque_id(profile.id.clone())?;
        if !ids.insert(profile.id.as_str()) || profile.revision == 0 {
            return Err(OcrProviderError::InvalidSettings);
        }
        validate_text(profile.name.clone(), MAX_NAME_LEN)?;
        validate_text(profile.model.clone(), MAX_MODEL_LEN)?;
        if normalize_base_url(&profile.base_url)? != profile.base_url {
            return Err(OcrProviderError::InvalidSettings);
        }
        if let Some(credential_id) = &profile.credential_id {
            validate_opaque_id(credential_id.clone())?;
        }
    }
    if let OcrEngineRef::Profile { profile_id } = &settings.active_engine {
        if !ids.contains(profile_id.as_str()) {
            return Err(OcrProviderError::InvalidSettings);
        }
    }
    Ok(())
}

fn validate_text(value: String, max_len: usize) -> Result<String, OcrProviderError> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.len() > max_len || trimmed.chars().any(char::is_control) {
        return Err(OcrProviderError::InvalidRequest);
    }
    Ok(trimmed.to_string())
}

fn validate_opaque_id(value: String) -> Result<String, OcrProviderError> {
    if value.is_empty()
        || value.len() > 128
        || value.contains(':')
        || value.chars().any(|character| {
            character.is_control() || !(character.is_ascii_alphanumeric() || character == '-')
        })
    {
        return Err(OcrProviderError::InvalidRequest);
    }
    Ok(value)
}

fn normalize_base_url(value: &str) -> Result<String, OcrProviderError> {
    if value.len() > MAX_BASE_URL_LEN || value.trim() != value {
        return Err(OcrProviderError::InvalidRequest);
    }
    let mut url = validate_base_url(value)?;
    let normalized_path = url.path().trim_end_matches('/').to_string();
    url.set_path(if normalized_path.is_empty() {
        "/"
    } else {
        &normalized_path
    });
    let normalized = url.as_str().trim_end_matches('/').to_string();
    Ok(normalized)
}

fn validate_base_url(value: &str) -> Result<Url, OcrProviderError> {
    let url = Url::parse(value).map_err(|_| OcrProviderError::InvalidRequest)?;
    if url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(OcrProviderError::InvalidRequest);
    }
    match url.scheme() {
        "https" => {}
        "http" if is_loopback_url(&url) => {}
        _ => return Err(OcrProviderError::InvalidRequest),
    }
    Ok(url)
}

pub fn is_loopback_url(url: &Url) -> bool {
    match url.host() {
        Some(Host::Domain(host)) => host.eq_ignore_ascii_case("localhost"),
        Some(Host::Ipv4(address)) => address == std::net::Ipv4Addr::LOCALHOST,
        Some(Host::Ipv6(address)) => address == std::net::Ipv6Addr::LOCALHOST,
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft() -> SaveProfileMetadata {
        SaveProfileMetadata {
            id: None,
            revision: None,
            name: "OpenAI".into(),
            provider: OcrProviderPreset::OpenAi,
            protocol: OcrProviderProtocol::OpenAiChatCompletions,
            base_url: OPEN_AI_BASE_URL.into(),
            model: OPEN_AI_MODEL.into(),
        }
    }

    #[test]
    fn defaults_to_local_without_profiles() {
        let directory = tempfile::tempdir().unwrap();
        let store = OcrProviderStore::open(directory.path()).unwrap();
        assert_eq!(store.settings().active_engine, OcrEngineRef::Local);
        assert!(store.settings().profiles.is_empty());
    }

    #[test]
    fn save_uses_revision_and_preserves_credential_pointer() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = OcrProviderStore::open(directory.path()).unwrap();
        let (next, created) = store
            .plan_save(
                draft(),
                CredentialMetadataChange::Replace("credential-one".into()),
            )
            .unwrap();
        store.commit(next).unwrap();

        let mut update = draft();
        update.id = Some(created.id.clone());
        update.revision = Some(1);
        update.model = "gpt-5".into();
        let (next, updated) = store
            .plan_save(update, CredentialMetadataChange::Preserve)
            .unwrap();
        assert_eq!(updated.revision, 2);
        assert_eq!(updated.credential_id.as_deref(), Some("credential-one"));
        store.commit(next).unwrap();

        let reloaded = OcrProviderStore::open(directory.path()).unwrap();
        assert_eq!(reloaded.profile(&created.id).unwrap().model, "gpt-5");
    }

    #[test]
    fn rejects_stale_revision_and_insecure_remote_http() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = OcrProviderStore::open(directory.path()).unwrap();
        let (next, created) = store
            .plan_save(draft(), CredentialMetadataChange::Preserve)
            .unwrap();
        store.commit(next).unwrap();

        let mut update = draft();
        update.id = Some(created.id);
        update.revision = Some(99);
        assert_eq!(
            store
                .plan_save(update, CredentialMetadataChange::Preserve)
                .unwrap_err(),
            OcrProviderError::RevisionConflict
        );

        let mut insecure = draft();
        insecure.base_url = "http://example.com/v1".into();
        assert_eq!(
            store
                .plan_save(insecure, CredentialMetadataChange::Preserve)
                .unwrap_err(),
            OcrProviderError::InvalidRequest
        );
    }

    #[test]
    fn permits_exact_loopback_http_and_builds_endpoint() {
        let mut loopback = draft();
        loopback.base_url = "http://127.0.0.1:11434/v1/".into();
        let directory = tempfile::tempdir().unwrap();
        let store = OcrProviderStore::open(directory.path()).unwrap();
        let (_, profile) = store
            .plan_save(loopback, CredentialMetadataChange::Preserve)
            .unwrap();
        assert_eq!(profile.base_url, "http://127.0.0.1:11434/v1");
        assert_eq!(
            profile.endpoint().unwrap().as_str(),
            "http://127.0.0.1:11434/v1/chat/completions"
        );
        assert!(is_loopback_url(&Url::parse(&profile.base_url).unwrap()));

        let mut ipv6 = draft();
        ipv6.base_url = "http://[::1]:11434/v1".into();
        let (_, profile) = store
            .plan_save(ipv6, CredentialMetadataChange::Preserve)
            .unwrap();
        assert!(is_loopback_url(&Url::parse(&profile.base_url).unwrap()));
    }

    #[test]
    fn rejects_non_exact_loopback_and_url_smuggling_fields() {
        let store_dir = tempfile::tempdir().unwrap();
        let store = OcrProviderStore::open(store_dir.path()).unwrap();
        for base_url in [
            "http://127.0.0.2:11434/v1",
            "http://foo.localhost:11434/v1",
            "https://user@example.com/v1",
            "https://example.com/v1?target=other",
        ] {
            let mut request = draft();
            request.base_url = base_url.into();
            assert_eq!(
                store
                    .plan_save(request, CredentialMetadataChange::Preserve)
                    .unwrap_err(),
                OcrProviderError::InvalidRequest,
                "unexpectedly accepted {base_url}"
            );
        }
    }

    #[test]
    fn persisted_metadata_rejects_unknown_secret_fields() {
        let settings = r#"{
          "schemaVersion": 1,
          "activeEngine": {"kind":"local"},
          "profiles": [{
            "id":"profile-one","revision":1,"name":"OpenAI",
            "provider":"openAi","protocol":"openAiChatCompletions",
            "baseUrl":"https://api.openai.com/v1","model":"gpt-5-mini",
            "apiKey":"must-not-be-accepted"
          }]
        }"#;
        assert!(serde_json::from_str::<PersistedOcrProviderSettings>(settings).is_err());
    }

    #[test]
    fn deleting_active_profile_returns_to_local() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = OcrProviderStore::open(directory.path()).unwrap();
        let (next, profile) = store
            .plan_save(draft(), CredentialMetadataChange::Preserve)
            .unwrap();
        store.commit(next).unwrap();
        let next = store
            .plan_set_active(OcrEngineRef::Profile {
                profile_id: profile.id.clone(),
            })
            .unwrap();
        store.commit(next).unwrap();
        let (next, _) = store.plan_delete(&profile.id, profile.revision).unwrap();
        assert_eq!(next.active_engine, OcrEngineRef::Local);
    }
}
