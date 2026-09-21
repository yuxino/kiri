//! Viewer-scoped video draft commands. Validation and disk I/O run off the
//! event loop, with the library mutex serializing save/migration/delete races.

use tauri::{AppHandle, Manager, WebviewWindow};

use crate::core::video_project::{VideoProject, VideoProjectError, VideoProjectSnapshot};
use crate::state::AppState;

fn owned_video_id(label: &str, id: &str) -> Result<uuid::Uuid, String> {
    let id = uuid::Uuid::parse_str(id).map_err(|_| VideoProjectError::Unavailable.to_string())?;
    if label != format!("viewer-{id}") {
        return Err(VideoProjectError::Unavailable.to_string());
    }
    Ok(id)
}

#[tauri::command]
pub async fn load_video_project(
    app: AppHandle,
    window: WebviewWindow,
    id: String,
) -> Result<VideoProjectSnapshot, String> {
    let id = owned_video_id(window.label(), &id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let mut context = state
            .library
            .lock()
            .map_err(|_| VideoProjectError::Unavailable.to_string())?;
        let library_id = context.expected_library_id();
        let generation = context.expected_library_generation();
        context
            .library()
            .map_err(|_| VideoProjectError::Unavailable.to_string())?
            .load_video_project(&id, library_id, generation)
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|_| VideoProjectError::Unavailable.to_string())?
}

#[tauri::command]
pub async fn save_video_project(
    app: AppHandle,
    window: WebviewWindow,
    id: String,
    revision: String,
    project: VideoProject,
) -> Result<VideoProjectSnapshot, String> {
    let id = owned_video_id(window.label(), &id)?;
    if revision.len() != 64
        || !revision
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(VideoProjectError::Conflict.to_string());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let mut context = state
            .library
            .lock()
            .map_err(|_| VideoProjectError::Unavailable.to_string())?;
        let library_id = context.expected_library_id();
        let generation = context.expected_library_generation();
        context
            .library()
            .map_err(|_| VideoProjectError::Unavailable.to_string())?
            .save_video_project(&id, library_id, generation, &revision, project)
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|_| VideoProjectError::Unavailable.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_only_accept_the_exact_owning_viewer() {
        let id = uuid::Uuid::parse_str("abcde012-3456-4789-8abc-def012345678").unwrap();
        assert_eq!(
            owned_video_id(&format!("viewer-{id}"), &id.to_string()).unwrap(),
            id
        );
        for label in [
            "library".to_string(),
            format!("editor-{id}"),
            format!("viewer-{}", uuid::Uuid::new_v4()),
            format!("viewer-{}", id.to_string().to_uppercase()),
        ] {
            assert!(owned_video_id(&label, &id.to_string()).is_err());
        }
    }
}
