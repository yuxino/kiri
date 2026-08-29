# ADR 0019: Direct-open library cards

- Status: Accepted
- Date: 2026-08-29
- Partially supersedes: The library double-click entry in ADR 0016

## Context

A single click previously selected one library card and exposed both a checked
state and the floating batch-action bar. Opening an asset required a double
click. This made an ordinary open gesture look like the start of a destructive
batch workflow, while per-card action icons remained visually prominent.

The editor's primary action was labelled Copy even though editing is a change
to the managed library asset. That label hid the normal edit-save workflow.

## Decision

- A normal card click opens an image editor or media viewer immediately.
- Checked cards, per-card action icons, and the batch-action bar appear only
  after rubber-band selection. Escape or Cancel clears that selection.
- While batch selection is active, card clicks do not open assets
  accidentally. Selected cards retain explicit per-card actions.
- Context-click remains available without entering batch selection.
- The editor's primary action saves the current library asset and is labelled
  Save in every language. Save As remains a separate export action.
- Closing without saving is labelled Cancel. It is not hidden behind an
  overflow icon because no editor overflow menu exists.

## Consequences

Opening a capture is direct and does not flash selection controls. Batch
operations require a visible drag gesture, and the editor presents the normal
edit-save workflow directly.
