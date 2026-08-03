# Text Background and v0.1.0 Release Design

## Interaction

Text annotations offer three background styles: Transparent, Dark, and Light.
Transparent is the default. A compact toolbar button opens a native menu, marks
the current style, and switches to the Text tool when a style is selected. The
same control is available in both inline capture and the full editor.

Each text annotation stores its own foreground color, background style, and text
layout rectangle. Changing the current setting affects the active or next text
annotation without recoloring existing marks. Long text uses the same wrapping
rectangle during editing, preview, and export.

## Visual and Accessibility

Dark and Light backgrounds render as padded rounded rectangles. Transparent text
has no exported backing; the active editor keeps a color-matched focus border so
its boundary remains visible. The background button exposes its current state in
its tooltip and accessibility label, and the menu offers familiar moon, sun, and
transparent symbols.

## Release

Update the Chinese README with the current capture, keyboard, color, text, Dock,
and privacy behavior. Build and verify the signed macOS app, package a zip, tag
the first public preview as `v0.1.0`, publish GitHub release notes, and push the
feature branch and tag.
