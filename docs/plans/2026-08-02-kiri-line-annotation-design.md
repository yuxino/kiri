# Kiri Line Annotation Design

## Goal

Make common screenshot markup discoverable without slowing down Kiri's default capture flow. Users should be able to draw a box, a plain connecting line, or an arrow immediately after choosing **Capture & Edit**.

## Interaction

Kiri keeps two capture paths. **Capture & Copy** remains the fastest path: selecting a region copies it immediately. **Capture & Edit** opens the inline annotation canvas after region selection. The edit toolbar exposes Pen, Rectangle, Line, Arrow, Text, and Mosaic as first-level tools. Rectangle uses `R`, Line uses `L`, and Arrow uses `A`. Every drag-based tool previews while dragging, commits on mouse-up, and participates in the existing undo/redo history.

The same tool ordering and shortcuts appear in the standalone editor so users do not have to relearn controls. The library and menu-bar entry use the explicit label **Capture & Edit** instead of the less actionable **Annotate**.

## Rendering and edge cases

Line marks store start and end points, render with rounded caps, and scale with the source image during export. Drags shorter than three points are ignored to prevent accidental dots; arrows use the same threshold. Existing Retina coordinate conversion remains unchanged, so previews and exported marks share the same geometry.

## Acceptance criteria

- Rectangle, Line, and Arrow are visible without opening a secondary menu.
- `R`, `L`, and `A` switch tools in inline annotation and the standalone editor.
- Line previews during drag and is preserved in copied or saved output.
- Undo, redo, and clear include line marks.
- Fast Capture & Copy remains one-step.
- Compact and dark library layouts remain unclipped.
