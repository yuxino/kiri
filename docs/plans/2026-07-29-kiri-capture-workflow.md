# Kiri Capture Workflow Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Deliver a capture-first workflow with window snapping, inline annotation, clipboard-first completion, draggable Quick Access, pinning, and automatic history.

**Architecture:** A single AppKit capture session owns selection and annotation over one frozen display. Shared annotation rendering feeds every completion action; the library persists the capture before a lightweight Quick Access panel appears.

**Tech Stack:** Swift 6, AppKit, SwiftUI, ScreenCaptureKit, CoreGraphics, Carbon, Swift Package Manager

---

### Task 1: Testable window snapping geometry

**Files:**
- Modify: `Sources/KiriCore/SelectionGeometry.swift`
- Modify: `Tests/KiriCoreTests/SelectionGeometryTests.swift`
- Modify: `Tests/KiriCoreTests/main.swift`

**Steps:**

1. Add failing tests for topmost-window hit testing, display clipping, minimum
   size filtering, and coordinate normalization.
2. Run `swift run kiri-core-tests` and confirm the new tests fail.
3. Add a `WindowSnapGeometry` helper that returns the first eligible window
   containing a point and clips candidates to the active display.
4. Run `swift run kiri-core-tests` and confirm all tests pass.

### Task 2: Capture visible-window metadata

**Files:**
- Modify: `Sources/KiriApp/CaptureCoordinator.swift`

**Steps:**

1. Extend `CapturedDisplay` with top-left local window rectangles.
2. Read visible, on-screen, non-desktop windows from `SCShareableContent`.
3. Convert each window frame into the active overlay's coordinate space and
   discard empty or tiny rectangles.
4. Run `swift build --product kiri -Xswiftc -warnings-as-errors`.

### Task 3: Share the annotation canvas

**Files:**
- Create: `Sources/KiriApp/AnnotationCanvasView.swift`
- Modify: `Sources/KiriApp/EditorWindowController.swift`

**Steps:**

1. Move `AnnotationTool`, `AnnotationMark`, and `AnnotationCanvasView` into a
   reusable AppKit source file without changing rendering behavior.
2. Keep `EditorWindowController` as an explicit full-editor destination.
3. Add tool selection callbacks needed by a compact inline toolbar.
4. Run strict build and the core tests.

### Task 4: Replace selection overlay with a capture session

**Files:**
- Replace: `Sources/KiriApp/SelectionOverlayController.swift`
- Modify: `Sources/KiriApp/AppModel.swift`

**Steps:**

1. Rename the controller conceptually to `CaptureSessionController` while
   keeping the source path stable for this change.
2. Add selection and annotation states to the full-screen view.
3. Highlight the topmost window on hover; accept it on click; preserve manual
   drag, move, and resize.
4. Transition the same window to inline annotation and place the shared canvas
   over the selected region.
5. Add Pen, Rectangle, Arrow, Text, Mosaic, Undo, Copy, Save, Pin, and Edit
   actions.
6. Make Return complete with Copy and make Escape step back before canceling.
7. Run strict build after each state transition is wired.

### Task 5: Persist once and fan out completion actions

**Files:**
- Modify: `Sources/KiriApp/AppModel.swift`
- Modify: `Sources/KiriCore/AssetLibrary.swift`
- Modify: `Tests/KiriCoreTests/AssetLibraryTests.swift`
- Modify: `Tests/KiriCoreTests/main.swift`

**Steps:**

1. Add a test proving import returns an asset whose stable URL can be resolved.
2. Make the capture completion pipeline encode and persist exactly once.
3. Copy to the clipboard by default; treat save and clipboard failures as
   recoverable without losing history.
4. Refresh the library after completion and pass the stored asset to Quick
   Access.
5. Run all core tests.

### Task 6: Add draggable Quick Access and pinning

**Files:**
- Create: `Sources/KiriApp/QuickAccessController.swift`
- Create: `Sources/KiriApp/PinnedImageController.swift`
- Modify: `Sources/KiriApp/AppModel.swift`

**Steps:**

1. Create a borderless bottom-right Quick Access panel with image preview and
   Copy, Save, Pin, Edit, and Close actions.
2. Implement an image drag source that advertises copy-only behavior.
3. Add hover-aware auto-dismiss.
4. Create a borderless, resizable, always-on-top pinned image panel.
5. Ensure repeated captures replace Quick Access without closing pinned images.
6. Run strict build.

### Task 7: Product copy, packaging, and interaction QA

**Files:**
- Modify: `README.md`
- Modify: `README_EN.md`
- Modify: `README_JA.md`

**Steps:**

1. Update documentation around the new capture-first workflow.
2. Run `swift run kiri-core-tests`.
3. Run `swift build --product kiri -Xswiftc -warnings-as-errors`.
4. Run `./scripts/package-app.sh` and verify the signature with
   `codesign --verify --deep --strict dist/kiri.app`.
5. Launch the packaged app and manually verify window snap, region selection,
   annotation, Return, Escape, Quick Access drag, Copy, Save, Pin, Edit, and
   history.
6. Review the complete diff and commit the implementation.
