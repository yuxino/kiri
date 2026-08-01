# Kiri Line Annotation Design

## Goal

Make common screenshot markup discoverable in one capture flow. Users should be able to draw a box, a plain connecting line, or an arrow immediately after selecting a capture region.

## Interaction

Kiri exposes one **Capture** action everywhere: the library, menu bar, application menu, and global shortcut all start the same flow. Selecting a region opens the inline annotation canvas. The toolbar exposes Pen, Rectangle, Line, Arrow, Text, and Mosaic as first-level tools. Rectangle uses `R`, Line uses `L`, and Arrow uses `A`. Every drag-based tool previews while dragging, commits on mouse-up, and participates in the existing undo/redo history. Users who do not need markup press Return or click Done immediately to copy the untouched capture.

The same tool ordering and shortcuts appear in the standalone editor so users do not have to relearn controls. There is no separate copy-versus-edit decision before capture.

## Rendering and edge cases

Line marks store start and end points, render with rounded caps, and scale with the source image during export. Drags shorter than three points are ignored to prevent accidental dots; arrows use the same threshold. Existing Retina coordinate conversion remains unchanged, so previews and exported marks share the same geometry.

## Acceptance criteria

- Rectangle, Line, and Arrow are visible without opening a secondary menu.
- `R`, `L`, and `A` switch tools in inline annotation and the standalone editor.
- Line previews during drag and is preserved in copied or saved output.
- Undo, redo, and clear include line marks.
- The app exposes only one Capture action.
- Return or Done copies a capture with or without annotations.
- Compact and dark library layouts remain unclipped.
