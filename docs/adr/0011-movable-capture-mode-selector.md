# ADR 0011: Movable capture mode selector

- Status: Accepted
- Date: 2026-08-25

## Context

The Screenshot, Record, and OCR selector is always visible at the top center of
the frozen display. This makes the three capture modes easy to find, but the
control can cover content near the top edge while a user is selecting or
annotating that area.

Adding a permanent drag handle would make a compact, high-frequency control
visually heavier. Letting every pointer movement drag the selector would make
normal mode clicks unreliable.

## Decision

- Keep the mode selector top-centered when a capture session starts.
- Allow the entire selector surface, including its buttons, to be dragged.
- Treat movement below a small threshold as a normal button click. Once the
  threshold is crossed, move the selector and suppress that gesture's click.
- Constrain the selector to an eight-point inset from the current display.
- Keep the moved position only for the active capture session. A new capture
  starts at the predictable top-center default.
- Use only a grab cursor and slightly stronger shadow while dragging; do not add
  a handle, tooltip, or persistent position setting.

## Consequences

Users can uncover top-edge content without losing the stable default location
or accidentally changing capture mode. Pointer capture keeps dragging reliable
outside the selector, and resizing the overlay clamps a moved selector back
inside the visible display.
