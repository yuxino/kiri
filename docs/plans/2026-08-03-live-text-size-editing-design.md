# Live Text Size Editing Design

## Goal

Allow a text annotation's font size to change after text entry has started or
after an existing text annotation is reopened.

## Interaction

- The existing font-size slider remains the single size control.
- Clicking the slider first commits the inline editor without losing the text
  annotation selection.
- Dragging previews the selected text at the new size immediately.
- The text bounds are recalculated as the font changes so glyphs are not clipped.
- Releasing the slider records the entire drag as one undoable edit.
- Keyboard-driven slider changes remain immediately undoable.
- The behavior is identical in the capture overlay and the full editor.

## Safety and history

Opening an existing text annotation and leaving its contents unchanged no
longer creates a no-op history entry. Font-size changes preserve the text,
color, background style, and top-left position. Undo and redo operate on the
completed size adjustment rather than every intermediate slider value.
