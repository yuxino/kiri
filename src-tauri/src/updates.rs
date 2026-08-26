//! Explicit, user-initiated release checks.
//!
//! Kiri does not download or install application updates. This module performs
//! one bounded request to the fixed public GitHub Releases API when the user
//! presses Check for Updates, compares semantic versions, and can open the
//! fixed release page in the system browser.

use std::time::Duration;

use reqwest::{redirect::Policy, Client};
use semver::Version;
use serde::{Deserialize, Serialize};
use tauri::AppHandle;

const LATEST_RELEASE_API: &str = "https://api.github.com/repos/yuxino/kiri/releases/latest";
const RELEASE_PAGE: &str = "https://github.com/yuxino/kiri/releases/latest";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_RESPONSE_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheckDto {
    current_version: String,
    latest_version: String,
    update_available: bool,
}

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
enum UpdateCheckError {
    #[error("the update client could not be initialized")]
    ClientInitialization,
    #[error("the update request failed")]
    RequestFailed,
    #[error("the update service returned HTTP {0}")]
    HttpStatus(u16),
    #[error("the update response could not be read")]
    ResponseReadFailed,
    #[error("the update response was too large")]
    ResponseTooLarge,
    #[error("the update response was invalid")]
    InvalidResponse,
    #[error("the latest release tag was not a valid version")]
    InvalidVersion,
}

#[tauri::command]
pub async fn check_for_updates(app: AppHandle) -> Result<UpdateCheckDto, String> {
    let current_version = app.package_info().version.clone();
    match fetch_latest_release(&current_version).await {
        Ok(result) => Ok(result),
        Err(error) => {
            log::warn!("[updates] check failed: {error}");
            Err("Couldn't check for updates.".into())
        }
    }
}

async fn fetch_latest_release(
    current_version: &Version,
) -> Result<UpdateCheckDto, UpdateCheckError> {
    ensure_ring_crypto_provider()?;
    let client = Client::builder()
        .redirect(Policy::none())
        .retry(reqwest::retry::never())
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .user_agent(format!("kiri/{current_version}"))
        .build()
        .map_err(|_| UpdateCheckError::ClientInitialization)?;

    let mut response = client
        .get(LATEST_RELEASE_API)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .await
        .map_err(|_| UpdateCheckError::RequestFailed)?;
    let status = response.status();
    if !status.is_success() {
        return Err(UpdateCheckError::HttpStatus(status.as_u16()));
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
    {
        return Err(UpdateCheckError::ResponseTooLarge);
    }

    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| UpdateCheckError::ResponseReadFailed)?
    {
        append_response_chunk(&mut bytes, &chunk)?;
    }
    parse_latest_release(&bytes, current_version)
}

fn append_response_chunk(bytes: &mut Vec<u8>, chunk: &[u8]) -> Result<(), UpdateCheckError> {
    if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
        return Err(UpdateCheckError::ResponseTooLarge);
    }
    bytes.extend_from_slice(chunk);
    Ok(())
}

fn parse_latest_release(
    body: &[u8],
    current_version: &Version,
) -> Result<UpdateCheckDto, UpdateCheckError> {
    if body.len() > MAX_RESPONSE_BYTES {
        return Err(UpdateCheckError::ResponseTooLarge);
    }
    let release: GitHubRelease =
        serde_json::from_slice(body).map_err(|_| UpdateCheckError::InvalidResponse)?;
    let version = release
        .tag_name
        .strip_prefix('v')
        .unwrap_or(&release.tag_name);
    let latest_version = Version::parse(version).map_err(|_| UpdateCheckError::InvalidVersion)?;

    Ok(UpdateCheckDto {
        current_version: current_version.to_string(),
        latest_version: latest_version.to_string(),
        update_available: latest_version > *current_version,
    })
}

fn ensure_ring_crypto_provider() -> Result<(), UpdateCheckError> {
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        let _ = rustls::crypto::ring::default_provider().install_default();
    }
    rustls::crypto::CryptoProvider::get_default()
        .map(|_| ())
        .ok_or(UpdateCheckError::ClientInitialization)
}

#[tauri::command]
pub fn open_release_page() -> Result<(), String> {
    open_external_url(RELEASE_PAGE).map_err(|error| {
        log::warn!("[updates] could not open release page: {error}");
        "Couldn't open the update page.".to_string()
    })
}

#[cfg(target_os = "macos")]
fn open_external_url(url: &str) -> Result<(), String> {
    let workspace = objc2_app_kit::NSWorkspace::sharedWorkspace();
    let url = objc2_foundation::NSURL::URLWithString(&objc2_foundation::NSString::from_str(url))
        .ok_or_else(|| "the release URL is invalid".to_string())?;
    if workspace.openURL(&url) {
        Ok(())
    } else {
        Err("the release page could not be opened".to_string())
    }
}

#[cfg(windows)]
fn open_external_url(url: &str) -> Result<(), String> {
    std::process::Command::new("explorer")
        .arg(url)
        .spawn()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::{
        append_response_chunk, parse_latest_release, UpdateCheckError, MAX_RESPONSE_BYTES,
    };
    use semver::Version;

    #[test]
    fn reports_a_newer_semantic_version() {
        let result = parse_latest_release(
            br#"{"tag_name":"v1.5.0"}"#,
            &Version::parse("1.4.3").unwrap(),
        )
        .unwrap();
        assert_eq!(result.current_version, "1.4.3");
        assert_eq!(result.latest_version, "1.5.0");
        assert!(result.update_available);
    }

    #[test]
    fn equal_or_older_releases_are_up_to_date() {
        for tag in ["v1.4.3", "v1.4.2"] {
            let body = format!(r#"{{"tag_name":"{tag}"}}"#);
            let result =
                parse_latest_release(body.as_bytes(), &Version::parse("1.4.3").unwrap()).unwrap();
            assert!(!result.update_available, "{tag} should not be an update");
        }
    }

    #[test]
    fn stable_release_is_newer_than_a_prerelease() {
        let result = parse_latest_release(
            br#"{"tag_name":"1.4.3"}"#,
            &Version::parse("1.4.3-beta.1").unwrap(),
        )
        .unwrap();
        assert!(result.update_available);
    }

    #[test]
    fn rejects_missing_or_invalid_release_tags() {
        assert_eq!(
            parse_latest_release(b"{}", &Version::new(1, 4, 3)),
            Err(UpdateCheckError::InvalidResponse)
        );
        assert_eq!(
            parse_latest_release(br#"{"tag_name":"latest"}"#, &Version::new(1, 4, 3)),
            Err(UpdateCheckError::InvalidVersion)
        );
    }

    #[test]
    fn response_buffer_has_a_hard_limit() {
        let mut body = vec![0; MAX_RESPONSE_BYTES - 2];
        assert_eq!(
            append_response_chunk(&mut body, &[1, 2, 3]),
            Err(UpdateCheckError::ResponseTooLarge)
        );
        assert_eq!(body.len(), MAX_RESPONSE_BYTES - 2);
    }
}
