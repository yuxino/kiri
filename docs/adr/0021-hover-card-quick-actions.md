# ADR 0021: Hover card quick actions

- Status: Accepted
- Date: 2026-08-29
- Refines: ADR 0019

## Context

Direct card opening removed persistent per-card controls, but hiding the same
controls during pointer hover made quick actions difficult to discover. The
first action also used a View label and eye icon for images even though it
opens the screenshot editor.

## Decision

- Normal card clicks still open images in the editor and media in the viewer.
- Pointer hover reveals the card's quick actions without entering batch
  selection. Selected cards and cards with an open menu also retain them.
- The first image action is Edit with a pencil icon. Video and GIF keep View
  with an eye icon. Copy remains the second action.
- Rubber-band selection remains the only way to start batch selection.

## Consequences

Quick actions are discoverable without adding persistent card clutter, and
their labels match the surface they open.
