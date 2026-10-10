# Kiri documentation

This directory contains only documentation that describes the current Tauri
application or a durable product decision.

## User guides

- [Setup and help](usage.md) · [使用与常见问题](usage.zh-CN.md) — installation, updates, permissions, and capture controls.
- [Video editing](video-editing.md) · [视频剪辑](video-editing.zh-CN.md) — import, trim, annotate, save, and export a local video.
- [Linux basic video export acceptance](qa/linux-video-export.md) — exact-package checklist for #74.
- [Linux](linux.md) — Ubuntu/GNOME setup, X11 compatibility,
  manual updates, feature limits, and separate CI/desktop acceptance.

## Read in your language

| Language | Overview | Setup and help | Video editing |
| --- | --- | --- | --- |
| 简体中文 | [Kiri](../README.md) | [简体中文](usage.zh-CN.md) | [简体中文](video-editing.zh-CN.md) |
| English | [Kiri](../README_EN.md) | [English](usage.md) | [English](video-editing.md) |
| 繁體中文 | [Kiri](../README_ZH_TW.md) | [繁體中文](usage.zh-TW.md) | [繁體中文](video-editing.zh-TW.md) |
| 日本語 | [Kiri](../README_JA.md) | [日本語](usage.ja.md) | [日本語](video-editing.ja.md) |
| Deutsch | [Kiri](../README_DE.md) | [Deutsch](usage.de.md) | [Deutsch](video-editing.de.md) |
| 한국어 | [Kiri](../README_KO.md) | [한국어](usage.ko.md) | [한국어](video-editing.ko.md) |
| Français | [Kiri](../README_FR.md) | [Français](usage.fr.md) | [Français](video-editing.fr.md) |

The user guides cover the same platform boundaries. Architecture, ADRs, detailed
Linux QA, and contribution records retain their original language.

## Current sources of truth

- [`qa/issue-21-windows/README.md`](qa/issue-21-windows/README.md) — native
  Windows multi-display screenshot acceptance and remaining hardware limits.
- [`qa/issue-82-retest.md`](qa/issue-82-retest.md) — verified repair scope and
  the numbered editor, OCR, QR and Linux desktop retest checklist.
- [`architecture.md`](architecture.md) — runtime structure, data boundaries,
  and platform responsibilities.
- [`../AGENTS.md`](../AGENTS.md) — product contract and repository rules.
- [`../ROADMAP.md`](../ROADMAP.md) — completed capabilities and work that still
  needs product or platform validation.
- [`../PRIVACY.md`](../PRIVACY.md) and [`../SECURITY.md`](../SECURITY.md) —
  network, credential, and local-data boundaries.
- [`windows-capture-incident.md`](windows-capture-incident.md) — current
  Windows screenshot lifecycle failure, diagnostics, and native retest gate.
- [`releases/v1.6.6.md`](releases/v1.6.6.md) — lighter recorded click highlight
  and Windows text scaling correction.
- [`releases/v1.4.12.md`](releases/v1.4.12.md) — preview recovery and consistent
  history updates after removing missing files.
- [`releases/v1.4.11.md`](releases/v1.4.11.md) — capture reliability, preview
  synchronization, and smaller application resources.
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
- [`adr/0059-lightweight-recorded-click-highlight.md`](adr/0059-lightweight-recorded-click-highlight.md)
  — reduce the recorded click ring's visual weight while retaining its red cue.
- [`adr/0063-qr-center-markers-and-supported-geometry.md`](adr/0063-qr-center-markers-and-supported-geometry.md)
  — explicit center-marker selection and validation of QR finder geometry.
- [`adr/0064-direct-qr-selection-and-opening.md`](adr/0064-direct-qr-selection-and-opening.md)
  — original-region QR markers, automatic saving of selected codes and direct opening.
- [`adr/0065-recording-save-status-and-file-copy.md`](adr/0065-recording-save-status-and-file-copy.md)
  — visible background recording saves and native file copying from the library and viewer.
- [`adr/0066-persistent-macos-dock-visibility.md`](adr/0066-persistent-macos-dock-visibility.md)
  — an immediate macOS Dock switch remembered across launches.
- [`adr/0067-bounded-local-qr-recovery.md`](adr/0067-bounded-local-qr-recovery.md)
  — bounded overlapping scans recover codes beyond full-image decoder limits.
- [`adr/0068-qr-markers-in-saved-image-editor.md`](adr/0068-qr-markers-in-saved-image-editor.md)
  — saved-image recognition uses the original editor surface and preserves drafts.

- [`adr/0069-image-fidelity-and-linux-media-playback.md`](adr/0069-image-fidelity-and-linux-media-playback.md)
  — transparent PNG editing, color conversion, shared styling and bounded Linux playback.

Completed implementation plans and the former Swift migration specifications
are intentionally not kept in the working tree. Git history and release tags
preserve them without letting obsolete paths or constraints guide current
development.

- [ADR 0027: Compact countdown](adr/0027-compact-countdown.md)

- [ADR 0028: Committed OCR selection](adr/0028-ocr-committed-selection.md)

- [ADR 0030: Text history and saved screenshot OCR](adr/0030-ocr-history.md)

- [ADR 0031: Windows update completion](adr/0031-windows-update-relaunch.md)

