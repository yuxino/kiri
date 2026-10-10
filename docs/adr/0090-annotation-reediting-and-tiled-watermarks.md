# ADR 0090: Reliable annotation re-editing and tiled watermarks

Status: Accepted

## Context

Native Dev acceptance found clipped property controls, a watermark tool that
started an empty draft instead of reopening existing text, and text tools that
intercepted a drag as editing. Numbered descriptions could wrap differently after
saving and place their last line outside a previously saved short frame.

## Decision

- Keep the saved-image property area independent of selection and tool changes.
  Reserve complete control rows at each viewport width, with scrolling available
  for constrained windows. Tool changes must not shift the image coordinates.
- Existing ordinary text and label bubbles move and resize directly with Select,
  Text, or Label active. A double-click or the explicit edit action opens the
  native textarea. Clicking the label dot flips around its fixed point; dragging
  the dot does not accidentally flip it.
- Use shared description measurements for editing and saved callouts. New or
  changed content includes a small width margin for CSS/canvas rounding. Repair
  an old insufficient height when editing, keeping its width and placement
  unless the canvas edge requires moving it inward. Cancelling leaves the saved
  mark untouched; committing remains one undoable change.
- Supersede ADR 0088's single/tiled choice for new watermarks: offer tiled content
  only, with spacing always available. Repeated tool, canvas, and edit actions
  reuse the selected or last existing watermark. Reopening an active draft
  preserves its content, caret, native text history, and keyboard focus.
- Preserve support for old saved single watermarks. Opening, cancelling, or
  saving without changes retains them. An explicit content or style edit changes
  that watermark to tiled layout; unrelated document edits do not migrate it.

## Verification

Exercise all seven languages at narrow, medium, and wide editor widths, with
every property control visible and reachable. Cover draft reopening, saved
watermark editing without duplication, text/label first drags, fixed-dot clicks
and drag suppression, callout wrapping, edge placement, cancellation, and undo.
Verify save and reopen in the signed native app. Browser text injection does not
establish physical IME acceptance.
