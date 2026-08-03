# Compact Single-Row Capture Toolbar

## Goal

Match the supplied reference with a quiet, single-row capture dock while keeping Kiri's existing annotation features immediately available.

## Layout

- Use one horizontal visual-effect surface with a 12-point continuous corner radius.
- Remove the decorative sparkle, persistent help line, second context row, and large titled Done button.
- Keep cancel, six annotation tools, colors, undo, redo, confirm, and more actions as icon-sized controls.
- Show the selected tool's settings inline between the tool group and color/history controls.

## Context controls

- Pen and shapes: line-weight icon, short continuous slider, compact numeric value.
- Text: font-size slider and three icon-only background choices.
- Mosaic: brush-size slider and three compact strength choices.
- Hide context controls until a tool is selected; hide the color palette for mosaic.
- Preserve tooltips and accessibility labels so the reduced visible copy does not reduce discoverability.

## Acceptance criteria

- The toolbar remains one row in selection, drawing, text, and mosaic states.
- A valid selection still exposes the toolbar immediately and remains resizable until a tool is chosen.
- All size, background, strength, color, undo/redo, Enter, and Escape behaviors continue to work.
- The production build passes with warnings treated as errors and the installed app is visually checked against the reference.
