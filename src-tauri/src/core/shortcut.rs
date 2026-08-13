//! CaptureShortcut — port of Sources/KiriCore/CaptureShortcut.swift.
//! The capture shortcut is exclusively Shift-Command-A on macOS and its
//! equivalent Shift-Control-A on Windows.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShortcutModifier {
    Control,
    Option,
    Shift,
    Command,
}

impl ShortcutModifier {
    pub fn glyph(self) -> &'static str {
        match self {
            ShortcutModifier::Control => "⌃",
            ShortcutModifier::Option => "⌥",
            ShortcutModifier::Shift => "⇧",
            ShortcutModifier::Command => "⌘",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaptureShortcut {
    pub key: char,
    pub modifiers: &'static [ShortcutModifier],
}

pub const KIRI_CAPTURE: CaptureShortcut = CaptureShortcut {
    key: 'a',
    modifiers: &[ShortcutModifier::Shift, ShortcutModifier::Command],
};

impl CaptureShortcut {
    pub fn display_label(&self) -> String {
        let prefix: String = self
            .modifiers
            .iter()
            .map(|m| m.glyph())
            .collect::<Vec<_>>()
            .join("");
        format!("{prefix}{}", self.key.to_ascii_uppercase())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kiri_shortcut_is_shift_command_a() {
        assert_eq!(KIRI_CAPTURE.key, 'a');
        assert_eq!(
            KIRI_CAPTURE.modifiers,
            &[ShortcutModifier::Shift, ShortcutModifier::Command]
        );
    }

    #[test]
    fn display_label_matches_swift_glyph_order() {
        // Swift iterates allCases: control, option, shift, command.
        assert_eq!(KIRI_CAPTURE.display_label(), "⇧⌘A");
    }
}
