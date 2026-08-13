# Kiri repository guide for agents

This file is the first source of truth for agents working in this repository.
Read it before editing, then read `docs/plans/2026-08-13-kiri-tauri-migration-design.md`.

## Start here

1. Run `git status -sb` before making changes.
2. Read `README.md`, `ROADMAP.md`, and the migration design doc.
3. For capture-selection behavior, also read
   `docs/adr/0003-manual-region-selection.md`.
4. Treat every pre-existing modification and untracked file as user-owned.
   Never reset, discard, overwrite, or reformat unrelated work.
5. `docs/spec/swift/` contains the authoritative 1:1 behavior specs extracted
   from the original Swift implementation. Historical plans under
   `docs/plans/` describe how earlier versions were built; when they conflict
   with current source, this file, a newer ADR, or the migration design wins.

## Product contract

Kiri is a local-first capture utility for macOS and Windows. Preserve these
decisions:

- The global capture shortcut is `⇧⌘A` on macOS (exclusive, consumed via
  CGEventTap) and `Shift+Ctrl+A` on Windows (RegisterHotKey).
- The initial overlay offers Screenshot, Record, and OCR.
- Window hover shows exactly one restrained violet outline without handles,
  dimensions, stacked borders, or a following tooltip. A click selects that
  window; a drag creates a custom region. Both selections remain movable and
  resizable with eight handles.
- Screenshot completion is clipboard-first and returns focus to the original
  application. Do not open the Kiri library after every capture.
- Escape cancels capture and countdown; Return confirms a screenshot.
- Annotation tools appear immediately after region selection. Existing text
  and shapes remain selectable and editable; size controls update live.
- Text backgrounds default to transparent. Mosaic is a continuous brush with
  adjustable diameter and intensity.
- Recording is Retina/DPI-scale, high-quality MP4. Kiri's recording controls
  and paused time must not appear in the exported video.
- The optional violet click ripple is visible live and is also captured.
- The 3-2-1 countdown is centered and compact; it must not dim the selected
  recording region.
- User-facing UI supports English and Simplified Chinese and follows the OS
  preferred language.
- Captures stay local. Never add uploads, analytics, accounts, or network
  behavior without an explicit product decision and privacy documentation.
  (The one exception: ffmpeg is downloaded once at build/first-run time to
  provide offline encoding.)

## Repository map

- `src/` — React frontend: capture overlay, annotation canvas, library,
  editor, countdown/control/ripple windows, i18n (en/zh-Hans), design tokens.
- `src-tauri/src/core/` — platform-independent models: geometry, recording
  policy, shortcut model, asset library (byte-compatible with the Swift
  version's `library.json`).
- `src-tauri/src/capture/` — per-platform capture backends (macOS:
  ScreenCaptureKit via objc2; Windows: xcap WGC + windows-capture + cpal).
- `src-tauri/src/platform/` — per-platform helpers: global shortcut, focus
  restoration, file reveal, click monitoring, capture exclusion.
- `src-tauri/src/record.rs` — ffmpeg encoding pipeline (H.264 + AAC → MP4,
  segment merge).
- `src-tauri/src/commands.rs` — the AppModel-equivalent command surface.
- `src-tauri/src/{ocr,gif,thumbnail,protocol,state}.rs` — OCR, GIF export,
  thumbnails, `kiri://` protocol, shared state.
- `scripts/` — packaging (package-app.sh, install-app.sh, ensure-ffmpeg.mjs).
- `docs/spec/swift/` — 1:1 behavior specs (the migration source of truth).
- `docs/adr/` — accepted architecture/product decisions.

## Architecture boundaries

- `AppState` (state.rs) coordinates capture, library operations, recording
  state, and transient feedback. Synchronous Tauri commands run on the main
  thread (mirroring the Swift @MainActor design); heavy work spawns
  background threads.
- `capture::macos` runs the SCK stream on a dedicated thread; control flows
  through channels. `SCShareableContent` is main-thread-only — resolve it on
  the main thread.
- The recording pipeline is: platform capture (BGRA frames + PCM audio) →
  pipe → ffmpeg (hardware H.264 preferred, AAC) → MP4. Pause/resume produces
  segments merged losslessly with the concat demuxer.
- `AssetLibrary` is the persistence boundary. It shares the Swift version's
  storage layout (`~/Library/Application Support/kiri` on macOS,
  `%APPDATA%\kiri` on Windows) so existing libraries keep working. Preserve
  recoverable Trash and never manipulate a user's library directly during QA.
- Frontend windows render by `?window=` query param; the frozen capture is
  served through the `kiri://` protocol from memory.

## Required verification

Run the smallest relevant check while editing, then all of the following
before handoff:

```bash
cargo test --manifest-path src-tauri/Cargo.toml
cargo check --manifest-path src-tauri/Cargo.toml
pnpm build
git diff --check
```

For changes to capture, recording, permissions, focus, keyboard handling, or
overlay windows, also package and test the fixed-path app:

```bash
./scripts/install-app.sh
open /Applications/Kiri.app
```

Use a stable signing identity (`KIRI_SIGNING_IDENTITY`). Do not silently use
ad-hoc signing because it changes the privacy identity and can invalidate
Screen Recording/Input Monitoring permissions. Windows builds are verified
through GitHub Actions (`.github/workflows/build.yml`).

## UI acceptance checklist

- Verify Screenshot, Record, and OCR from the initial overlay.
- Verify the single-outline window hover and click selection, plus manual
  region drag, move, and all eight resize handles.
- Verify Escape and Return behavior and original-app focus restoration.
- Verify the toolbar at narrow regions and near every display edge.
- Verify text creation, IME input, second edit, live font sizing, and
  background styles.
- Verify mosaic brush diameter/intensity and editing of existing annotations.
- For recording, inspect extracted frames around start, click ripple,
  pause/resume, and stop. Confirm clarity and absence of all Kiri controls.
- Avoid leaving QA captures in the user's library. Move only agent-created
  test assets to Kiri Trash, which is recoverable; never empty Trash without
  consent.

## Localization and documentation

- All user-facing strings go through `t()`/`fmt()` in `src/i18n`; the English
  string is the key (matching the Swift L10n behavior).
- Keep the English and zh-Hans dictionaries identical in key set.
- Update `README.md` and `README_ZH.md` together for user-visible behavior.
  `README_JA.md` is not synchronized; do not claim it is.
- Record durable interaction changes as a new ADR instead of rewriting old
  history without explanation.

## Git and release safety

- Current work may be intentionally dirty. Do not create a new branch, commit,
  merge, push, tag, or publish a release unless the user asks.
- When asked to commit, inspect the exact diff and keep unrelated user work
  out of the commit when possible.
- Do not delete capture data, reset privacy permissions, or replace signing
  identities as a troubleshooting shortcut.
- Never include private captures, credentials, personal absolute paths, or the
  contents of `~/Library/Application Support/kiri/` in commits.
