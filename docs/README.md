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

## Decision records

- [`adr/0003-manual-region-selection.md`](adr/0003-manual-region-selection.md)
  — quiet single-outline window selection.
- [`adr/0004-kawaii-professional-visual-system.md`](adr/0004-kawaii-professional-visual-system.md)
  — the former kawaii-professional system and app-icon history.
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
- [`adr/0012-interactive-completion-and-recording-output.md`](adr/0012-interactive-completion-and-recording-output.md)
  — interactive completion actions, explicit MP4/GIF output, and the neutral
  recording countdown.
- [`adr/0013-fail-closed-macos-release-signing.md`](adr/0013-fail-closed-macos-release-signing.md)
  — historical fail-closed signing rationale, superseded for release
  orchestration by ADR 0017.
- [`adr/0014-user-initiated-update-checks.md`](adr/0014-user-initiated-update-checks.md)
  — explicit, bounded update checks without background downloads or installs.
- [`adr/0015-monochrome-workspace-visual-system.md`](adr/0015-monochrome-workspace-visual-system.md)
  — the black, white, and neutral-gray application visual system.
- [`adr/0016-editable-screenshot-projects.md`](adr/0016-editable-screenshot-projects.md)
  — re-editable local screenshot annotations, flat-image compatibility, and
  completion/library editor entry points.
- [`adr/0017-maintainer-packaged-macos-releases.md`](adr/0017-maintainer-packaged-macos-releases.md)
  — developer-installable GitHub distribution with maintainer-packaged,
  stable self-signed macOS DMGs and no intentional CI red light.

Completed implementation plans and the former Swift migration specifications
are intentionally not kept in the working tree. Git history and release tags
preserve them without letting obsolete paths or constraints guide current
development.
