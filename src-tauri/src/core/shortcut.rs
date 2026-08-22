//! Platform-specific capture shortcut model.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShortcutModifier {
    #[cfg(windows)]
    Control,
    Shift,
    #[cfg(target_os = "macos")]
    Command,
}

impl ShortcutModifier {
    fn display_token(self) -> &'static str {
        match self {
            #[cfg(windows)]
            ShortcutModifier::Control => "Ctrl",
            #[cfg(target_os = "macos")]
            ShortcutModifier::Shift => "⇧",
            #[cfg(windows)]
            ShortcutModifier::Shift => "Shift",
            #[cfg(target_os = "macos")]
            ShortcutModifier::Command => "⌘",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaptureShortcut {
    pub key: char,
    pub modifiers: &'static [ShortcutModifier],
}

#[cfg(target_os = "macos")]
pub const KIRI_CAPTURE: CaptureShortcut = CaptureShortcut {
    key: 'a',
    modifiers: &[ShortcutModifier::Shift, ShortcutModifier::Command],
};

#[cfg(windows)]
pub const KIRI_CAPTURE: CaptureShortcut = CaptureShortcut {
    key: 'a',
    modifiers: &[ShortcutModifier::Shift, ShortcutModifier::Control],
};

impl CaptureShortcut {
    pub fn display_label(&self) -> String {
        let prefix: String = self
            .modifiers
            .iter()
            .map(|m| m.display_token())
            .collect::<Vec<_>>()
            .join(if cfg!(windows) { "+" } else { "" });
        let separator = if cfg!(windows) && !prefix.is_empty() {
            "+"
        } else {
            ""
        };
        format!("{prefix}{separator}{}", self.key.to_ascii_uppercase())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(target_os = "macos")]
    fn kiri_shortcut_is_shift_command_a() {
        assert_eq!(KIRI_CAPTURE.key, 'a');
        assert_eq!(
            KIRI_CAPTURE.modifiers,
            &[ShortcutModifier::Shift, ShortcutModifier::Command]
        );
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn macos_display_label_uses_platform_glyphs() {
        assert_eq!(KIRI_CAPTURE.display_label(), "⇧⌘A");
    }

    #[test]
    #[cfg(windows)]
    fn kiri_shortcut_is_shift_control_a() {
        assert_eq!(KIRI_CAPTURE.key, 'a');
        assert_eq!(
            KIRI_CAPTURE.modifiers,
            &[ShortcutModifier::Shift, ShortcutModifier::Control]
        );
        assert_eq!(KIRI_CAPTURE.display_label(), "Shift+Ctrl+A");
    }
}
