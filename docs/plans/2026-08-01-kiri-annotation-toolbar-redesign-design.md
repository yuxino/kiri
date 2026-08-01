# Kiri Annotation Toolbar Redesign

## Goal

Make the capture editor feel quiet, predictable, and native to macOS while
adding the editing controls users expect from a screenshot tool.

## Interaction model

The floating toolbar has three clear groups. Annotation tools come first:
pen, rectangle, arrow, text, and mosaic. History controls come next with
Undo and Redo. The trailing group contains one primary Done button and a
More menu. Done copies the rendered capture and closes the session, matching
Return. The More menu contains Save As, Pin, Open in Editor, and Clear
Annotations. This keeps frequent actions visible and infrequent actions
available without making every action compete for attention.

Command-Z undoes, Shift-Command-Z redoes, Return finishes and copies, and
Escape returns to selection. The history buttons reflect availability and
are disabled when no action is possible. A new annotation after Undo clears
the redo branch, as users expect in native editors. Clear Annotations is
disabled when the canvas is already empty.

## Visual system

The toolbar uses a single translucent macOS HUD surface, a 12-point radius,
subtle border, 6-point outer padding, and compact 30-point icon buttons.
Unselected controls are monochrome. Hover uses a low-contrast neutral fill;
the selected tool alone uses the system accent color and a restrained tinted
background. The Done button is the only filled control. Separators divide
tools, history, and completion rather than relying on color coding.

All icons use SF Symbols with consistent size and weight. Tooltips include
the action and shortcut where appropriate. Disabled history controls remain
visible at reduced opacity, preserving layout and communicating state.

## Architecture and state

`AnnotationHistory` in KiriCore owns generic undo and redo stacks. It exposes
`canUndo`, `canRedo`, append, undo, redo, and clear. The AppKit canvas stores
annotation marks in this history and reports state changes to its controller.
Both the inline capture overlay and full editor consume the same state, so
their buttons and keyboard commands cannot drift apart.

Toolbar appearance remains centralized in `CaptureUIStyle.swift`. The
overlay owns the More menu because its commands complete the capture
session. The full editor keeps its window-level completion controls but
adopts the same history behavior and keyboard shortcuts.

## Error handling and verification

Unavailable actions stay disabled instead of failing silently. Rendering or
export failures keep the current session open. Pure history transitions are
covered by KiriCore tests. Verification uses background unit tests, debug and
release compilation with warnings treated as errors, and isolated packaging.
No foreground screenshot session is launched during automated verification.
