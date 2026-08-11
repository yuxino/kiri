# kiri product design

## Product

`kiri` is a native macOS visual capture workspace. Its name comes from
「切り取り」: to clip or cut out. It belongs to the same product family as
`mimi`, using a short lowercase Japanese name, a quiet light interface, fine
line art, and a restrained lavender accent.

The product is local-first. Captures remain on the Mac unless the user chooses
to share them. The library is not an optional gallery bolted onto the capture
flow: every completed capture becomes a recoverable asset with its own
metadata, thumbnail, and source file.

## Releases

### v0.1 — still capture

- Menu bar app and global shortcut
- Frozen-screen region selection with dimension feedback
- Rectangle, arrow, freehand, text, and mosaic annotations
- Undo and redo
- Copy PNG to the clipboard or save to a chosen location
- Local library with image thumbnails, search, favorites, and trash

### v0.2 — motion capture

- Screen and region recording through ScreenCaptureKit
- MP4 export
- Short recording to GIF conversion
- Video/GIF thumbnails and playback in the same asset library

### v0.3 — long capture

- Assisted scrolling capture
- Overlap detection and image stitching
- Manual seam correction when automatic stitching is uncertain

## Experience

The global shortcut enters capture mode. kiri takes a snapshot of each
display, presents it as a frozen borderless overlay, and lets the pointer drag
a region. Releasing the pointer reveals a compact toolbar near the selection.
Escape cancels, Return completes, and Command-C copies.

Completing a capture writes an original PNG and a metadata record into the
library before copying it to the clipboard. This ordering makes captures
recoverable even if a later clipboard or save operation fails. Deletion moves
an item to an app-managed trash and does not immediately destroy the source.

## Architecture

- **KiriApp** owns the menu bar, windows, keyboard shortcuts, and library UI.
- **KiriCore** owns asset models, storage paths, library indexing, geometry,
  crop calculations, and future capture-mode abstractions.
- **CaptureCoordinator** obtains Screen Recording permission and snapshots
  displays using ScreenCaptureKit.
- **SelectionOverlayController** maps AppKit screen coordinates to image pixel
  coordinates and owns the selection session.
- **AnnotationDocument** stores non-destructive drawing operations before a
  final renderer composites them.
- **AssetLibrary** stores originals under Application Support and metadata as
  JSON in v0.1. The interface is intentionally storage-agnostic so it can move
  to SwiftData if richer queries become necessary.

`CaptureKind` supports image, video, and GIF. Future formats extend capture and
rendering services without changing the library storage boundary.

## Reliability and privacy

- Missing Screen Recording permission opens a clear system-settings guide.
- Empty or off-screen selections are rejected without writing an asset.
- File writes use a temporary sibling followed by an atomic replacement.
- Retina and multi-display coordinate conversion is covered by unit tests.
- Captures never leave the device automatically.
- Trash is recoverable from the library; permanent deletion is explicit.

## Verification

Core tests cover crop geometry, metadata round-trips, search, favorites, and
trash restoration. Manual acceptance covers first-run permission, Retina
selection, mixed-scale displays, clipboard output, keyboard cancellation, and
library recovery.
