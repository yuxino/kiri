// objc2-generated delegates mirror Objective-C selector names.
#![allow(non_snake_case)]
#![allow(non_camel_case_types)]
#![allow(non_upper_case_globals)]

mod capture;
mod commands;
mod core;
mod gif;
mod ocr;
mod platform;
mod protocol;
mod record;
mod state;
mod thumbnail;

use tauri::{Emitter, Manager};

use crate::state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::init();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("library") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .manage(protocol::ProtocolStore::new())
        .register_uri_scheme_protocol("kiri", |ctx, request| {
            protocol::handle(ctx.app_handle(), &request)
        })
        .setup(|app| {
            let state = AppState::new(app.handle())?;
            let options = state::load_recording_options(app.handle());
            *state.saved_recording_options.lock().unwrap() = options;
            app.manage(state);
            app.get_webview_window("library")
                .map(|window| {
                    let _ = window.show();
                    let _ = window.set_focus();
                });

            register_shortcut(app.handle())?;
            install_click_monitor(app.handle())?;
            Ok(())
        })
.on_window_event(|window, event| {
            // The library window hides instead of closing (single-instance
            // Dock/taskbar app); capture sessions close it programmatically.
            match event {
                tauri::WindowEvent::CloseRequested { .. } => {
                }
                tauri::WindowEvent::Destroyed => {
                }
                _ => {}
            }
            if window.label() == "library" {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_assets,
            commands::set_favorite,
            commands::move_to_trash,
            commands::restore_asset,
            commands::permanently_delete,
            commands::empty_trash,
            commands::copy_asset,
            commands::open_asset,
            commands::reveal_asset,
            commands::convert_to_gif,
            commands::start_capture,
            commands::cancel_capture,
            commands::confirm_capture,
            commands::save_file_dialog,
            commands::update_asset,
            commands::recognize_text,
            commands::copy_text,
            commands::start_recording_flow,
            commands::cancel_recording_flow,
            commands::begin_recording,
            commands::pause_recording,
            commands::resume_recording,
            commands::stop_recording,
            commands::mic_supported,
            commands::log_frontend_error,
            commands::get_locale,
            commands::get_shortcut_label,
            commands::open_settings,
            commands::quit_app,
            commands::get_recording_options,
            commands::set_recording_options,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if let tauri::RunEvent::Reopen { .. } = event {
                // Dock icon click (macOS): bring the library back.
                if let Some(window) = app.get_webview_window("library") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        });
}

#[cfg(target_os = "macos")]
fn register_shortcut(app: &tauri::AppHandle) -> tauri::Result<()> {
    use crate::platform::macos;
    let handle = app.clone();
    let shortcut = macos::start_shortcut(Box::new(move || {
        // Runs on the CGEventTap callback thread; dispatch to the main thread
        // so AppKit/SCK calls stay on the main thread.
        let handle = handle.clone();
        let trigger = handle.clone();
        let _ = trigger.run_on_main_thread(move || {
            let _ = commands::start_capture(handle);
        });
    }));
    match shortcut {
        Ok(_) => Ok(()),
        Err(error) => {
            log::warn!("shortcut registration failed: {error}");
            Ok(())
        }
    }
}

#[cfg(windows)]
fn register_shortcut(app: &tauri::AppHandle) -> tauri::Result<()> {
    use crate::platform::windows;
    let handle = app.clone();
    let shortcut = windows::start_shortcut(Box::new(move || {
        let handle = handle.clone();
        let _ = std::thread::spawn(move || {
            let _ = commands::start_capture(handle);
        });
    }));
    match shortcut {
        Ok(_) => Ok(()),
        Err(error) => {
            log::warn!("shortcut registration failed: {error}");
            Ok(())
        }
    }
}

/// Installs the app-lifetime global click monitor once. The callback reads the
/// current recording region from state and forwards normalized, region-local
/// click positions to the ripple window (mirrors the Swift original's single
/// global monitor).
fn install_click_monitor(app: &tauri::AppHandle) -> tauri::Result<()> {
    #[cfg(target_os = "macos")]
    let main_height = {
        use objc2_app_kit::NSScreen;
        let mtm = objc2::MainThreadMarker::new().unwrap();
        NSScreen::mainScreen(mtm)
            .map(|screen| {
                let frame = screen.frame();
                frame.origin.y + frame.size.height
            })
            .unwrap_or(0.0)
    };
    #[cfg(windows)]
    let main_height = 0.0;

    let handle = app.clone();
    let callback: std::sync::Arc<dyn Fn(f64, f64) + Send + Sync> =
        std::sync::Arc::new(move |x, y| {
            // Read the active recording configuration.
            let (active, region, frame, scale) = {
                let state = handle.state::<AppState>();
                let recording = state.recording.lock().unwrap();
                let config = recording.configuration.as_ref();
                (
                    recording.is_recording
                        && config
                            .map(|c| c.options.highlights_clicks)
                            .unwrap_or(false),
                    config.map(|c| c.region),
                    config.map(|c| c.screen_frame),
                    config.map(|c| c.backing_scale),
                )
            };
            if !active {
                return;
            }
            let (region, frame, scale) = match (region, frame, scale) {
                (Some(r), Some(f), Some(s)) => (r, f, s),
                _ => return,
            };
            // Platform callbacks deliver platform-native global coordinates:
            // macOS Quartz bottom-left points; Windows physical pixels.
            #[cfg(target_os = "macos")]
            let (gx, gy) = (x, main_height - y);
            #[cfg(windows)]
            let (gx, gy) = (x / scale, y / scale);
            let payload = serde_json::json!({
                "x": gx - frame.x - region.x,
                "y": gy - frame.y - region.y,
            });
            let _ = handle.emit("ripple-click", payload);
        });
    let monitor = platform::start_click_monitor(callback).map_err(|e| tauri::Error::Anyhow(e.into()))?;
    let state = app.state::<AppState>();
    *state.click_monitor.lock().unwrap() = Some(monitor);
    Ok(())
}
