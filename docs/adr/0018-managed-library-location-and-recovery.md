# ADR 0018: Managed library location and recovery

- Status: Accepted
- Date: 2026-08-29

## Context

Kiri previously fixed its library to the application-data directory. A missing
file, an unavailable library root, and a media playback failure also appeared
too similar in the UI. Moving storage to another disk requires the index,
assets, editable projects, and active root to change as one unit.

A recording can finish successfully while its final library import fails. The
completed media must remain recoverable without treating temporary storage as
the active library.

## Decision

- Kiri owns one active library. It contains `library.json`, `Assets/`,
  `Annotations/`, and a marker with the library schema, lineage UUID, and copy
  generation. Thumbnails are rebuildable.
- The default library stays in the operating-system application-data location.
  Settings can move the whole library to another local directory or external
  disk. Migration stages, copies, and validates the destination before
  switching; the source is left unchanged and the destination receives a new
  copy generation, preventing the retained source from being located as the
  current copy later.
- A remembered location is opened only when its marker matches. If it is
  unavailable, Kiri reports the library as offline and does not create an empty
  replacement or switch silently, including when the missing location is the
  default one. New capture and recording sessions wait until the user retries
  or locates the library. Returning to the default location is allowed only
  while the active library is available and uses the same verified migration
  path.
- Library metadata and file access resolve through one locked context. Indexed
  filenames remain single basenames, and reads reject symlinks.
- The viewer distinguishes loading, missing, unreadable, and playback-failed
  states. A missing asset can be restored through a native file picker, which
  copies the validated file back into the managed library, or its missing record
  can be removed.
- A finalized MP4 that cannot be imported is moved to a local recovery area
  with a small manifest. Kiri exposes a retry action and removes the recovery
  files only after the library import is durable.
- The WebView receives status labels and native actions, not unrestricted
  filesystem paths.

## Consequences

The active library moves as one coherent unit, including editable screenshot
projects. Disconnecting its disk produces an explicit offline state instead of
an empty library. Individual missing files and completed recordings awaiting
import have separate recovery paths.
