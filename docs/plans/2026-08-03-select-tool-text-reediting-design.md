# Select Tool and Text Re-editing

## Goal

Add the missing pointer tool and make existing text annotations selectable, movable, and editable again.

## Interaction

- The pointer is the first annotation tool and appears selected while the capture region is still adjustable.
- Choosing a drawing tool locks the region as before.
- Returning to the pointer tool enters annotation selection mode.
- Click text to select it, drag selected text to move it, double-click it to reopen the inline editor, and click empty space to clear selection.
- Reopened text keeps its text, color, background style, font size, and position.

## State and history

- Track the selected text mark index and a temporary translated preview while dragging.
- Keep the original mark hidden while its inline editor is open.
- Extend annotation history with an indexed replacement operation backed by before/after snapshots, so moving or editing text is a single undoable action.

## Toolbar behavior

- Add a pointer icon to the compact screenshot toolbar and full editor.
- The pointer has no size controls or color palette of its own.
- Use `V` as its keyboard shortcut.

## Acceptance criteria

- Pointer is visible and initially highlighted.
- Existing text shows a visible selection outline when clicked.
- Text can be moved and double-clicked for editing without duplicating the old mark.
- Undo restores the previous text or position; redo reapplies it.
- Existing region resize, drawing, Enter, Escape, export, and mosaic behavior remain intact.
