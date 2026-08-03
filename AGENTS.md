# Kiri repository guide for Codex

This file is the first source of truth for agents working in this repository.
Read it before editing, then read the current handoff plan linked below.

## Start here

1. Run `git status -sb` before making changes.
2. Read `README.md`, `ROADMAP.md`, and
   `docs/plans/2026-08-04-kiri-v0-2-codex-handoff.md`.
3. For capture-selection behavior, also read
   `docs/adr/0003-manual-region-selection.md`.
4. Treat every pre-existing modification and untracked file as user-owned.
   Never reset, discard, overwrite, or reformat unrelated work.
5. Historical plans under `docs/plans/` describe how earlier versions were
   built. When they conflict with current source, this file, a newer ADR, or
   the current handoff, the newer source of truth wins.

## Product contract

Kiri is a native, local-first macOS capture utility. Preserve these decisions:

- Minimum platform is macOS 14; the project uses Swift 6 and Swift Package
  Manager with Apple frameworks rather than third-party dependencies.
- The global capture shortcut is exclusively `Shift-Command-A` (`⇧⌘A`).
- The initial overlay offers Screenshot and Record at the first level.
- Window selection is silent until click. Do not restore hover-following window
  outlines. A click selects the frontmost window; a drag creates a custom
  region. Both selections remain movable and resizable with eight handles.
- Screenshot completion is clipboard-first and returns focus to the original
  application. Do not open the Kiri library after every capture.
- Escape cancels capture and countdown; Return confirms a screenshot.
- Annotation tools appear immediately after region selection. Existing text
  and shapes remain selectable and editable; size controls update live.
- Text backgrounds default to transparent. Mosaic is a continuous brush with
  adjustable diameter and intensity.
- Recording is Retina-scale, high-quality MP4. Kiri's recording controls and
  paused time must not appear in the exported video.
- The optional violet click ripple is visible live and is also captured.
- The 3-2-1 countdown is centered and compact; it must not dim the selected
  recording region.
- User-facing UI currently supports English and Simplified Chinese and should
  follow the macOS preferred language.
- Captures stay local. Never add uploads, analytics, accounts, or network
  behavior without an explicit product decision and privacy documentation.

## Repository map

- `Sources/KiriApp/` — AppKit/SwiftUI application, capture overlay, annotation
  UI, library, recording controllers, localization, and app orchestration.
- `Sources/KiriCore/` — testable models, storage, geometry, shortcut, permission,
  and recording policies. Keep platform-independent logic here when practical.
- `Tests/KiriCoreTests/` — lightweight executable test suite. Register new tests
  in `main.swift`.
- `Sources/KiriApp/Resources/*.lproj/` — localized app and Info.plist strings.
- `docs/adr/` — accepted architecture/product decisions.
- `docs/plans/` — dated design and implementation history; older files may be
  superseded.
- `scripts/package-app.sh` — release build and stable code signing.
- `scripts/install-app.sh` — package and install `/Applications/Kiri.app`.
- `scripts/render-ui-snapshots.sh` — offscreen library-state visual regression
  fixtures; it must not read the user's capture library.

## Architecture boundaries

- `AppModel` coordinates capture, library operations, recording state, and
  transient feedback. Keep framework-specific services in focused controllers.
- `CaptureCoordinator` obtains permission, freezes the active display, and
  collects visible-window geometry only for click selection, never hover UI.
- `SelectionOverlayController` owns screenshot/record mode selection, manual
  region interactions, and the inline annotation toolbar.
- `AnnotationCanvasView` owns annotation history, drawing, selection, resizing,
  and inline text editing.
- `RegionRecorder` owns ScreenCaptureKit capture and media writing.
- `RecordingCountdownController`, `RecordingControlPanelController`, and
  `RecordingClickHighlighterController` are separate overlay concerns.
- `AssetLibrary` is the persistence boundary. Preserve recoverable Trash and
  never manipulate a user's library directly during QA.

## Required verification

Run the smallest relevant check while editing, then all of the following before
handoff:

```bash
swift run kiri-core-tests
swift build --product kiri -Xswiftc -warnings-as-errors
snapshot_dir=$(mktemp -d)
./scripts/render-ui-snapshots.sh "$snapshot_dir"
git diff --check
```

For changes to capture, recording, permissions, focus, keyboard handling, or
AppKit overlays, also package and test the fixed-path app:

```bash
./scripts/install-app.sh
open /Applications/Kiri.app
```

Use a stable signing identity. Do not silently use ad-hoc signing because it
changes the privacy identity and can invalidate Screen Recording/Input
Monitoring permissions.

## UI acceptance checklist

- Verify both Screenshot and Record modes from the initial overlay.
- Verify click-to-select-window without hover outlines, plus manual region drag,
  move, and all eight resize handles.
- Verify Escape and Return behavior and original-app focus restoration.
- Verify the toolbar at narrow regions and near every display edge.
- Verify text creation, IME input, second edit, live font sizing, and background
  styles.
- Verify mosaic brush diameter/intensity and editing of existing annotations.
- For recording, inspect extracted frames around start, click ripple,
  pause/resume, and stop. Confirm clarity and absence of all Kiri controls.
- Avoid leaving QA captures in the user's library. Move only agent-created test
  assets to Kiri Trash, which is recoverable; never empty Trash without consent.

## Localization and documentation

- Wrap user-facing AppKit/SwiftUI strings with `L10n.text`/`L10n.format`.
- Keep English and Simplified Chinese key sets identical.
- Validate `.strings` files through the packaging script or `plutil -lint`.
- Update `README.md` and `README_EN.md` together for user-visible behavior.
  `README_JA.md` is currently not synchronized with v0.2; do not claim it is.
- Record durable interaction changes as a new ADR instead of rewriting old
  history without explanation.

## Git and release safety

- Current work may be intentionally dirty. Do not create a new branch, commit,
  merge, push, tag, or publish a release unless the user asks.
- When asked to commit, inspect the exact diff and keep unrelated user work out
  of the commit when possible.
- Do not delete capture data, reset privacy permissions, or replace signing
  identities as a troubleshooting shortcut.
- Never include private captures, credentials, personal absolute paths, or the
  contents of `~/Library/Application Support/kiri/` in commits.
