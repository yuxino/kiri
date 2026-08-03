# Capture Escape Cancel Design

## Goal

Pressing Escape must immediately cancel the active capture session during region
selection, annotation, or inline text entry.

## Design

Use a dedicated borderless capture window that explicitly allows key-window
status. The window intercepts the physical Escape key before AppKit dispatches it
to the current first responder, so controls and text fields cannot consume the
event first. It also implements `cancelOperation` for AppKit's semantic cancel
path.

When the overlay is presented, Kiri activates and makes the overlay key. Both
Escape paths call the capture session's existing cancellation callback, preserving
the established cleanup and state-reset behavior. The handler is cleared before
the window closes to prevent a stale callback or duplicate cancellation.

## Verification

- Build with warnings treated as errors and run all core tests.
- Start a capture and press Escape before selecting a region.
- Start another capture, select a region, and press Escape from annotation mode.
- Confirm the overlay closes, the library returns, and a new capture can start.
