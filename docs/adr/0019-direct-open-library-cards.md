# ADR 0019: Direct-open library cards

- Status: Accepted
- Date: 2026-08-29
- Partially supersedes: The library double-click entry in ADR 0016

## Context

A single click previously selected one library card and exposed both a checked
state and the floating batch-action bar. Opening an asset required a double
click. This made an ordinary open gesture look like the start of a destructive
batch workflow, while per-card action icons remained visually prominent.

The editor's primary action was labelled Copy even though it first updates the
managed library asset and then copies the flattened image to the clipboard.

## Decision

- A normal card click opens an image editor or media viewer immediately.
- Checked cards, per-card action icons, and the batch-action bar appear only
  after rubber-band selection. Escape or Cancel clears that selection.
- While batch selection is active, card clicks do not open assets
  accidentally. Selected cards retain explicit per-card actions.
- Context-click remains available without entering batch selection.
- The editor's primary action is labelled Save & Copy in every language.

## Consequences

Opening a capture is direct and does not flash selection controls. Batch
operations require a visible drag gesture, and the editor states both effects
of its primary action before the user commits it.
