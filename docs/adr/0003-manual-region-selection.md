# ADR 0003: Manual region selection

- Status: Accepted
- Date: 2026-08-04
- Supersedes: The window-snapping portions of ADR 0001 and earlier capture plans

## Context

The original capture overlay queried visible windows, highlighted the window
under the pointer, and accepted a click as a full-window selection. In real use
the changing outlines felt like several boxes following the pointer and competed
with the user's intent to choose an exact region.

Kiri already supports free region creation, moving, and eight-handle resizing,
so automatic window recognition is not required for a complete selection flow.

## Decision

Screenshot and recording selection are manual:

- Moving the pointer before a selection does not display a window outline.
- A click without a drag does not choose a window.
- Dragging creates the region.
- Dragging inside an existing region moves it.
- Dragging any of the eight handles resizes it.
- `CaptureCoordinator` does not collect visible-window rectangles for the
  selection overlay.

## Consequences

The overlay has less visual noise, performs less window metadata work, and gives
the user deterministic control over size. Kiri no longer provides one-click
whole-window capture. Historical plans and ADR 0001 remain useful background,
but their window-snapping requirements are no longer active.
