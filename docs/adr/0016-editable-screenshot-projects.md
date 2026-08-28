# ADR 0016: Editable screenshot projects

- Status: Accepted
- Date: 2026-08-29
- Partially supersedes: The image-thumbnail viewer action in ADR 0012
- Preserves: The single clipboard-first completion action in ADR 0010

## Context

Kiri previously saved only the flattened screenshot. That format is portable
and keeps thumbnails, clipboard copy, export, and the Swift-compatible library
index simple, but text, shapes, and mosaic strokes cannot be selected or
changed after capture. The completion card also sent every media thumbnail to
the quick viewer even when continuing an image edit was the more useful action.

Older libraries and the Swift implementation must remain readable. A corrupt
or stale project must not pair marks with a different image, and making mosaic
re-editable must not hide the fact that an unredacted source remains on disk.

## Decision

- `Assets/<filename>.png` remains the canonical flattened image used by the
  library, thumbnails, clipboard copy, quick preview, drag-out, and export.
  `library.json` does not gain annotation fields.
- Editable state lives under `Annotations/` as a versioned JSON wrapper and an
  immutable `<asset-id>.source.png`. A capture with visible marks creates both
  immediately. An unannotated or historical image creates them only after its
  first annotated editor save, using the current flattened image as the source.
- Annotation document V1 stores a fixed logical canvas, immutable source-pixel
  dimensions, and ordered marks. Both Rust and the frontend strictly validate
  schema, enum, geometry, count, text, and aggregate-size bounds.
- Capture export and Rust source cropping use the same rounded integer pixel
  crop. The flattened PNG and document are staged with an owner-bound,
  one-time token so two completion requests cannot cross-pair their payloads.
- Opening an editor returns a content-addressed revision covering the current
  flat image and the exact presence and bytes of both project files. The custom
  protocol serves the source only while that revision still matches. Saving is
  compare-and-swap: a changed flat image, sidecar, or source is rejected instead
  of being overwritten by a stale editor.
- Missing project files open the current flat image as a new baseline. Invalid
  or stale project files also fall back visibly to the current flat image, and
  can be replaced only while the same invalid revision is still current.
- Clicking an image in the completion card opens the editor. Double-click and
  the primary context-menu action edit images in the library; the explicit eye
  action remains a flat quick preview. Video and GIF actions still use the
  viewer.
- Screenshot completion itself remains one action: copy, save locally, close
  the overlay, and restore the original application. Kiri never opens the
  editor automatically after capture.
- Moving an asset to Trash retains its project. Restore brings the whole
  project back. Permanent single, batch, and empty-Trash deletion remove the
  flat asset, thumbnail, sidecar, and clean source.
- A native Save panel gives the WebView an opaque one-time authorization, not
  an unrestricted filesystem path.

## Consequences

New Kiri annotations can be selected and changed after reopening at a different
editor size without coordinate drift. Historical marks already baked into a
flat PNG cannot be reconstructed, although the image can start a new project.

Editable projects use more local storage and retain a clean source that may
contain pixels covered by mosaic, text backgrounds, or shapes. Privacy copy
must say this plainly. The files never leave the device through this feature,
remain recoverable in Trash, and are removed by permanent deletion.

The flat asset, sidecar, and source are separate files rather than one database
transaction. Normal write failures restore the previous snapshot on a
best-effort basis; hashes and revision checks make an interrupted or externally
modified project fail closed on the next load.
