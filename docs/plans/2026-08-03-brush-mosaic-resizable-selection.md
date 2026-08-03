# Brush Mosaic and Resizable Selection Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make screenshot regions adjustable before annotation and replace rectangular mosaic regions with a continuous round brush.

**Architecture:** Keep selection adjustment inside `CaptureSessionView`'s selecting phase, using the existing pure `SelectionGeometry` resize and move functions. Represent each mosaic mark as a path plus brush-size and pixel-strength presets, then clip a pixelated source-image crop to a round stroked path for both live preview and export.

**Tech Stack:** Swift 6, AppKit, Core Graphics, Swift Package Manager, custom `KiriCoreTests` runner.

---

### Task 1: Keep a completed selection editable

**Files:**
- Modify: `Sources/KiriApp/SelectionOverlayController.swift`
- Modify: `Sources/KiriCore/SelectionGeometry.swift`
- Test: `Tests/KiriCoreTests/SelectionGeometryTests.swift`
- Test: `Tests/KiriCoreTests/main.swift`

**Steps:**

1. Replace mouse-up auto-completion with a policy that keeps a newly created region selected.
2. Wire `SelectionInteraction.creating`, `.moving`, and `.resizing` into mouse down, drag, and mouse up.
3. Use the existing eight-handle hit test, resize, and move geometry.
4. Enter annotation on a click inside an unchanged selection or on Return.
5. Draw eight visible handles and update the hint to explain resize, move, and confirmation.
6. Add focused policy tests and run `swift run kiri-core-tests`.

### Task 2: Convert mosaic rectangles to brush strokes

**Files:**
- Modify: `Sources/KiriApp/AnnotationCanvasView.swift`

**Steps:**

1. Add small, medium, and large brush-size presets.
2. Change the mosaic mark payload from a rectangle to points, brush size, and intensity.
3. Collect mosaic points during drag just like the pen tool and store one undoable mark per mouse gesture.
4. Draw a pixelated crop clipped to a round Core Graphics stroked path.
5. Use the same clipping and scaling logic for exported Retina images.
6. Draw a circular brush cursor while the mosaic tool is active.

### Task 3: Expose contextual mosaic settings

**Files:**
- Modify: `Sources/KiriApp/SelectionOverlayController.swift`
- Modify: `Sources/KiriApp/EditorWindowController.swift`

**Steps:**

1. Show size `S / M / L` and strength `Soft / Standard / Strong` only when Mosaic is selected.
2. Keep the screenshot toolbar compact and preserve the existing text-background contextual row.
3. Add matching size and strength sections to the full editor's Mosaic settings menu.
4. Keep keyboard focus on the canvas after changing a parameter.

### Task 4: Verify and install

**Files:**
- Modify: `README.md` only if the interaction description is stale.

**Steps:**

1. Run `git diff --check`.
2. Run `swift build -Xswiftc -warnings-as-errors` and expect a successful build.
3. Run `swift run kiri-core-tests` and expect all tests to pass.
4. Run `./scripts/install-app.sh` to replace `/Applications/Kiri.app`.
5. Verify resize, move, Return confirmation, Escape cancellation, continuous mosaic strokes, all size presets, and all strength presets.
