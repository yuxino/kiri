# Kiri UI/UX and Long Screenshot Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement the implementation plan task-by-task.

**Goal:** Refine the Kiri library and capture experience, and add a guided local long-screenshot workflow that stitches multiple viewport captures into one recoverable library asset.

**Architecture:** Keep the existing AppKit/SwiftUI split. The library UI remains in `LibraryView`; long-image stitching lives in `KiriCore` as deterministic image-only logic; an AppKit controller owns the guided capture panel and `AppModel` remains the only coordinator that writes to the clipboard and `AssetLibrary`.

**Tech Stack:** Swift 6, macOS 14, SwiftUI, AppKit, ScreenCaptureKit, CoreGraphics, ImageIO, Swift Package Manager.

---

### Task 1: Establish visual and behavior baseline

**Files:**
- Read: `AGENTS.md`, `README.md`, `ROADMAP.md`, `docs/plans/2026-08-04-kiri-v0-2-codex-handoff.md`
- QA: `scripts/render-ui-snapshots.sh`

Run `swift run kiri-core-tests`, `swift build --product kiri -Xswiftc -warnings-as-errors`, and the offscreen snapshot script. Inspect populated, compact, empty, and dark library fixtures. Preserve the current clean worktree and existing privacy/capture behavior.

### Task 2: Improve library UI/UX

**Files:**
- Modify: `Sources/KiriApp/LibraryView.swift`
- Modify: `Sources/KiriApp/KiriDesignSystem.swift`
- Test fixture only if needed: `scripts/qa/LibrarySnapshotMain.swift`

Make long-image cards visually legible, clarify header priority at compact widths, and improve card metadata and actions without changing `AppModel` behavior or adding dependencies. Keep all interaction states accessible and localized through existing keys.

### Task 3: Add deterministic long-image stitching

**Files:**
- Create: `Sources/KiriCore/LongScreenshotStitcher.swift`
- Create: `Tests/KiriCoreTests/LongScreenshotStitcherTests.swift`
- Modify: `Tests/KiriCoreTests/main.swift`

Implement bounded vertical overlap detection and stitching with explicit input/output limits. Test empty input, no-overlap composition, detected overlap, different widths, and output-height rejection.

### Task 4: Integrate guided long screenshot capture

**Files:**
- Modify: `Sources/KiriApp/SelectionOverlayController.swift`
- Modify: `Sources/KiriApp/AppModel.swift`
- Create: `Sources/KiriApp/LongScreenshotCaptureController.swift`
- Modify: localized strings, `README.md`, `README_EN.md`, `ROADMAP.md`

Add a first-level Long Screenshot mode. After the initial region is selected, capture the first section, present a compact movable guide, hide it for each fresh `CaptureCoordinator` capture, allow next/undo/finish/cancel, stitch locally, and persist/copy the resulting `.longImage` PNG through existing `AppModel` pathways.

### Task 5: Verify and hand off

Run the focused core tests, full core tests, warnings-as-errors build, UI snapshots, `git diff --check`, and the packaging/install flow required by `AGENTS.md` for capture and AppKit overlay changes. Review the final diff for localization parity and confirm no QA media was added to the user's library. Do not commit or publish unless explicitly requested.
