# Kiri v0.2 Region Recording Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a native region-recording flow with countdown, audio and pointer controls that creates MP4 library assets and converts bounded short videos to GIF.

**Architecture:** Reuse the existing selection overlay to return a region, stream that region through ScreenCaptureKit into an AVAssetWriter, then import the finalized file through AssetLibrary. Convert video assets with AVAssetImageGenerator and ImageIO so release builds have no FFmpeg dependency.

**Tech Stack:** Swift 6, AppKit/SwiftUI, ScreenCaptureKit, AVFoundation, CoreMedia, CoreVideo, ImageIO, KiriCore.

---

### Task 1: Recording and GIF policy models

**Files:**
- Create: `Sources/KiriCore/RecordingPolicy.swift`
- Create: `Tests/KiriCoreTests/RecordingPolicyTests.swift`
- Modify: `Tests/KiriCoreTests/main.swift`

Define even recording dimensions, duration display, GIF eligibility, frame
count, and output-size constraints as pure testable logic. Run
`swift run kiri-core-tests`; policy tests must pass.

### Task 2: Import finalized media files

**Files:**
- Modify: `Sources/KiriCore/AssetLibrary.swift`
- Modify: `Tests/KiriCoreTests/AssetLibraryTests.swift`

Add an atomic file-import path so large MP4/GIF files do not need to be loaded
fully into memory. Verify successful import and cleanup after a metadata write
failure.

### Task 3: Native MP4 region recorder

**Files:**
- Create: `Sources/KiriApp/RegionRecorder.swift`
- Modify: `Package.swift`

Build a single-session SCStream output and AVAssetWriter pipeline. Start the
writer on the first complete frame, normalize timestamps to zero, make stop
idempotent, and remove partial files on failure. Run strict `swift build`.

### Task 4: Return a recording region from the overlay

**Files:**
- Modify: `Sources/KiriApp/SelectionOverlayController.swift`
- Modify: `Sources/KiriApp/AppModel.swift`

Add Record Region to More Actions. Return the top-left selection rectangle
without rendering annotations, close the overlay, and start RegionRecorder for
the selected display. Verify screenshot actions remain unchanged.

### Task 5: Recording state and stop controls

**Files:**
- Modify: `Sources/KiriApp/AppModel.swift`
- Modify: `Sources/KiriApp/KiriApp.swift`
- Modify: `Sources/KiriApp/LibraryView.swift`

Publish recording state and elapsed time, expose Stop Recording in the menu bar
and main library, and import the finalized MP4. Confirm capture is disabled
while recording and stop remains safe when pressed twice.

### Task 6: Native GIF exporter

**Files:**
- Create: `Sources/KiriApp/GIFExporter.swift`
- Modify: `Sources/KiriApp/AppModel.swift`
- Modify: `Sources/KiriApp/LibraryView.swift`

Sample at 12 fps, cap input at 15 seconds, scale the long edge to 720 pixels,
encode an infinite-loop GIF, and import it as `.gif`. Expose Convert to GIF on
video cards with progress/error feedback.

### Task 7: Documentation and release verification

**Files:**
- Modify: `README.md`
- Modify: `README_EN.md`
- Modify: `README_JA.md`
- Modify: `ROADMAP.md`

Run `swift build -Xswiftc -warnings-as-errors`, `swift run kiri-core-tests`,
package/install Kiri, record a short fixture region, stop it, play the MP4, and
convert it to GIF. Confirm both assets appear with correct dimensions and
duration before committing.

### Task 8: First-line recording setup

**Files:**
- Create: `Sources/KiriApp/RecordingOptionsPopoverController.swift`
- Create: `Sources/KiriApp/RecordingCountdownController.swift`
- Modify: `Sources/KiriApp/SelectionOverlayController.swift`
- Modify: `Sources/KiriApp/AppModel.swift`
- Modify: `Sources/KiriApp/RegionRecorder.swift`
- Modify: `Sources/KiriApp/Info.plist`

Promote Record Region to the main selection toolbar. Present remembered switch
controls for countdown, system audio, microphone, pointer, and click feedback.
Request microphone permission on demand, run a cancellable 3-2-1 overlay, and
use ScreenCaptureKit's native multi-media recording output on macOS 15 and
later. Verify silent, system-audio, and microphone-enabled recordings plus
countdown cancellation.
