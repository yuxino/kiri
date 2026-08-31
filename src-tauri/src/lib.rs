// objc2-generated delegates mirror Objective-C selector names.
#![allow(non_snake_case)]
#![allow(non_camel_case_types)]
#![allow(non_upper_case_globals)]

mod capture;
mod commands;
mod core;
mod diagnostics;
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
mod updates;

use tauri::{Emitter, Manager};

use crate::state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    diagnostics::init();
    log::info!(
        "[app] process starting version={} pid={} platform={}",
        env!("CARGO_PKG_VERSION"),
        std::process::id(),
        std::env::consts::OS
    );
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_single_instance::init(|app, args, cwd| {
            log::info!(
                "[single-instance] reopen requested args={} cwd_present={}",
                args.len(),
                !cwd.is_empty()
            );
            if let Err(error) = show_library_window(app, "single-instance") {
                log::error!("[single-instance] library reopen failed: {error}");
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
        .register_asynchronous_uri_scheme_protocol("kiri", |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            // WebKit starts custom-scheme tasks on the application thread on
            // macOS. Thumbnail decoding and media I/O must not block that
            // thread, especially while a native save panel is closing.
            std::mem::drop(tauri::async_runtime::spawn_blocking(move || {
                responder.respond(protocol::handle(&app, &request));
            }));
        })
        .setup(|app| {
            log::info!("[app] setup beginning");
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
            let appearance = state::load_annotation_appearance(app.handle());
            *state.saved_annotation_appearance.lock().unwrap() = appearance;
            let options = state::load_recording_options(app.handle());
            *state.saved_recording_options.lock().unwrap() = options;
            app.manage(state);
            show_library_window(app.handle(), "startup").map_err(anyhow::Error::msg)?;

            // A conflicting system-wide shortcut must not prevent Kiri from
            // opening. Settings surfaces the unavailable binding and lets the
            // user retry after releasing the conflict.
            if let Err(error) = register_shortcut(app.handle()) {
                log::warn!("[shortcut] registration failed: {error}");
            }
            install_tray(app.handle())?;
            log::info!("[app] setup complete");
            Ok(())
        })
        .on_window_event(|window, event| {
            // The library window hides instead of closing (single-instance
            // Dock/taskbar app); capture sessions close it programmatically.
            match event {
                tauri::WindowEvent::CloseRequested { .. } => {}
                tauri::WindowEvent::Destroyed => {
                    log::info!("[window] destroyed label={}", window.label());
                    if let Some(state) = window.app_handle().try_state::<AppState>() {
                        let label = window.label();
                        state.editor_annotations.lock().unwrap().remove(label);
                        state.editor_save_destinations.lock().unwrap().remove(label);
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
                    commands::finalize_confirmed_capture_after_overlay_destroyed(
                        window.app_handle(),
                        window.label(),
                    );
                }
                _ => {}
            }
            if window.label() == "library" {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    log::info!("[window] library close requested; hiding resident window");
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_assets,
            commands::get_asset,
            commands::get_library_status,
            commands::get_asset_availability,
            commands::choose_library_location,
            commands::locate_library,
            commands::restore_default_library,
            commands::retry_library,
            commands::reveal_library,
            commands::restore_missing_asset,
            commands::remove_missing_asset,
            commands::list_pending_recordings,
            commands::retry_pending_recordings,
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
            commands::open_editor,
            commands::reveal_asset,
            commands::convert_to_gif,
            commands::start_capture,
            commands::cancel_capture,
            commands::prepare_capture_annotation,
            commands::confirm_capture,
            commands::save_file_dialog,
            commands::get_asset_annotation_project,
            commands::prepare_asset_annotation,
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
            commands::get_shortcut_status,
            commands::retry_shortcut,
            commands::open_settings,
            commands::quit_app,
            commands::get_recording_options,
            commands::set_recording_options,
            commands::get_annotation_appearance,
            commands::set_annotation_appearance,
            updates::check_for_updates,
            updates::open_release_page,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if matches!(&event, tauri::RunEvent::Exit) {
                log::info!("[app] process exiting");
            }
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = event {
                // Dock icon click (macOS): bring the library back.
                if let Err(error) = show_library_window(app, "dock-reopen") {
                    log::error!("[app] Dock reopen failed: {error}");
                }
            }
            #[cfg(not(target_os = "macos"))]
            let _ = (app, event);
        });
}

fn show_library_window(app: &tauri::AppHandle, reason: &str) -> Result<(), String> {
    let window = match app.get_webview_window("library") {
        Some(window) => window,
        None => {
            log::warn!("[window] library missing; recreating reason={reason}");
            tauri::WebviewWindowBuilder::new(
                app,
                "library",
                tauri::WebviewUrl::App("index.html?window=library".into()),
            )
            .title("kiri")
            .inner_size(960.0, 640.0)
            .min_inner_size(820.0, 540.0)
            .center()
            .resizable(true)
            .build()
            .map_err(|error| format!("library window could not be recreated: {error}"))?
        }
    };
    window
        .show()
        .map_err(|error| format!("library window could not be shown: {error}"))?;
    window
        .unminimize()
        .map_err(|error| format!("library window could not be restored: {error}"))?;
    window
        .set_focus()
        .map_err(|error| format!("library window could not be focused: {error}"))?;
    log::info!("[window] library visible reason={reason}");
    Ok(())
}

#[cfg(all(target_os = "macos", not(debug_assertions)))]
fn install_macos_app_icon() -> std::io::Result<()> {
    use objc2::{AllocAnyThread, MainThreadMarker};
    use objc2_app_kit::{NSApplication, NSImage};
    use objc2_foundation::NSData;

    // The app bundle still carries the full multi-resolution ICNS. The bare
    // release binary only needs a crisp 256 px Dock image at runtime, so avoid
    // embedding the much larger ICNS a second time here.
    const APP_ICON: &[u8] = include_bytes!("../icons/128x128@2x.png");
    let main_thread = MainThreadMarker::new()
        .ok_or_else(|| std::io::Error::other("app icon must be installed on the main thread"))?;
    let data = NSData::with_bytes(APP_ICON);
    let icon = NSImage::initWithData(NSImage::alloc(), &data)
        .ok_or_else(|| std::io::Error::other("embedded app icon is invalid"))?;
    let application = NSApplication::sharedApplication(main_thread);
    unsafe { application.setApplicationIconImage(Some(&icon)) };
    Ok(())
}

pub(crate) fn register_shortcut(app: &tauri::AppHandle) -> tauri::Result<()> {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;
    let shortcut = capture_shortcut();
    app.global_shortcut()
        .register(shortcut)
        .map_err(|e| tauri::Error::Anyhow(e.into()))?;
    log::info!(
        "[shortcut] registered {}",
        crate::core::shortcut::KIRI_CAPTURE.display_label()
    );
    Ok(())
}

fn capture_shortcut() -> tauri_plugin_global_shortcut::Shortcut {
    use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut};
    // macOS: Command+Shift+A; Windows: Control+Shift+A.
    #[cfg(target_os = "macos")]
    let modifiers = Modifiers::SUPER | Modifiers::SHIFT;
    #[cfg(not(target_os = "macos"))]
    let modifiers = Modifiers::CONTROL | Modifiers::SHIFT;
    Shortcut::new(Some(modifiers), Code::KeyA)
}

pub(crate) fn capture_shortcut_is_registered(app: &tauri::AppHandle) -> bool {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;
    app.global_shortcut().is_registered(capture_shortcut())
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
        use objc2_core_graphics::{CGDisplayBounds, CGMainDisplayID};
        // Use the fixed Core Graphics main display, not NSScreen.mainScreen:
        // the latter follows the key window and changes when a fullscreen app
        // owns a secondary display.
        let bounds = CGDisplayBounds(CGMainDisplayID());
        bounds.origin.y + bounds.size.height
    };
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
const MAIN_TRAY_ID: &str = "main-tray";

fn tray_menu_entries(language: &str) -> [(&'static str, &'static str); 3] {
    let (open_label, capture_label, quit_label) = tray_labels(language);
    [
        ("open-library", open_label),
        ("capture", capture_label),
        ("quit", quit_label),
    ]
}

fn build_tray_menu(
    app: &tauri::AppHandle,
    language: &str,
) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    use tauri::menu::{Menu, MenuItem};

    let [(open_id, open_label), (capture_id, capture_label), (quit_id, quit_label)] =
        tray_menu_entries(language);
    let open_library = MenuItem::with_id(app, open_id, open_label, true, None::<&str>)?;
    let capture = MenuItem::with_id(app, capture_id, capture_label, true, None::<&str>)?;
    let quit = MenuItem::with_id(app, quit_id, quit_label, true, None::<&str>)?;
    Menu::with_items(app, &[&open_library, &capture, &quit])
}

pub(crate) fn refresh_tray_menu(app: &tauri::AppHandle, language: &str) -> Result<(), String> {
    let tray = app
        .tray_by_id(MAIN_TRAY_ID)
        .ok_or_else(|| "The Kiri tray icon is unavailable.".to_string())?;
    let menu = build_tray_menu(app, language).map_err(|error| error.to_string())?;
    tray.set_menu(Some(menu)).map_err(|error| error.to_string())
}

fn install_tray(app: &tauri::AppHandle) -> tauri::Result<()> {
    use tauri::tray::TrayIconBuilder;

    // Follow the same persisted preference and OS-locale fallback as the
    // frontend. Explorer-launched Windows apps normally have no LANG/LC_ALL,
    // so environment variables cannot represent the user's display language.
    let selected_language = state::load_language(app);
    let language = if selected_language.is_empty() {
        commands::get_locale()
    } else {
        selected_language
    };
    let menu = build_tray_menu(app, &language)?;

    // macOS tints the monochrome template for either menu-bar appearance.
    // Windows has no template-image rendering, so use a dedicated light,
    // colored icon that stays visible on dark and light taskbars.
    let icon = {
        #[cfg(target_os = "macos")]
        let bytes = include_bytes!("../icons/tray-viewfinder.png");
        #[cfg(not(target_os = "macos"))]
        let bytes = include_bytes!("../icons/tray-viewfinder-windows.png");
        tauri::image::Image::from_bytes(bytes).ok()
    };
    let mut builder = TrayIconBuilder::with_id(MAIN_TRAY_ID)
        .menu(&menu)
        .tooltip("Kiri")
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open-library" => {
                if let Err(error) = show_library_window(app, "tray") {
                    log::error!("[tray] library open failed: {error}");
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
    #[cfg(target_os = "macos")]
    {
        builder = builder.icon_as_template(true);
    }
    if let Some(icon) = icon {
        builder = builder.icon(icon);
    }
    builder.build(app)?;
    Ok(())
}

fn tray_labels(language: &str) -> (&'static str, &'static str, &'static str) {
    match language {
        "zh-Hans" => ("打开素材库", "截图 / 录屏", "退出 Kiri"),
        "ja" => ("ライブラリを開く", "キャプチャ", "Kiri を終了"),
        _ => ("Open Library", "Capture", "Quit Kiri"),
    }
}

#[cfg(test)]
mod tests {
    use super::{tray_labels, tray_menu_entries};

    #[test]
    fn tray_labels_cover_supported_languages_and_default_to_english() {
        assert_eq!(tray_labels("en"), ("Open Library", "Capture", "Quit Kiri"));
        assert_eq!(
            tray_labels("zh-Hans"),
            ("打开素材库", "截图 / 录屏", "退出 Kiri")
        );
        assert_eq!(
            tray_labels("ja"),
            ("ライブラリを開く", "キャプチャ", "Kiri を終了")
        );
        assert_eq!(
            tray_labels("unknown"),
            ("Open Library", "Capture", "Quit Kiri")
        );
    }

    #[test]
    fn tray_language_switch_keeps_action_ids_and_replaces_every_label() {
        let english = tray_menu_entries("en");
        let chinese = tray_menu_entries("zh-Hans");
        let japanese = tray_menu_entries("ja");
        assert_eq!(english.map(|(id, _)| id), chinese.map(|(id, _)| id));
        assert_eq!(english.map(|(id, _)| id), japanese.map(|(id, _)| id));
        for index in 0..english.len() {
            assert_ne!(english[index].1, chinese[index].1);
            assert_ne!(english[index].1, japanese[index].1);
        }
    }

    fn luminance(rgb: [f64; 3]) -> f64 {
        let channel = |value: f64| {
            let value = value / 255.0;
            if value <= 0.04045 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(rgb[0]) + 0.7152 * channel(rgb[1]) + 0.0722 * channel(rgb[2])
    }

    fn contrast_ratio(left: [f64; 3], right: [f64; 3]) -> f64 {
        let (lighter, darker) = {
            let left = luminance(left);
            let right = luminance(right);
            if left >= right {
                (left, right)
            } else {
                (right, left)
            }
        };
        (lighter + 0.05) / (darker + 0.05)
    }

    fn contrasting_tray_pixels(background: u8) -> usize {
        let source =
            image::load_from_memory(include_bytes!("../icons/tray-viewfinder-windows.png"))
                .unwrap()
                .to_rgba8();
        let icon = image::imageops::resize(&source, 16, 16, image::imageops::FilterType::Lanczos3);
        icon.pixels()
            .filter(|pixel| {
                let alpha = f64::from(pixel[3]) / 255.0;
                let composite = [
                    f64::from(pixel[0]) * alpha + f64::from(background) * (1.0 - alpha),
                    f64::from(pixel[1]) * alpha + f64::from(background) * (1.0 - alpha),
                    f64::from(pixel[2]) * alpha + f64::from(background) * (1.0 - alpha),
                ];
                contrast_ratio(composite, [f64::from(background); 3]) >= 3.0
            })
            .count()
    }

    #[test]
    fn windows_tray_icon_has_three_to_one_contrast_on_light_and_dark_taskbars() {
        assert!(
            contrasting_tray_pixels(245) >= 24,
            "16px tray icon needs a dark silhouette on a light taskbar"
        );
        assert!(
            contrasting_tray_pixels(20) >= 24,
            "16px tray icon needs a light core on a dark taskbar"
        );
    }
}
