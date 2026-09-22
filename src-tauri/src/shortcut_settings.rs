//! Native capture shortcut preference and replacement transaction.
use std::{
    io::Write,
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Mutex,
    },
};
use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut};

pub struct CaptureBinding {
    current: Mutex<Shortcut>,
    id: AtomicU32,
    editing: AtomicBool,
}

impl CaptureBinding {
    pub fn new(shortcut: Shortcut) -> Self {
        Self {
            current: Mutex::new(shortcut),
            id: AtomicU32::new(shortcut.id()),
            editing: AtomicBool::new(false),
        }
    }

    // The plugin invokes its handler while holding its registration map lock.
    // Never acquire the replacement mutex from that callback (lock inversion).
    pub fn matches(&self, shortcut: &Shortcut) -> bool {
        self.id.load(Ordering::Acquire) == shortcut.id()
    }

    pub fn set_editing(&self, editing: bool) {
        self.editing.store(editing, Ordering::Release);
    }

    pub fn is_editing(&self) -> bool {
        self.editing.load(Ordering::Acquire)
    }
}

pub fn default_binding() -> Shortcut {
    let primary = if cfg!(target_os = "macos") {
        Modifiers::SUPER
    } else {
        Modifiers::CONTROL
    };
    Shortcut::new(Some(primary | Modifiers::SHIFT), Code::KeyA)
}

pub fn parse(value: &str) -> Result<Shortcut, String> {
    let shortcut: Shortcut = value
        .parse()
        .map_err(|_| "Invalid capture shortcut.".to_string())?;
    let key = shortcut.key.to_string();
    let letter = key
        .strip_prefix("Key")
        .is_some_and(|s| s.len() == 1 && s.as_bytes()[0].is_ascii_uppercase());
    let digit = key
        .strip_prefix("Digit")
        .is_some_and(|s| s.len() == 1 && s.as_bytes()[0].is_ascii_digit());
    if !(letter || digit)
        || !shortcut
            .mods
            .intersects(Modifiers::CONTROL | Modifiers::ALT | Modifiers::SUPER)
    {
        return Err("Use Control, Alt, or Command with a letter or number.".into());
    }
    Ok(shortcut)
}

pub fn current(app: &AppHandle) -> Shortcut {
    *app.state::<CaptureBinding>().current.lock().unwrap()
}

pub fn load(app: &AppHandle) -> Shortcut {
    app.path()
        .app_config_dir()
        .ok()
        .and_then(|dir| std::fs::read_to_string(dir.join("capture-shortcut.txt")).ok())
        .and_then(|value| parse(value.trim()).ok())
        .unwrap_or_else(default_binding)
}

pub fn label(shortcut: Shortcut) -> String {
    if shortcut == default_binding() {
        return crate::core::shortcut::KIRI_CAPTURE.display_label();
    }
    let mac = cfg!(target_os = "macos");
    let mut tokens = Vec::new();
    for (modifier, symbol, name) in [
        (Modifiers::SHIFT, "⇧", "Shift"),
        (Modifiers::CONTROL, "⌃", "Ctrl"),
        (Modifiers::ALT, "⌥", "Alt"),
        (Modifiers::SUPER, "⌘", "Win"),
    ] {
        if shortcut.mods.contains(modifier) {
            tokens.push(if mac { symbol } else { name }.to_string());
        }
    }
    let key = shortcut.key.to_string();
    tokens.push(
        key.strip_prefix("Key")
            .or_else(|| key.strip_prefix("Digit"))
            .unwrap_or(&key)
            .to_string(),
    );
    tokens.join(if mac { "" } else { "+" })
}

pub fn replace(app: &AppHandle, value: Option<&str>) -> Result<(), String> {
    let candidate = value
        .map(parse)
        .transpose()?
        .unwrap_or_else(default_binding);
    let state = app.state::<CaptureBinding>();
    let mut active = state.current.lock().unwrap();
    let previous = *active;
    let manager = app.global_shortcut();
    if candidate == previous && manager.is_registered(candidate) {
        return Ok(());
    }
    // Stage the preference before touching either registration. Persist uses an
    // atomic replacement on both platforms, including an existing destination.
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|_| "Could not save the capture shortcut.".to_string())?;
    let prepare = || -> std::io::Result<tempfile::NamedTempFile> {
        std::fs::create_dir_all(&dir)?;
        let mut file = tempfile::NamedTempFile::new_in(&dir)?;
        file.write_all(candidate.to_string().as_bytes())?;
        file.as_file().sync_all()?;
        Ok(file)
    };
    let staged = prepare().map_err(|_| "Could not save the capture shortcut.".to_string())?;
    manager
        .register(candidate)
        .map_err(|_| "This shortcut is unavailable. Choose another combination.".to_string())?;
    let previous_registered = previous != candidate && manager.is_registered(previous);
    if previous_registered {
        if let Err(error) = manager.unregister(previous) {
            let _ = manager.unregister(candidate);
            log::warn!("Could not release previous capture shortcut: {error}");
            return Err("Could not change the capture shortcut.".into());
        }
    }
    if staged.persist(dir.join("capture-shortcut.txt")).is_err() {
        let _ = manager.unregister(candidate);
        if previous_registered {
            if let Err(error) = manager.register(previous) {
                log::warn!("Could not restore capture shortcut: {error}");
            }
        }
        return Err("Could not save the capture shortcut.".into());
    }
    *active = candidate;
    state.id.store(candidate.id(), Ordering::Release);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_typing_and_modifier_only_bindings() {
        for value in ["KeyA", "Shift+KeyA", "Control", "Control+Enter", "garbage"] {
            assert!(parse(value).is_err(), "{value}");
        }
    }
    #[test]
    fn accepts_modified_letters_and_numbers_and_roundtrips() {
        for value in ["Control+Shift+KeyA", "Alt+Digit7", "Super+KeyK"] {
            let shortcut = parse(value).unwrap();
            assert_eq!(parse(&shortcut.to_string()).unwrap(), shortcut);
        }
        assert_eq!(
            label(default_binding()),
            crate::core::shortcut::KIRI_CAPTURE.display_label()
        );
    }
}
