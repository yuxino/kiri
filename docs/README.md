# Kiri documentation

This directory contains only documentation that describes the current Tauri
application or a durable product decision.

## Current sources of truth

- [`architecture.md`](architecture.md) — runtime structure, data boundaries,
  and platform responsibilities.
- [`../AGENTS.md`](../AGENTS.md) — product contract and repository rules.
- [`../ROADMAP.md`](../ROADMAP.md) — completed capabilities and work that still
  needs product or platform validation.
- [`../PRIVACY.md`](../PRIVACY.md) and [`../SECURITY.md`](../SECURITY.md) —
  network, credential, and local-data boundaries.

## Accepted decisions

- [`adr/0003-manual-region-selection.md`](adr/0003-manual-region-selection.md)
  — quiet single-outline window selection.
- [`adr/0004-kawaii-professional-visual-system.md`](adr/0004-kawaii-professional-visual-system.md)
  — the shared visual language.
- [`adr/0007-opt-in-remote-ocr-profiles.md`](adr/0007-opt-in-remote-ocr-profiles.md)
  — opt-in remote OCR and credential handling.
- [`adr/0008-stable-development-signing-identity.md`](adr/0008-stable-development-signing-identity.md)
  — stable macOS privacy identity during development.
- [`adr/0009-operation-local-feedback.md`](adr/0009-operation-local-feedback.md)
  — operation-local progress and completion feedback.
- [`adr/0010-single-action-screenshot-completion.md`](adr/0010-single-action-screenshot-completion.md)
  — one clipboard-first completion action without an overflow menu.
- [`adr/0011-movable-capture-mode-selector.md`](adr/0011-movable-capture-mode-selector.md)
  — a top-centered mode selector that can be moved for the current capture.

Completed implementation plans and the former Swift migration specifications
are intentionally not kept in the working tree. Git history and release tags
preserve them without letting obsolete paths or constraints guide current
development.
