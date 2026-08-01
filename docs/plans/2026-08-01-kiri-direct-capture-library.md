# Kiri Direct Capture and Library Onboarding Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make region capture complete on mouse release and replace the broken library empty screen with a compact, informative onboarding layout.

**Architecture:** Keep Kiri's existing selection/annotation phases but transition from selection to annotation at the end of every valid mouse gesture. Recompose the SwiftUI library into explicit loading, first-run, no-results, trash-empty, and content states while retaining the existing asset model.

**Tech Stack:** Swift 6, AppKit, SwiftUI, Swift Package Manager

---

### Task 1: Make selection single-gesture

**Files:**
- Modify: `Sources/KiriApp/SelectionOverlayController.swift`

**Steps:**

1. Remove double-click as the annotation transition.
2. After mouse-up normalizes a valid region or window candidate, call the existing annotation transition once.
3. Preserve invalid-click behavior and Escape-to-reselect behavior.
4. Compile the app with warnings as errors.

### Task 2: Add capture-stage guidance

**Files:**
- Modify: `Sources/KiriApp/SelectionOverlayController.swift`

**Steps:**

1. Replace the stale double-click hint with “Release to capture.”
2. Draw a compact initial instruction when no window or region is active.
3. Add a secondary hint row under the annotation actions describing Return and Escape.
4. Check toolbar sizing and edge clamping without opening a foreground window.

### Task 3: Model initial library loading

**Files:**
- Modify: `Sources/KiriApp/AppModel.swift`

**Steps:**

1. Add a published first-load completion flag.
2. Set it after the initial library read succeeds.
3. Build with warnings as errors.

### Task 4: Recompose the library screen

**Files:**
- Modify: `Sources/KiriApp/LibraryView.swift`
- Modify: `Sources/KiriApp/KiriApp.swift`

**Steps:**

1. Make the root view fill and top-align to the window.
2. Replace the duplicate brand header with title, count, search, navigation, and Capture actions.
3. Build dedicated loading, onboarding, search-empty, and trash-empty states.
4. Add a compact three-step capture hint to onboarding.
5. Compile with warnings as errors.

### Task 5: Verify and package

**Files:**
- Modify only if verification exposes a defect.

**Steps:**

1. Run `swift run kiri-core-tests`; expect every test to pass.
2. Run debug and release builds with `-warnings-as-errors`.
3. Render the library layout offscreen and inspect the result without ordering a window front.
4. Package with the stable signing identity and verify the designated requirement.
5. Do not reset permissions or leave a debug application bundle outside Trash.
