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
- [`windows-capture-incident.md`](windows-capture-incident.md) — current
  Windows screenshot lifecycle failure, diagnostics, and native retest gate.
- [`releases/v1.4.9.md`](releases/v1.4.9.md) — first signed-updater release
  notes and one-time bootstrap instructions.

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
  — historical browser-only update checks, superseded by ADR 0025.
- [`adr/0015-monochrome-workspace-visual-system.md`](adr/0015-monochrome-workspace-visual-system.md)
  — the black, white, and neutral-gray application visual system.
- [`adr/0016-editable-screenshot-projects.md`](adr/0016-editable-screenshot-projects.md)
  — re-editable local screenshot annotations, flat-image compatibility, and
  completion/library editor entry points.
- [`adr/0017-maintainer-packaged-macos-releases.md`](adr/0017-maintainer-packaged-macos-releases.md)
  — developer-installable GitHub distribution with maintainer-packaged,
  stable self-signed macOS artifacts and no intentional CI red light.
- [`adr/0018-managed-library-location-and-recovery.md`](adr/0018-managed-library-location-and-recovery.md)
  — one managed library, whole-library migration, offline handling, and media
  recovery.
- [`adr/0019-direct-open-library-cards.md`](adr/0019-direct-open-library-cards.md)
  — direct card opening, rubber-band-only batch selection, and explicit editor
  completion wording.
- [`adr/0020-destructive-editor-cropping.md`](adr/0020-destructive-editor-cropping.md)
  — pending crop geometry, destructive library saves, and export-only Save As.
- [`adr/0021-hover-card-quick-actions.md`](adr/0021-hover-card-quick-actions.md)
  — hover-revealed card actions with Edit for images and View for media.
- [`adr/0022-persistent-annotation-appearance.md`](adr/0022-persistent-annotation-appearance.md)
  — shared last-used annotation styling without persisting the active tool.
- [`adr/0023-universal-macos-release.md`](adr/0023-universal-macos-release.md)
  — one verified Universal DMG for Apple silicon and Intel.
- [`adr/0024-native-media-pipelines.md`](adr/0024-native-media-pipelines.md)
  — AVFoundation/ImageIO on macOS and Media Foundation on Windows without a
  downloaded media executable.
- [`adr/0025-signed-user-initiated-updates.md`](adr/0025-signed-user-initiated-updates.md)
  — separately confirmed signed checks, downloads, installation, and
  platform-accurate restart behavior.

Completed implementation plans and the former Swift migration specifications
are intentionally not kept in the working tree. Git history and release tags
preserve them without letting obsolete paths or constraints guide current
development.
