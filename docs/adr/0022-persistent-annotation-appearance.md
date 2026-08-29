# ADR 0022: Persistent annotation appearance

- Status: Accepted
- Date: 2026-08-29

## Context

Every capture overlay and editor previously started with the same hard-coded
annotation color and sizes. Repeating a preferred visual style required
reselecting it in each independent Tauri window.

## Decision

- Kiri remembers the last-used color, pen and shape widths, text background
  and font size, and mosaic style, strength, and brush diameter.
- The capture overlay and saved-image editor share the same preference.
- Rust validates and stores the preference in the native app config directory;
  slider changes are debounced and the latest value is flushed when a window
  closes.
- Kiri does not remember the active tool, selection, crop, or annotation
  content. New surfaces still begin in Select.
- Missing, partial, invalid, or out-of-range preferences fall back to safe
  defaults or are clamped to the existing toolbar ranges.

## Consequences

Repeated captures keep the user's visual style without making the next tool or
editing operation surprising. The preference stays local and contains no
capture content.
