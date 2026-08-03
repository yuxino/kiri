# Annotation Colors, Text, and Keyboard Design

## Interaction

- Keep one unified capture flow: select a region, annotate in place, then copy.
- Show eight compact one-click color swatches beside the annotation tools.
- Store the selected color on each mark so later color changes do not recolor
  existing annotations.
- Make Text enter placement mode. Clicking the image creates an inline text
  field at that position instead of opening a separate modal alert.
- While typing, Return commits the text and completes the capture. Escape
  cancels the entire capture. Outside text editing, Return completes and Escape
  cancels from the capture overlay as well.

## Visual Treatment

- Use small circular swatches inside the existing translucent grouped toolbar.
- Give the selected swatch a tinted surface and a crisp color-matched ring.
- Preserve the lavender Kiri accent as the default while offering cherry,
  orange, yellow, mint, blue, white, and black.
- Render text in the chosen color over a contrast-aware backing so light and
  dark colors stay legible on screenshots.

## Verification

- Build with warnings treated as errors and run the full core test suite.
- Launch the packaged app and inspect toolbar sizing and selected states.
- Exercise Text placement, Return completion, and Escape cancellation in the
  real full-screen capture overlay.
