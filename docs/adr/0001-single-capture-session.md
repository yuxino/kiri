# ADR-0001: Use a Single Capture Session

## Status

Superseded by [Kiri Capture Flow Reset](../plans/2026-08-01-kiri-capture-flow-reset-design.md).

## Context

Kiri currently freezes one display for selection, closes that overlay, and
opens a separate editor window. This interrupts the user's context and makes
basic capture slower than macOS. The approved workflow requires window
snapping, inline annotation, clipboard-first completion, Quick Access, and
background history.

The interaction must remain native, local-only, responsive on Retina displays,
and maintainable in the existing Swift/AppKit application.

## Decision

Use one borderless AppKit overlay window for both selection and inline
annotation. Extract the annotation canvas so the overlay and full editor share
the same renderer. Persist a completed capture before presenting a separate
lightweight Quick Access panel.

Keep history as a background service rather than a required destination.

## Consequences

### Positive

- Selection and annotation no longer require an application context switch.
- The same rendered result powers clipboard, history, save, pin, and edit.
- Existing annotation behavior stays consistent between quick and full modes.
- Quick Access receives a stable library URL for safe drag-and-drop.

### Negative

- The capture overlay becomes a small state machine.
- AppKit window and responder behavior needs explicit interaction testing.
- Window snapping depends on the metadata macOS exposes for visible windows.

### Neutral

- The existing library and asset format remain unchanged.
- Advanced capture modes can be added as new session inputs later.

## Alternatives Considered

**Invoke the macOS screenshot command and watch for its output**

Rejected because Kiri would lose control of inline annotation, completion
actions, and reliable handoff into history.

**Keep the separate editor window**

Rejected because it preserves the interruption users already identified.

**Build selection, editor, and Quick Access as independent workflows**

Rejected because duplicated image state would create inconsistent output and
more failure modes.