- [ADR 0032: Localized Kiri installer](adr/0032-localized-kiri-installer.md)
- [ADR 0033: Shared installer source](adr/0033-shared-installer-source.md)

- [0034 — Local video trimming and microphone checks](adr/0034-local-video-trimming-and-input-check.md)

- [0035 — Video timeline and timed effects](adr/0035-video-timeline-and-timed-effects.md)

- [0036 — Video annotation tracks and playback controls](adr/0036-video-annotation-tracks-and-playback.md)

- [0041 — Single-canvas video editing](adr/0041-single-canvas-video-editing.md)
- [0042 — Readable tools and explicit layer order](adr/0042-readable-tools-and-layer-order.md)
- [0043 — Editable video annotation objects](adr/0043-editable-video-annotation-objects.md)
- [0044 — Completing an edit without losing the user's place](adr/0044-video-editing-completion-and-selection.md)
- [0045 — Local video projects and cancellable export](adr/0045-local-video-projects-and-cancellable-export.md)

- [0047 — Native panel parents for full-screen capture windows](adr/0047-macos-fullscreen-space-parents.md)
- [ADR 0049: Linux screenshot MVP and staged recording](adr/0049-linux-screenshot-mvp.md) — historical initial scope, superseded by ADR 0051.
- [ADR 0050: Linux Wayland grim stills and Hyprland shortcut](adr/0050-linux-wayland-grim-and-hyprland-shortcut.md) — historical Hyprland implementation, superseded where noted.
- [ADR 0051: Ubuntu Linux capture boundaries](adr/0051-ubuntu-linux-capture-boundaries.md) — current platform, shortcut, recording, and packaging contract.
- [ADR 0052: GNOME screenshot consent recovery](adr/0052-gnome-screenshot-consent-recovery.md) — ambiguous first-use denial and safe retry guidance.
- [ADR 0053: Direct adjustment of selected lines](adr/0053-direct-line-adjustment.md)
- [ADR 0054: High-contrast recorded click highlight](adr/0054-red-click-highlight.md)
- [ADR 0055: Editable OCR history with original text](adr/0055-editable-ocr-history.md)
- [ADR 0056: Copy selected library items to a folder](adr/0056-batch-library-export.md)
- [ADR 0057: Explicit clipboard image import](adr/0057-clipboard-image-import.md)
- [ADR 0058: Pinned screenshot reference windows](adr/0058-pinned-screenshot-reference.md)
- [ADR 0060: Image text editing and unsaved close](adr/0060-image-text-editing-and-close.md)
- [ADR 0061: Local QR recognition and favorites](adr/0061-local-qr-recognition-and-favorites.md)
- [ADR 0062: Merge QR results across contrast thresholds](adr/0062-qr-multi-contrast-results.md)

- [ADR 0074: Linux basic video export](adr/0074-linux-basic-video-export.md) — normal-speed cuts, source audio, bounded native processing and independent capability gates.
- [ADR 0073: Linux recording audio](adr/0073-linux-recording-audio.md) — local sound sources, a shared media clock and real-device acceptance limits.
- [ADR 0075: Wayland GlobalShortcuts Portal](adr/0075-wayland-global-shortcuts-portal.md) — explicit setup, truthful bindings and command fallbacks.
- [ADR 0070: File size on library asset cards](adr/0070-library-asset-file-size.md)

- [ADR 0071: Library navigation and floating feedback](adr/0071-library-navigation-and-floating-feedback.md)
- [ADR 0072: Live annotation transform preview](adr/0072-live-annotation-transform-preview.md)
- [ADR 0076: Inline capture size controls](adr/0076-inline-capture-size-controls.md)
- [ADR 0077: Quick screenshot completion and pinning](adr/0077-quick-screenshot-completion-and-pin.md)
- [ADR 0078: Cancellable GIF conversion and video validation](adr/0078-cancellable-gif-video-validation.md)
- [ADR 0079: Seven languages and a shared preference](adr/0079-seven-language-preferences.md)
- [ADR 0080: Pinned macOS release signing](adr/0080-pinned-macos-release-signing.md)
- [ADR 0081: Direct capture toolbar pin](adr/0081-direct-capture-toolbar-pin.md)
- [ADR 0082: Screenshot hover color picker](adr/0082-screenshot-hover-color-picker.md)
- [ADR 0083: Editable numbered screenshot callouts](adr/0083-numbered-screenshot-callouts.md)
- [ADR 0084: Directional label bubbles](adr/0084-directional-label-bubbles.md)
- [ADR 0085: Edit numbered callout descriptions on the canvas](adr/0085-inline-callout-text-editing.md)
- [ADR 0086: Managed library file actions](adr/0086-library-file-actions.md)
- [ADR 0087: Keep capture controls clear of the mode selector](adr/0087-capture-hud-layout.md)
- [ADR 0088: Annotation properties and local text watermarks](adr/0088-annotation-properties-and-watermarks.md)
- [ADR 0089: Anchored labels and direct callout dragging](adr/0089-anchored-labels-and-direct-callout-drag.md)
- [ADR 0090: Reliable annotation re-editing and tiled watermarks](adr/0090-annotation-reediting-and-tiled-watermarks.md)
