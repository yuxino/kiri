# Kiri Capture Flow Reset Design

## Goal

Make Kiri's default capture feel immediate while keeping annotation available
without forcing it into every screenshot.

## Product references

ShareX's standard region capture completes when the pointer is released and
then runs configured after-capture tasks. Its adjustable multi-region mode is
an explicit alternative rather than the default. Flameshot keeps annotation
inside the capture surface and exposes copy/save as clear completion actions.
Kiri will combine those strengths: a one-gesture default and a separate inline
annotation entry point.

- https://getsharex.com/docs/region-capture.html
- https://flameshot.org/

## Interaction model

Kiri exposes two capture intents wherever capture starts. **Capture & Copy** is
the primary action and the global-shortcut behavior. Dragging a valid region or
clicking a highlighted window immediately copies it, closes the overlay, and
saves it to History in the background. **Capture & Annotate** uses the same
selection behavior, then opens the existing inline annotation tools. Done or
Return copies and closes.

The selection is never left in an artificial intermediate state, so resize
handles and move behavior are removed from the basic gesture. During a drag,
Kiri shows dimensions, a loupe, and a short intent-specific instruction. Escape
always cancels; in annotation it returns to a fresh selection.

## Completion and feedback

Clipboard writing happens before library persistence so the result is ready as
soon as the overlay closes. History persistence remains automatic and local.
Failures continue to surface in the library and menu bar with recovery text.
There is no post-capture preview, floating screenshot, or task panel. Save,
pin, and full editor remain secondary actions inside annotation's More menu.

## Surfaces and states

The library header and first-run state name both intents explicitly. The menu
bar lists quick copy first and annotation second. Existing loading, empty,
search, trash, permission, keyboard, hover, focus, and disabled states remain.
The primary action uses the accent color; annotation stays secondary so the
fast path is visually unambiguous.

## Verification

Core tests cover valid and invalid mouse-up completion. Strict debug and
release builds catch AppKit/SwiftUI integration errors. The packaged app is
then inspected through its real library UI, and source search verifies that no
Quick Access controller or preview path remains.
