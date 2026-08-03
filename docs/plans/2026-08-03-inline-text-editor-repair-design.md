# Inline Text Editor Repair Design

## Goal

Text annotations should accept normal and complex text input reliably, remain
readable while typing, and render in the exported capture where the user placed
them.

## Design

Replace the fixed-size `NSTextField` overlay with a purpose-built `NSTextView`.
The text view uses AppKit's native text input system, including marked-text
composition used by Chinese and Japanese input methods. It grows with the entered
content within the selected image bounds and uses the selected annotation color
for text, caret, border, and a contrast-aware background.

Return commits the active text and confirms the capture, matching Kiri's global
Return behavior. Escape continues to cancel the whole capture at the overlay
window level. Switching tools or clicking another text position commits the
current text first. Undo commits pending text before changing history, while Clear
discards an unfinished editor.

## Verification

- Build with warnings treated as errors and run all core tests.
- Verify ASCII, Chinese text, spaces, and long strings remain visible while typing.
- Confirm Return exports the committed text at the same visual position.
- Confirm Escape cancels from active text input without creating a capture.
