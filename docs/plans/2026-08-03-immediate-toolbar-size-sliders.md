# Immediate Toolbar and Size Sliders Implementation Plan

**Goal:** Show the annotation toolbar immediately after a screenshot region is created and provide continuous size sliders for every size-sensitive annotation tool.

**Architecture:** Keep region creation and adjustment in the selecting phase while preparing a hidden annotation canvas and visible toolbar for the current crop. Selecting a drawing tool activates that canvas and locks the region. Store each mark's resolved size so later slider changes do not alter existing annotations.

**Tech Stack:** Swift 6, AppKit, Core Graphics, Swift Package Manager, custom `KiriCoreTests` runner.

---

### Task 1: Show the toolbar with an adjustable selection

**Files:**
- Modify: `Sources/KiriApp/SelectionOverlayController.swift`

**Steps:**

1. Prepare a hidden annotation canvas and visible toolbar as soon as a valid selection is released.
2. Keep all tool buttons visually unselected while the selection is still adjustable.
3. Rebuild the hidden canvas whenever the selection is moved or resized.
4. Activate and reveal the canvas when a drawing tool or its keyboard shortcut is selected.
5. Make Return finish the crop directly and Escape cancel the session.

### Task 2: Store continuous sizes per annotation mark

**Files:**
- Modify: `Sources/KiriApp/AnnotationCanvasView.swift`

**Steps:**

1. Add independent pen width, shared shape width, text font size, and mosaic brush diameter properties.
2. Replace fixed widths and font sizes in annotation marks with the current resolved value.
3. Apply the stored value consistently to live preview and Retina export.
4. Update text editor sizing and mosaic brush cursor live as sliders move.

### Task 3: Add contextual sliders

**Files:**
- Modify: `Sources/KiriApp/SelectionOverlayController.swift`
- Modify: `Sources/KiriApp/EditorWindowController.swift`

**Steps:**

1. Add an AppKit slider and numeric value label to the current-tool row.
2. Use ranges Pen 1–24 px, Shapes 1–16 px, Text 12–64 pt, Mosaic 12–120 px.
3. Keep text background and mosaic strength beside their size sliders.
4. Mirror the controls in the full editor and keep sliders keyboard accessible.

### Task 4: Verify and install

**Files:**
- Modify: `README.md`
- Modify: `README_EN.md`
- Modify: `README_JA.md`

**Steps:**

1. Run `git diff --check`.
2. Run `swift build -Xswiftc -warnings-as-errors`.
3. Run `swift run kiri-core-tests`.
4. Install with `./scripts/install-app.sh`.
5. Verify immediate toolbar appearance, resize/move before tool choice, region lock after tool choice, all sliders, Enter, Escape, undo, and export.
