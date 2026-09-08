# ADR 0030: Text history and saved screenshot OCR

Status: accepted

## Decision

OCR history is primarily a text-reading surface. A dedicated Text History
navigation item opens a searchable chronological list with text excerpts and
a reading pane. The pane keeps line breaks and allows text selection or whole
text copying. The source image is behind View Source Image, with full-size
viewing available on click. Search uses the stored text, not image filenames
alone. Empty and failed recognition do not create history records.

Successful screen OCR, local or explicitly approved remote OCR, automatically
saves the selected PNG and text locally. A storage failure does not discard
recognized text: the result remains copyable with an unsaved warning.
Closing or canceling capture before recognition completes prevents late saves.

Saved screenshot menus and viewers offer Recognize Text Locally. The editor
labels its action Recognize Saved Image Locally because unsaved changes are
not part of the request. A result dialog presents text immediately on success.
Closing that dialog allows its already-started local operation to complete and
save. No historical image is uploaded by this action, regardless of the active
screen-OCR provider. At most one historical image is recognized at a time.

Each text record owns an independent snapshot and optional `ocrText` metadata
on an ordinary image asset. It is excluded from the capture grid, included in
Trash, searchable, and migrated with the managed library. Source edits and
deletion cannot change a historical snapshot. Removing a text record moves its
text and snapshot to recoverable Trash without removing the original screenshot.
Old indexes decode with absent OCR metadata and retain their existing behavior.
