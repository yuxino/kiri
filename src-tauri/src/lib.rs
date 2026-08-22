// objc2-generated delegates mirror Objective-C selector names.
#![allow(non_snake_case)]
#![allow(non_camel_case_types)]
#![allow(non_upper_case_globals)]

mod capture;
mod commands;
mod core;
mod gif;
mod ocr;
mod ocr_commands;
mod ocr_controller;
mod platform;
mod protocol;
mod record;
mod remote_ocr;
mod state;
mod thumbnail;

use tauri::{Emitter, Manager};

use crate::state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::init();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("library") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    use tauri_plugin_global_shortcut::ShortcutState;
                    if event.state() == ShortcutState::Pressed {
                        log::info!("[shortcut] pressed: {:?}", shortcut);
                        let handle = app.clone();
                        let trigger = handle.clone();
                        let _ = trigger.run_on_main_thread(move || {
                            let _ = commands::start_capture(handle);
                        });
                    }
                })
                .build(),
        )
        .manage(protocol::ProtocolStore::new())
        .register_uri_scheme_protocol("kiri", |ctx, request| {
            protocol::handle(ctx.app_handle(), &request)
        })
        .setup(|app| {
            // Force a regular activation policy (macOS Dock icon). A bare
            // binary launched from a terminal may otherwise drop out of the
            // Dock once every window is hidden; the library window handles
            // the Dock-click reopen.
            #[cfg(target_os = "macos")]
            {
                app.set_activation_policy(tauri::ActivationPolicy::Regular);
                // Tauri installs this icon itself in dev. Release builds also
                // set it at runtime so the Dock and bare --no-bundle binaries
                // cannot fall back to a stale or generic icon.
                #[cfg(not(debug_assertions))]
                install_macos_app_icon()?;
                log::info!("[app] activation policy = Regular (Dock icon enabled)");
            }
            let state = AppState::new(app.handle())?;
            let options = state::load_recording_options(app.handle());
            *state.saved_recording_options.lock().unwrap() = options;
            app.manage(state);
            if let Some(window) = app.get_webview_window("library") {
                let _ = window.show();
                let _ = window.set_focus();
            }

            register_shortcut(app.handle())?;
            install_tray(app.handle())?;
            Ok(())
        })
        .on_window_event(|window, event| {
            // The library window hides instead of closing (single-instance
            // Dock/taskbar app); capture sessions close it programmatically.
            match event {
                tauri::WindowEvent::CloseRequested { .. } => {}
                tauri::WindowEvent::Destroyed => {
                    if let Some(state) = window.app_handle().try_state::<AppState>() {
                        let label = window.label();
                        let destroyed_overlay = {
                            let mut capture = state.capture.lock().unwrap();
                            capture.destroy_overlay(label)
                        };
                        if let Some((capture_id, ended_session)) = destroyed_overlay {
                            state.ocr_requests.clear_owner(
                                &crate::ocr_controller::OcrRequestOwner {
                                    label: label.to_string(),
                                    capture_id,
                                },
                            );
                            if let Some(session) = ended_session {
                                commands::teardown_cancelled_capture(
                                    window.app_handle(),
                                    session,
                                    false,
                                );
                            }
                        }
                    }
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
            commands::get_asset,
            commands::set_favorite,
            commands::move_to_trash,
            commands::restore_asset,
            commands::permanently_delete,
            commands::empty_trash,
            commands::batch_move_to_trash,
            commands::batch_restore,
            commands::batch_permanently_delete,
            commands::batch_set_favorite,
            commands::copy_asset,
            commands::open_asset,
            commands::reveal_asset,
            commands::convert_to_gif,
            commands::start_capture,
            commands::cancel_capture,
            commands::confirm_capture,
            commands::save_file_dialog,
            commands::update_asset,
            commands::rename_asset,
            commands::set_tags,
            ocr_commands::get_ocr_provider_settings,
            ocr_commands::save_ocr_provider_profile,
            ocr_commands::delete_ocr_provider_profile,
            ocr_commands::set_active_ocr_engine,
            ocr_commands::prepare_ocr_request,
            ocr_commands::recognize_prepared_ocr_local,
            ocr_commands::recognize_prepared_ocr_remote,
            ocr_commands::cancel_prepared_ocr,
            commands::copy_text,
            commands::start_recording_flow,
            commands::cancel_recording_flow,
            commands::begin_recording,
            commands::pause_recording,
            commands::resume_recording,
            commands::stop_recording,
            commands::show_confirm_dialog,
            commands::mic_supported,
            commands::log_frontend_error,
            commands::get_locale,
            commands::get_language,
            commands::set_language,
            commands::get_shortcut_label,
            commands::open_settings,
            commands::quit_app,
            commands::get_recording_options,
            commands::set_recording_options,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = event {
                // Dock icon click (macOS): bring the library back.
                if let Some(window) = app.get_webview_window("library") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            #[cfg(not(target_os = "macos"))]
            let _ = (app, event);
        });
}

#[cfg(all(target_os = "macos", not(debug_assertions)))]
fn install_macos_app_icon() -> std::io::Result<()> {
    use objc2::{AllocAnyThread, MainThreadMarker};
    use objc2_app_kit::{NSApplication, NSImage};
    use objc2_foundation::NSData;

    const APP_ICON: &[u8] = include_bytes!("../icons/icon.icns");
    let main_thread = MainThreadMarker::new()
        .ok_or_else(|| std::io::Error::other("app icon must be installed on the main thread"))?;
    let data = NSData::with_bytes(APP_ICON);
    let icon = NSImage::initWithData(NSImage::alloc(), &data)
        .ok_or_else(|| std::io::Error::other("embedded app icon is invalid"))?;
    let application = NSApplication::sharedApplication(main_thread);
    unsafe { application.setApplicationIconImage(Some(&icon)) };
    Ok(())
}

fn register_shortcut(app: &tauri::AppHandle) -> tauri::Result<()> {
    use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut};
    // macOS: Command+Shift+A; Windows: Control+Shift+A.
    #[cfg(target_os = "macos")]
    let modifiers = Modifiers::SUPER | Modifiers::SHIFT;
    #[cfg(not(target_os = "macos"))]
    let modifiers = Modifiers::CONTROL | Modifiers::SHIFT;
    let shortcut = Shortcut::new(Some(modifiers), Code::KeyA);
    app.global_shortcut()
        .register(shortcut)
        .map_err(|e| tauri::Error::Anyhow(e.into()))?;
    log::info!("[shortcut] registered {modifiers:?} + A");
    Ok(())
}

/// Installs the global click monitor for the click ripple. The monitor uses
/// NSEvent.addGlobalMonitorForEventsMatchingMask, which requires the Input
/// Monitoring permission — so it is installed ON DEMAND (only while
/// recording with "highlight clicks" enabled) instead of at every launch,
/// which would make macOS prompt for the permission on each start.
pub fn ensure_click_monitor(app: &tauri::AppHandle) -> tauri::Result<()> {
    {
        let state = app.state::<AppState>();
        if state.click_monitor.lock().unwrap().is_some() {
            return Ok(());
        }
    }
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
                        && config.map(|c| c.options.highlights_clicks).unwrap_or(false),
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
            #[cfg(target_os = "macos")]
            let _ = scale;
            let payload = serde_json::json!({
                "x": gx - frame.x - region.x,
                "y": gy - frame.y - region.y,
            });
            let _ = handle.emit("ripple-click", payload);
        });
    let monitor = platform::start_click_monitor(callback).map_err(tauri::Error::Anyhow)?;
    let state = app.state::<AppState>();
    *state.click_monitor.lock().unwrap() = Some(monitor);
    Ok(())
}

