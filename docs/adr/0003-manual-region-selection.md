# ADR 0003: Single-outline window hover

- Status: Accepted
- Date: 2026-08-04
- Supersedes: The stacked hover-preview styling in earlier capture plans

## Context

The original capture overlay used stacked border treatments and additional
pointer-adjacent feedback while highlighting the window under the pointer. In
real use this felt like several boxes following the pointer. Removing hover
entirely made one-click whole-window selection less discoverable.

Kiri supports free region creation, moving, and eight-handle resizing. Window
recognition should remain visible, but its hover state must be visually quiet.

## Decision

Screenshot and recording selection use restrained window recognition:

- Hovering an eligible window displays exactly one violet outline.
- Hover does not display handles, dimensions, a stacked white border, a loupe,
  or a pointer-following tooltip.
- A click selects the highlighted frontmost window.
- Dragging creates the region.
- Dragging inside an existing region moves it.
- Dragging any of the eight handles resizes it.
- The platform capture backend collects visible-window rectangles for hover and
  click hit testing.

## Consequences

The overlay keeps one-click whole-window capture discoverable without the visual
noise of several nested or pointer-following elements. A clicked window becomes
a normal selection and can be moved or resized, so the user retains exact size
control.
