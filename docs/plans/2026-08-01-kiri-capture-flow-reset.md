# Kiri Capture Flow Reset Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make quick capture a one-gesture copy, keep annotation as an explicit alternate mode, and remove the post-capture preview.

**Architecture:** Pass a capture intent from each app entry point into the existing overlay. The overlay either emits the cropped image immediately or transitions to its inline annotation phase, while AppModel copies before asynchronously persisting the capture.

**Tech Stack:** Swift 6, AppKit, SwiftUI, ScreenCaptureKit, Swift Package Manager

---

### Task 1: Add explicit capture intents

**Files:**
- Modify: `Sources/KiriApp/SelectionOverlayController.swift`
- Modify: `Sources/KiriApp/AppModel.swift`

**Steps:**
1. Add quick-copy and annotation capture intents.
2. Pass the selected intent from AppModel into the overlay.
3. Keep the global shortcut mapped to quick copy.
4. Build with warnings as errors.

### Task 2: Make selection intent-driven

**Files:**
- Modify: `Sources/KiriApp/SelectionOverlayController.swift`

**Steps:**
1. Make valid mouse-up emit a cropped quick-copy result in quick mode.
2. Keep the inline annotation transition only in annotation mode.
3. Remove transient resize handles and stale-selection manipulation from the one-gesture flow.
4. Replace all selection hints with short, intent-specific copy or annotate language.
5. Build with warnings as errors.

### Task 3: Remove post-capture preview and reduce latency

**Files:**
- Modify: `Sources/KiriApp/AppModel.swift`
- Delete: `Sources/KiriApp/QuickAccessController.swift`

**Steps:**
1. Copy quick results to the clipboard before background persistence.
2. Stop opening Quick Access after copy, save, pin, or editor completion.
3. Remove Quick Access state and stored URL data that no longer has a consumer.
4. Delete the unused preview controller.
5. Search the source tree to confirm the feature is gone.

### Task 4: Expose both modes clearly

**Files:**
- Modify: `Sources/KiriApp/KiriApp.swift`
- Modify: `Sources/KiriApp/LibraryView.swift`

**Steps:**
1. Rename the primary action to Capture & Copy.
2. Add Capture & Annotate as a secondary menu-bar, header, and onboarding action.
3. Rewrite onboarding around drag, copy, and paste rather than mandatory annotation.
4. Build with warnings as errors.

### Task 5: Verify and package

**Files:**
- Modify only if verification exposes a defect.

**Steps:**
1. Run `swift run kiri-core-tests` and expect all tests to pass.
2. Run debug and release builds with `-warnings-as-errors`.
3. Run `git diff --check` and review the complete diff.
4. Package the app and verify its code signature.
5. Inspect the packaged library UI and confirm both capture modes are visible.
