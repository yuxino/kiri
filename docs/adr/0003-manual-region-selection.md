# ADR 0003: Window selection without hover outlines

- Status: Accepted
- Date: 2026-08-04
- Supersedes: The hover-preview portions of ADR 0001 and earlier capture plans

## Context

The original capture overlay highlighted the window under the pointer before a
click. In real use the changing outline felt like several boxes following the
pointer and competed with the user's intent to choose an exact region. Removing
both the preview and click selection went too far because one-click whole-window
selection remains useful.

Kiri supports free region creation, moving, and eight-handle resizing. Window
recognition should complement that flow without producing hover noise.

## Decision

Screenshot and recording selection use silent window recognition:

- Moving the pointer before a selection does not display a window outline.
- A click without a drag selects the frontmost eligible window at that point.
- Dragging creates the region.
- Dragging inside an existing region moves it.
- Dragging any of the eight handles resizes it.
- `CaptureCoordinator` collects visible-window rectangles for click hit testing,
  but the overlay never uses them to draw hover state.

## Consequences

The overlay keeps one-click whole-window capture without visual noise while the
pointer moves. A clicked window becomes a normal selection and can be moved or
resized, so the user retains exact size control. Historical plans and ADR 0001
remain useful background, but their hover-preview requirement is no longer
active.
