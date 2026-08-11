# ADR 0004: Kawaii-professional visual system

Date: 2026-08-11

## Status

Accepted

## Context

Kiri's library, capture overlay, OCR result, recording options, recording controls, and destructive confirmations had grown independently. The result worked, but looked like several unrelated macOS utilities. The previous monochrome app icon also did not express the requested friendly personality.

## Decision

Kiri uses one restrained kawaii-professional system across its native UI:

- a clean white canvas with white elevated surfaces in light mode and plum-charcoal surfaces in dark mode;
- lavender as the primary action color, sky blue for freshness, and peach pink only for warm emphasis or destructive states;
- rounded geometry, fine borders, and soft shadows while retaining native macOS materials and controls;
- a colorful chibi-girl app icon with violet-blue hair and a capture-frame motif;
- the same chibi-girl artwork as the in-app brand mark instead of a generic capture glyph;
- compact dark materials for capture and OCR overlays so they remain legible over arbitrary screen content;
- a light OCR result panel with dark editable text to guarantee contrast;
- custom in-app confirmation sheets for permanent deletion instead of visually unrelated system action sheets.

Cute details remain concentrated in the app icon, brand mark, gradients, and empty state. Dense working surfaces prioritize legibility and do not use decorative character art.

## Consequences

- Shared spacing, radius, palette, gradient, surface, and primary-button definitions live in `KiriDesignSystem.swift` and `CaptureUIStyle.swift`.
- New UI should reuse these definitions rather than introduce isolated purple, blue, or gray values.
- Light, dark, compact, empty, loading, error, search, and Trash states remain part of visual regression coverage.
- Destructive actions are intentionally not bound to Return to reduce accidental confirmation.
