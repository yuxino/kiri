//! Fixed recovery route for update failures.
//!
//! Signed update checks, downloads, and installation are handled by Tauri's
//! official updater plugin. GitHub Releases is only an explicit recovery link.

const RELEASE_PAGE: &str = "https://github.com/yuxino/kiri/releases/latest";

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
