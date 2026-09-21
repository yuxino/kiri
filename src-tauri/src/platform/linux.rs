//! Linux platform helpers: focus restoration, file reveal, click monitoring
//! stubs, capture-exclusion stubs, and Wayland compositor hotkeys.
//!
//! `tauri-plugin-global-shortcut` / `global-hotkey` only grab keys under X11.
//! On Hyprland the grab "succeeds" via XWayland but never receives Wayland
//! key events, so we also install a compositor bind that pokes a FIFO Kiri
//! listens on.

use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use anyhow::{anyhow, Result};

use super::{ClickMonitorHandle, MicrophoneAccess};

static COMPOSITOR_HOTKEY_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Focus restoration is best-effort on Wayland; many compositors deny
/// application-driven activation. A no-op keeps the capture contract intact.
pub fn activate_application(_pid: u32) {}

pub fn reveal_path(path: &Path) {
    let target = if path.is_dir() {
        path.to_path_buf()
    } else {
        path.parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| path.to_path_buf())
    };
    let _ = Command::new("xdg-open").arg(&target).spawn();
}

/// Wayland does not expose a portable frontmost-application API to sandboxed
/// apps. Returning `None` skips focus restoration metadata without failing capture.
pub fn frontmost_application() -> Option<(u32, Option<String>)> {
    None
}

pub fn mic_supported() -> bool {
    false
}

pub fn request_microphone_access() -> Result<MicrophoneAccess> {
    Ok(MicrophoneAccess::Unsupported)
}

pub fn set_window_click_through(_app: &tauri::AppHandle, _label: &str) {
    // GTK/WebKit click-through is not required for the screenshot MVP.
}

pub fn set_window_capture_excluded(_app: &tauri::AppHandle, _label: &str, _excluded: bool) {
    // Portal ScreenCast cannot reliably exclude sibling windows yet.
}

/// Global click monitoring for the recorded ripple requires compositor support
/// that is not portable across Wayland sessions; refuse rather than half-work.
pub fn start_click_monitor(
    _callback: Arc<dyn Fn(f64, f64) + Send + Sync>,
) -> Result<Box<dyn ClickMonitorHandle + Send>> {
    Err(anyhow!(
        "Global click highlights are not available on this Linux session yet."
    ))
}

pub fn compositor_hotkey_is_active() -> bool {
    COMPOSITOR_HOTKEY_ACTIVE.load(Ordering::Acquire)
}

/// Install a Hyprland bind for Shift+Ctrl+A that wakes Kiri through a FIFO.
/// No-op on non-Hyprland sessions. Returns whether a compositor bind was installed.
pub fn install_compositor_capture_hotkey<F>(on_trigger: F) -> Result<bool>
where
    F: Fn() + Send + Sync + 'static,
{
    if std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_none() {
        return Ok(false);
    }

    let fifo_path = capture_trigger_fifo_path()?;
    ensure_fifo(&fifo_path)?;

    let reader_path = fifo_path.clone();
    let on_trigger = Arc::new(on_trigger);
    thread::Builder::new()
        .name("kiri-hypr-hotkey".into())
        .spawn(move || listen_capture_fifo(reader_path, on_trigger))
        .map_err(|error| anyhow!("Could not start the Hyprland hotkey listener: {error}"))?;

    install_hyprland_bind(&fifo_path)?;

    COMPOSITOR_HOTKEY_ACTIVE.store(true, Ordering::Release);
    log::info!(
        "[shortcut] Hyprland bind installed for Shift+Ctrl+A via {}",
        fifo_path.display()
    );
    Ok(true)
}

