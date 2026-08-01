# Kiri Annotation Toolbar Redesign Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace the colorful flat action strip with a native-feeling grouped toolbar and complete undo/redo behavior.

**Architecture:** Add a generic history model to KiriCore, then make `AnnotationCanvasView` publish history availability. Rebuild the inline toolbar around monochrome tool buttons, stateful undo/redo controls, one Done action, and a More menu; reuse the history behavior in the full editor.

**Tech Stack:** Swift 6, AppKit, Swift Package Manager, SF Symbols

---

### Task 1: Add the annotation history model

**Files:**
- Create: `Sources/KiriCore/AnnotationHistory.swift`
- Create: `Tests/KiriCoreTests/AnnotationHistoryTests.swift`
- Modify: `Tests/KiriCoreTests/main.swift`

**Steps:**

1. Add tests for empty state, append, undo, redo, redo invalidation after a new append, and clear.
2. Run `swift run kiri-core-tests` and confirm the new suite fails to compile.
3. Implement `AnnotationHistory<Element>` with value semantics and computed availability.
4. Register the suite and rerun `swift run kiri-core-tests`; expect all tests to pass.

### Task 2: Connect history state to the annotation canvas

**Files:**
- Modify: `Sources/KiriApp/AnnotationCanvasView.swift`

**Steps:**

1. Replace the canvas mark array with `AnnotationHistory<AnnotationMark>`.
2. Publish `onHistoryChange(canUndo:canRedo:)` whenever a mark is appended, undone, redone, or cleared.
3. Add `redo()` and `clearAnnotations()` while preserving rendering order and text placement behavior.
4. Build the app with warnings treated as errors.

### Task 3: Rebuild the inline toolbar

**Files:**
- Modify: `Sources/KiriApp/CaptureUIStyle.swift`
- Modify: `Sources/KiriApp/SelectionOverlayController.swift`

**Steps:**

1. Replace per-action colors with neutral, selected, primary, hover, and disabled states.
2. Build grouped tool, history, and completion controls with 30-point icon buttons.
3. Add stateful Undo and Redo buttons plus a More menu containing Save As, Pin, Open in Editor, and Clear Annotations.
4. Add Shift-Command-Z redo and keep Return, Escape, Command-C, and Command-S behavior.
5. Compile with warnings treated as errors.

### Task 4: Bring history parity to the full editor

**Files:**
- Modify: `Sources/KiriApp/EditorWindowController.swift`

**Steps:**

1. Add Redo and Clear controls and synchronize enabled states from the canvas.
2. Add standard Undo and Redo keyboard equivalents.
3. Keep copy and save completion behavior unchanged.
4. Compile the app and confirm no warnings.

### Task 5: Verify and package safely

**Files:**
- Modify only if verification exposes a defect.

**Steps:**

1. Run `swift run kiri-core-tests`; expect every suite to pass.
2. Run debug and release builds with `-warnings-as-errors`.
3. Package to an isolated output and verify its signature.
4. Review `git diff --check`, the final diff, and repository status.
5. Do not launch the packaged app or create a foreground capture session.
