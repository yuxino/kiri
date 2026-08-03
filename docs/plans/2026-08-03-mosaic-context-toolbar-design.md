# Mosaic Context Toolbar Design

## Problem

The capture toolbar exposes Mosaic as an unlabeled grid icon among many controls.
It has no visible strength setting or drag feedback, while Text Background remains
visible even when another tool is active. The result looks like Mosaic is missing
and makes the toolbar hierarchy difficult to scan.

## Design

Keep the first row focused on universal capture actions and the six annotation
tools. Use the existing second row as a contextual parameter area:

- Mosaic shows Soft, Standard, and Strong segmented options plus “Drag over an
  area to pixelate”. Standard is the default.
- Text shows Transparent, Dark, and Light background options plus a placement
  hint.
- Other tools keep the compact Return, Escape, and Undo hint.

Each mosaic annotation stores its chosen strength. Pixel block size is calculated
in view points and converted to source pixels so preview and Retina export have
matching visual strength. While dragging, Kiri draws a dashed accent boundary
around the pending mosaic area.

The full editor keeps compact contextual option buttons and only displays the
option relevant to the selected tool.

## Verification

- Build with warnings as errors and run all core tests.
- Verify the toolbar switches context for Text and Mosaic without clipping.
- Compare Soft, Standard, and Strong in preview and exported images.
- Confirm Undo, Return, and Escape still work after changing mosaic strength.