/// Menu-bar (macOS) / tray (Windows) icon with Capture, Open Library, and Quit.
/// The library window and global shortcut remain the primary entry points.
fn install_tray(app: &tauri::AppHandle) -> tauri::Result<()> {
    use tauri::menu::{Menu, MenuItem};
    use tauri::tray::TrayIconBuilder;

    // Simple OS-language lookup for the tray menu (the frontend i18n dict is
    // the source of truth for the UI; the tray mirrors its strings).
    let zh = std::env::var("LANG")
        .or_else(|_| std::env::var("LC_ALL"))
        .map(|lang| lang.to_lowercase().starts_with("zh"))
        .unwrap_or(false);
    let (open_label, capture_label, quit_label) = if zh {
        ("打开素材库", "截屏", "退出 Kiri")
    } else {
        ("Open Library", "Capture", "Quit Kiri")
    };

    let open_library = MenuItem::with_id(app, "open-library", open_label, true, None::<&str>)?;
    let capture = MenuItem::with_id(app, "capture", capture_label, true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", quit_label, true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open_library, &capture, &quit])?;

    // Menu-bar icon: the Lucide Zap glyph, rendered as a black template
    // image so macOS tints it automatically for light/dark menu bars.
    let icon = {
        let bytes = include_bytes!("../icons/tray-viewfinder.png");
        tauri::image::Image::from_bytes(bytes).ok()
    };
    let mut builder = TrayIconBuilder::new()
        .menu(&menu)
        .tooltip("Kiri")
        .icon_as_template(true)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open-library" => {
                if let Some(window) = app.get_webview_window("library") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            "capture" => {
                let handle = app.clone();
                let trigger = handle.clone();
                let _ = trigger.run_on_main_thread(move || {
                    let _ = commands::start_capture(handle);
                });
            }
            "quit" => {
                app.exit(0);
            }
            _ => {}
        });
    if let Some(icon) = icon {
        builder = builder.icon(icon);
    }
    builder.build(app)?;
    Ok(())
}
