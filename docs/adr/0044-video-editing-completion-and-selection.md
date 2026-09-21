# 0044 — Completing an edit without losing the user's place

Status: accepted

## Context

End-to-end editing exposed gaps that individual tool checks missed. An imported
file lost its recognizable name and could remain hidden by an old search. A
selected effect still left a Delete segment action in the timeline. Export
startup depended on the next animation frame, which WebKit can suspend in a
covered window. Closing a window discarded edits without warning.

## Decision

Local imports retain the original file stem as their display title, in the same
library transaction as the copied file. Internal filenames remain generated.
Successful imports return the library to its unfiltered capture view at the top;
cancelled or entirely failed imports leave the current view intact.

Timeline deletion follows the selected object: video clip, annotation, sticker
or effect. Keyboard deletion follows the same rule. Scrubbing the video ruler
and splitting a clip return selection to the video. Deletion remains undoable.

The inspector's collapsed Visible during summary uses finished-video time,
matching the timeline after cuts and speed changes. If an effect appears in
separate retained ranges, the summary lists those ranges. Expanded numeric
controls explicitly retain the original-video clock, which anchors effects to
their source content through cuts and reordering.

Export yields to an ordinary task before preparing overlays and invoking the
native encoder; it never waits for an animation frame to start that operation.
The native media and non-destructive copy boundaries remain unchanged.

Closing a video window with unexported changes requires an explicit discard
choice. Escape, Cmd/Ctrl+W and native close requests use the same guard. Closing
is deferred while export is in progress. Leaving the editor for playback keeps
the edit in that window. This is not persistent project storage: exported MP4s
are flattened videos and editable projects do not survive window closure.

## Verification

Tests cover imported title persistence, searching, bounds and preservation of
the source. Native acceptance imports actual footage, cuts and changes speed,
adds timed content, exports copies, and reopens the resulting videos. It also
checks selected-object deletion, undo and cancelling the close warning.
The native close listener also needs the window-destroy capability to finish a
permitted close; this is scoped to viewer windows. Both cancellation and actual
closure must be checked in the packaged app, including a fresh unedited viewer.

This iteration does not add multi-source video composition, photo sequences,
music tracks or persistent video projects. Importing several files into the
library must not be described as merging them into one timeline.
