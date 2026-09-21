# 0045 — Local video projects and cancellable export

Status: accepted

## Context

An export is a shareable result, not a replacement for an editable project.
Asking users to export before every window closure made ordinary editing feel
fragile. Long native exports also needed visible progress and a way to stop
without losing the edit or adding an incomplete library asset.

## Decision

Each source video may own an optional versioned project in the active library's
`VideoProjects` directory. It records the retained source intervals, speeds,
timed effects, editable annotations, embedded PNG stickers, export preset and
source playhead. The source file and library index format remain unchanged.
One project still edits one source video; importing several assets does not
compose them into a shared timeline.

The viewer loads a valid project before enabling editing. Edits save after a
short idle period through one serialized writer; the newest snapshot follows
any in-flight save using its returned revision. Cmd/Ctrl+S flushes that queue.
Untouched playback does not create a project. Uncommitted nonempty text is
included in autosave without interrupting IME composition or adding redundant
undo entries. Normal close commits pending text and waits for saving. A failed
save offers retry, continued editing, or an explicit close that leaves the last
successfully saved project intact. This replaces the export-or-discard close
behavior from ADR 0044.

Native storage validates the typed document, geometry, times and embedded PNG
limits. Writes are atomic. An opaque revision binds the previous document,
source metadata and library identity/generation. Corrupt projects, changed
sources and stale revisions are reported instead of silently overwritten.
Source identity uses size and modification time, not a full media-content hash.
Projects survive recoverable Trash, migrate with the library, and are removed
only after permanent deletion of the source has been committed to the index.

Every export receives a unique request ID scoped to the originating viewer.
The native worker reports preparing, rendering and saving phases; rendering
progress comes from platform media operations rather than a simulated timer.
Cancellation stops those operations and cleans temporary output. An atomic
gate separates cancellable processing from the final library import: once
saving starts, the interface finishes that save. The single-flight lease stays
held until the native worker has actually stopped. Cancelling never imports a
partial video and does not remove the editable project.

## Verification

Storage tests cover reload, stale revisions, source changes, invalid documents,
image limits, Trash, permanent deletion and library migration. Queue tests cover
rapid edits, writes in flight, failed writes, retry and close flushing. Native
export tests cover cancellation during processing and the final import gate.
Packaged-app acceptance must close and reopen a real edit, retain pending text,
cancel an export and then successfully export the same project. Windows media
checks run in hosted CI; these checks do not replace physical-device acceptance.
