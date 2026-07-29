# Kiri Capture Workflow Design

## Product goal

Kiri should feel invisible until the user needs it. The primary workflow is:

1. Press the capture shortcut.
2. Hover a window to snap to it, or drag a custom region.
3. Annotate directly on the frozen capture without opening another window.
4. Press Return to copy the result.
5. Use a small corner overlay to drag, save, pin, or open the full editor.
6. Find every completed capture in history later.

The capture library remains useful, but it is no longer the center of the
capture experience. OCR, scrolling capture, and recording remain outside this
slice so the basic interaction can become fast and reliable first.

## Capture session

A single `CaptureSessionController` owns the frozen display, the borderless
overlay window, selection state, and inline annotation state. During selection,
hovering highlights the topmost eligible window. A click accepts that window;
dragging creates a manual region. Existing resize and move behavior remains
available before annotation begins.

Confirming the region changes the same overlay into annotation mode. The
selected pixels stay in their original screen position, the rest of the screen
remains dimmed, and a compact toolbar sits next to the selection. Pen,
rectangle, arrow, text, mosaic, and undo reuse one annotation canvas shared
with the full editor. Escape returns from annotation to selection before it
cancels the entire session.

## Completion and quick access

Return is the default completion action: render annotations, copy the PNG, and
persist it to Kiri's library. Persistence happens before Quick Access appears,
so its drag source always references a stable local file. The corner overlay
shows the image plus Copy, Save, Pin, Edit, and Close actions. Dragging the
thumbnail exports a copy and never moves the library original.

Pin creates an always-on-top resizable panel. Edit opens the existing full
editor only when explicitly requested. Quick Access auto-dismisses after a
short delay but pauses while the pointer is over it.

## Reliability and privacy

All capture and annotation data stays local. Cancellation never creates a
history item. A failed clipboard write still preserves history and surfaces a
non-blocking error. A failed history write keeps the rendered image available
for copying. Window metadata is used only for snapping and is discarded when
the session ends.

Geometry and action decisions live in testable core types. AppKit integration
is verified with strict builds plus a manual interaction pass covering region,
window, Escape, Return, drag export, pin, save, and edit.
