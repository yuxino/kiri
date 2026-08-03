# Edit All Annotation Types

## Goal

Extend the pointer tool from text-only editing to every annotation type without adding more toolbar complexity.

## Interaction

- Click the topmost visible annotation to select it; click empty space to clear selection.
- Drag the body of any selected annotation to move it within the screenshot.
- Rectangle: drag any of eight handles to resize.
- Line and arrow: drag either endpoint to change direction and length.
- Pen and mosaic: move the complete stroke as one object.
- Text: keep move and double-click re-edit behavior.
- Delete or Forward Delete removes the selected annotation.

## Visual feedback

- Draw a subtle violet-and-white dashed selection boundary.
- Use eight compact handles for rectangles, endpoint handles for lines and arrows, and a bounding outline for pen, mosaic, and text.
- Change the cursor to an open hand over movable annotations and a resize cursor over handles.

## History and bounds

- Commit one history replacement when a drag ends, not on every pointer movement.
- Clamp translations, rectangle resizing, and endpoints to the captured image.
- Deletion is a normal undoable history operation.

## Acceptance criteria

- Every mark type can be selected and moved.
- Rectangle resizing and line/arrow endpoint editing are immediately visible and export correctly.
- Undo/redo restores movement, resizing, endpoint edits, and deletion.
- Existing drawing, text re-editing, region resizing, Enter, Escape, and export behavior remains intact.
