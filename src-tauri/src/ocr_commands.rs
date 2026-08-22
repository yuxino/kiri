use tauri::{AppHandle, Manager, WebviewWindow};

use crate::commands::RectDto;
use crate::core::ocr_provider::OcrEngineRef;
use crate::ocr_controller::{
    OcrProviderSettingsDto, OcrRequestOwner, PreparedEngine, PreparedOcrRequestDto,
    SaveOcrProviderProfileRequest,
};
use crate::state::AppState;

const MAX_PREPARED_PNG_BYTES: usize = 20 * 1024 * 1024;

#[tauri::command]
pub async fn get_ocr_provider_settings(
    window: WebviewWindow,
    app: AppHandle,
) -> Result<OcrProviderSettingsDto, String> {
    require_library_window(&window)?;
    let manager = app.state::<AppState>().ocr_providers.clone();
    tauri::async_runtime::spawn_blocking(move || manager.settings())
        .await
        .map_err(|_| "OCR settings could not be loaded.".to_string())
}

#[tauri::command]
pub async fn save_ocr_provider_profile(
    window: WebviewWindow,
    app: AppHandle,
    request: SaveOcrProviderProfileRequest,
) -> Result<OcrProviderSettingsDto, String> {
    require_library_window(&window)?;
    let manager = app.state::<AppState>().ocr_providers.clone();
    tauri::async_runtime::spawn_blocking(move || manager.save(request))
        .await
        .map_err(|_| "OCR settings could not be saved.".to_string())?
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn delete_ocr_provider_profile(
    window: WebviewWindow,
    app: AppHandle,
    profile_id: String,
    profile_revision: u64,
) -> Result<OcrProviderSettingsDto, String> {
    require_library_window(&window)?;
    let manager = app.state::<AppState>().ocr_providers.clone();
    tauri::async_runtime::spawn_blocking(move || manager.delete(&profile_id, profile_revision))
        .await
        .map_err(|_| "OCR settings could not be saved.".to_string())?
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn set_active_ocr_engine(
    window: WebviewWindow,
    app: AppHandle,
    engine: OcrEngineRef,
) -> Result<OcrProviderSettingsDto, String> {
    require_library_window(&window)?;
    let manager = app.state::<AppState>().ocr_providers.clone();
    tauri::async_runtime::spawn_blocking(move || manager.set_active(engine))
        .await
        .map_err(|_| "OCR settings could not be saved.".to_string())?
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn prepare_ocr_request(
    window: WebviewWindow,
    app: AppHandle,
    selection: RectDto,
) -> Result<PreparedOcrRequestDto, String> {
    let owner = require_active_overlay(&window, &app)?;
    let source = {
        let state = app.state::<AppState>();
        let capture = state.capture.lock().unwrap();
        let session = capture
            .session
            .as_ref()
            .filter(|session| {
                session.capture_id == owner.capture_id
                    && session
                        .overlay_labels
                        .iter()
                        .any(|label| label == &owner.label)
            })
            .ok_or_else(|| "No active capture session.".to_string())?;
        FrozenPngSource {
            png: session.display.png_data.clone(),
            declared_width: session.display.pixel_width,
            declared_height: session.display.pixel_height,
            display_width: session.display.screen_frame.width,
            display_height: session.display.screen_frame.height,
            scale: session.display.backing_scale,
        }
    };
    let cropped = tauri::async_runtime::spawn_blocking(move || crop_frozen_png(source, selection))
        .await
        .map_err(|_| "The OCR image could not be prepared.".to_string())??;
    if cropped.png.len() > MAX_PREPARED_PNG_BYTES {
        return Err("The OCR image exceeds the size limit.".into());
    }

    let (manager, requests) = {
        let state = app.state::<AppState>();
        (state.ocr_providers.clone(), state.ocr_requests.clone())
    };
    let engine = tauri::async_runtime::spawn_blocking(move || manager.prepared_engine())
        .await
        .map_err(|_| "OCR settings could not be loaded.".to_string())?
        .map_err(|error| error.to_string())?;
    // Keep the capture lock across the final owner check and insertion. All
    // capture teardown paths take this lock before clearing pending requests,
    // so cancel/Destroyed cannot race a request back into memory.
    let state = app.state::<AppState>();
    let capture = state.capture.lock().unwrap();
    let still_active = capture.session.as_ref().is_some_and(|session| {
        session.capture_id == owner.capture_id
            && session
                .overlay_labels
                .iter()
                .any(|overlay_label| overlay_label == &owner.label)
    });
    if !still_active || window.label() != owner.label {
        return Err("The active capture overlay changed.".into());
    }
    requests
        .prepare(owner, cropped.png, cropped.width, cropped.height, engine)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn recognize_prepared_ocr_local(
    window: WebviewWindow,
    app: AppHandle,
    request_id: String,
) -> Result<String, String> {
    let owner = require_active_overlay(&window, &app)?;
    let requests = app.state::<AppState>().ocr_requests.clone();
    let lease = requests
        .begin(&owner, &request_id)
        .map_err(|error| error.to_string())?;
    let png = lease.png.clone();
    let result =
        tauri::async_runtime::spawn_blocking(move || crate::ocr::recognize_text(&png)).await;
    if lease.cancellation.is_cancelled() {
        return Err("The OCR request was canceled.".into());
    }
    match result {
        Ok(Ok(text)) => {
            requests.complete(&lease);
            Ok(text)
        }
        Ok(Err(_)) | Err(_) => {
            requests.restore_after_failure(&lease);
            Err("Local OCR failed.".into())
        }
    }
}

#[tauri::command]
pub async fn recognize_prepared_ocr_remote(
    window: WebviewWindow,
    app: AppHandle,
    request_id: String,
    profile_id: String,
    profile_revision: u64,
) -> Result<String, String> {
    let owner = require_active_overlay(&window, &app)?;
    let (requests, manager, client) = {
        let state = app.state::<AppState>();
        (
            state.ocr_requests.clone(),
            state.ocr_providers.clone(),
            state.remote_ocr.clone(),
        )
    };
    let lease = requests
        .begin(&owner, &request_id)
        .map_err(|error| error.to_string())?;
    if !matches!(
        &lease.engine,
        PreparedEngine::Profile { profile, .. }
            if profile.id == profile_id && profile.revision == profile_revision
    ) {
        requests.restore_after_failure(&lease);
        return Err("OCR provider profile changed; prepare the request again.".into());
    }

    let lookup_id = profile_id.clone();
    let resolved_task = tauri::async_runtime::spawn_blocking(move || {
        manager.resolve_remote(&lookup_id, profile_revision)
    });
    let resolved = tokio::select! {
        biased;
        _ = lease.cancellation.cancelled() => {
            return Err("The OCR request was canceled.".into());
        }
        resolved = resolved_task => resolved,
    };
    let resolved = match resolved {
        Ok(Ok(resolved)) => resolved,
        Ok(Err(error)) => {
            requests.restore_after_failure(&lease);
            return Err(error.to_string());
        }
        Err(_) => {
            requests.restore_after_failure(&lease);
            return Err("OCR provider settings could not be loaded.".into());
        }
    };
    let Some(client) = client else {
        requests.restore_after_failure(&lease);
        return Err("Remote OCR is unavailable.".into());
    };
    let endpoint = match resolved.profile.endpoint() {
        Ok(endpoint) => endpoint,
        Err(_) => {
            requests.restore_after_failure(&lease);
            return Err("Remote OCR endpoint is invalid.".into());
        }
    };
    if lease.cancellation.is_cancelled()
        || require_active_overlay(&window, &app).ok().as_ref() != Some(&owner)
    {
        return Err("The OCR request was canceled.".into());
    }
    let result = tokio::select! {
        biased;
        _ = lease.cancellation.cancelled() => {
            return Err("The OCR request was canceled.".into());
        }
        result = client.recognize(
            &endpoint,
            &resolved.profile.model,
            &resolved.api_key,
            &lease.png,
        ) => result,
    };
    match result {
        Ok(text) => {
            requests.complete(&lease);
            Ok(text)
        }
        Err(error) => {
            requests.restore_after_failure(&lease);
            Err(error.to_string())
        }
    }
}

#[tauri::command]
pub fn cancel_prepared_ocr(
    window: WebviewWindow,
    app: AppHandle,
    request_id: String,
) -> Result<(), String> {
    let owner = require_active_overlay(&window, &app)?;
    app.state::<AppState>()
        .ocr_requests
        .cancel(&owner, &request_id)
        .map_err(|error| error.to_string())
}

fn require_library_window(window: &WebviewWindow) -> Result<(), String> {
    if window.label() == "library" {
        Ok(())
    } else {
        Err("This command is only available from the library window.".into())
    }
}

fn require_active_overlay(
    window: &WebviewWindow,
    app: &AppHandle,
) -> Result<OcrRequestOwner, String> {
    let label = window.label().to_string();
    let state = app.state::<AppState>();
    let capture = state.capture.lock().unwrap();
    capture
        .session
        .as_ref()
        .filter(|session| {
            session
                .overlay_labels
                .iter()
                .any(|overlay_label| overlay_label == &label)
        })
        .map(|session| OcrRequestOwner {
            label,
            capture_id: session.capture_id,
        })
        .ok_or_else(|| "This command is only available from the active capture overlay.".into())
}

struct FrozenPngSource {
    png: Vec<u8>,
    declared_width: i64,
    declared_height: i64,
    display_width: f64,
    display_height: f64,
    scale: f64,
}

struct CroppedPng {
    png: Vec<u8>,
    width: u32,
    height: u32,
}

fn crop_frozen_png(source: FrozenPngSource, selection: RectDto) -> Result<CroppedPng, String> {
    let values = [
        selection.x,
        selection.y,
        selection.width,
        selection.height,
        source.display_width,
        source.display_height,
        source.scale,
    ];
    if values.iter().any(|value| !value.is_finite())
        || selection.x < 0.0
        || selection.y < 0.0
        || selection.width <= 0.0
        || selection.height <= 0.0
        || source.display_width <= 0.0
        || source.display_height <= 0.0
        || source.scale <= 0.0
        || selection.x + selection.width > source.display_width + 0.01
        || selection.y + selection.height > source.display_height + 0.01
    {
        return Err("The OCR selection is invalid.".into());
    }
    let image = image::load_from_memory_with_format(&source.png, image::ImageFormat::Png)
        .map_err(|_| "The frozen capture image is invalid.".to_string())?;
    let actual_width = image.width();
    let actual_height = image.height();
    if u32::try_from(source.declared_width).ok() != Some(actual_width)
        || u32::try_from(source.declared_height).ok() != Some(actual_height)
    {
        return Err("The frozen capture dimensions do not match.".into());
    }

    let left = (selection.x * source.scale).floor().max(0.0) as u32;
    let top = (selection.y * source.scale).floor().max(0.0) as u32;
    let right = ((selection.x + selection.width) * source.scale)
        .ceil()
        .min(actual_width as f64) as u32;
    let bottom = ((selection.y + selection.height) * source.scale)
        .ceil()
        .min(actual_height as f64) as u32;
    let width = right.saturating_sub(left);
    let height = bottom.saturating_sub(top);
    if width == 0 || height == 0 || right > actual_width || bottom > actual_height {
        return Err("The OCR selection is invalid.".into());
    }
    let cropped = image.crop_imm(left, top, width, height);
    let mut output = std::io::Cursor::new(Vec::new());
    cropped
        .write_to(&mut output, image::ImageFormat::Png)
        .map_err(|_| "The OCR image could not be prepared.".to_string())?;
    Ok(CroppedPng {
        png: output.into_inner(),
        width,
        height,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_source() -> FrozenPngSource {
        let image = image::RgbaImage::from_fn(8, 6, |x, y| image::Rgba([x as u8, y as u8, 0, 255]));
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        FrozenPngSource {
            png: png.into_inner(),
            declared_width: 8,
            declared_height: 6,
            display_width: 4.0,
            display_height: 3.0,
            scale: 2.0,
        }
    }

    #[test]
    fn crops_selection_at_backing_scale() {
        let cropped = crop_frozen_png(
            fixture_source(),
            RectDto {
                x: 1.0,
                y: 0.5,
                width: 2.0,
                height: 1.5,
            },
        )
        .unwrap();
        assert_eq!((cropped.width, cropped.height), (4, 3));
        let decoded = image::load_from_memory(&cropped.png).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (4, 3));
    }

    #[test]
    fn rejects_out_of_bounds_and_dimension_mismatch() {
        assert!(crop_frozen_png(
            fixture_source(),
            RectDto {
                x: 3.0,
                y: 0.0,
                width: 2.0,
                height: 1.0,
            },
        )
        .is_err());

        let mut source = fixture_source();
        source.declared_width = 9;
        assert!(crop_frozen_png(
            source,
            RectDto {
                x: 0.0,
                y: 0.0,
                width: 1.0,
                height: 1.0,
            },
        )
        .is_err());
    }
}
