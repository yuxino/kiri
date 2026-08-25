# ADR 0010: Single-action screenshot completion

- Status: Accepted
- Date: 2026-08-25

## Context

The screenshot toolbar ended with a primary completion button and an overflow
menu containing reselect, Save As, pin, editor, and clear-annotation actions.
Most of those actions duplicated the adjustable selection, annotation history,
or the local library. The menu made the high-frequency capture path look more
complex than it was.

The Save As action also opened a blocking native save panel from a synchronous
main-thread command. Cancelling that panel could re-enter WebKit custom-protocol
handling and leave the application thread waiting on a semaphore.

## Decision

- Screenshot completion has one visible action: copy the rendered selection to
  the clipboard, import it into the local library, close the overlay, and
  restore focus to the originating application.
- Remove the overflow menu and its reselect, immediate Save As, pin, editor,
  and clear-all entries. Remove the hidden screenshot Save As shortcut as well.
- Remove the floating pinned-image window and its in-memory protocol route.
- Keep file access and export in the library/editor workflows instead of the
  capture overlay.
- Run any remaining native save panel outside the application thread, and
  serve `kiri://` resources through the asynchronous protocol responder so
  image decoding and media I/O cannot block AppKit.

## Consequences

The screenshot toolbar ends with one unambiguous completion button and the
capture contract stays clipboard-first. Low-frequency actions no longer compete
with annotation tools, and cancelling a save panel cannot deadlock the main
thread with a custom resource request.