fn install_hyprland_bind(fifo_path: &Path) -> Result<()> {
    let fifo = fifo_path.display().to_string();
    // exec_cmd runs through `sh -c`. Prefer single quotes around the path.
    let shell_cmd = format!("printf x > '{fifo}'");
    let lua_shell = shell_cmd.replace('\\', "\\\\").replace('"', "\\\"");
    let lua = format!("hl.bind(\"CTRL + SHIFT + A\", hl.dsp.exec_cmd(\"{lua_shell}\"))");

    // Modern Hyprland (Lua config): keyword is rejected; use eval.
    let eval = Command::new("hyprctl")
        .args(["eval", &lua])
        .output()
        .map_err(|error| anyhow!("hyprctl eval could not be started: {error}"))?;
    let eval_out = combined_output(&eval);
    if hyprctl_succeeded(&eval_out) {
        return Ok(());
    }

    // Legacy parser fallback.
    let legacy = Command::new("hyprctl")
        .args([
            "keyword",
            "bind",
            &format!("CTRL SHIFT, A, exec, sh -c \"printf x > '{fifo}'\""),
        ])
        .output()
        .map_err(|error| anyhow!("hyprctl keyword could not be started: {error}"))?;
    let legacy_out = combined_output(&legacy);
    if hyprctl_succeeded(&legacy_out) {
        return Ok(());
    }

    Err(anyhow!(
        "Could not install Hyprland Shift+Ctrl+A bind. eval: {eval_out}; keyword: {legacy_out}"
    ))
}

fn combined_output(output: &std::process::Output) -> String {
    let mut text = String::new();
    text.push_str(&String::from_utf8_lossy(&output.stdout));
    if !output.stderr.is_empty() {
        if !text.is_empty() {
            text.push(' ');
        }
        text.push_str(&String::from_utf8_lossy(&output.stderr));
    }
    text.trim().to_string()
}

fn hyprctl_succeeded(output: &str) -> bool {
    let trimmed = output.trim();
    !trimmed.is_empty()
        && trimmed.eq_ignore_ascii_case("ok")
        && !trimmed.to_ascii_lowercase().contains("can't work")
        && !trimmed.to_ascii_lowercase().contains("error")
}

fn capture_trigger_fifo_path() -> Result<PathBuf> {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    let dir = runtime.join("kiri");
    fs::create_dir_all(&dir)
        .map_err(|error| anyhow!("Could not create {}: {error}", dir.display()))?;
    Ok(dir.join("capture-trigger"))
}

fn ensure_fifo(path: &Path) -> Result<()> {
    if path.exists() {
        let meta = fs::metadata(path)
            .map_err(|error| anyhow!("Could not stat {}: {error}", path.display()))?;
        if !is_fifo(&meta) {
            fs::remove_file(path).map_err(|error| {
                anyhow!("Could not replace non-FIFO {}: {error}", path.display())
            })?;
        } else {
            return Ok(());
        }
    }
    let status = Command::new("mkfifo")
        .args(["-m", "600"])
        .arg(path)
        .status()
        .map_err(|error| anyhow!("mkfifo could not be started: {error}"))?;
    if !status.success() && !path.exists() {
        return Err(anyhow!(
            "Could not create FIFO {} (exit {status})",
            path.display()
        ));
    }
    Ok(())
}

fn is_fifo(meta: &fs::Metadata) -> bool {
    use std::os::unix::fs::FileTypeExt;
    meta.file_type().is_fifo()
}

fn listen_capture_fifo(path: PathBuf, on_trigger: Arc<dyn Fn() + Send + Sync>) {
    loop {
        match File::open(&path) {
            Ok(mut file) => {
                let mut buf = [0_u8; 64];
                loop {
                    match file.read(&mut buf) {
                        Ok(0) => break,
                        Ok(_) => {
                            log::info!("[shortcut] Hyprland capture bind pressed");
                            on_trigger();
                            thread::sleep(Duration::from_millis(250));
                        }
                        Err(error) => {
                            log::warn!(
                                "[shortcut] Hyprland FIFO read failed ({}): {error}",
                                path.display()
                            );
                            thread::sleep(Duration::from_millis(500));
                            break;
                        }
                    }
                }
            }
            Err(error) => {
                log::warn!(
                    "[shortcut] could not open Hyprland FIFO {}: {error}",
                    path.display()
                );
                thread::sleep(Duration::from_secs(1));
            }
        }
    }
}
