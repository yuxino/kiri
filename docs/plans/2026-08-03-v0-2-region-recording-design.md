# Kiri v0.2 Region Recording Design

## Scope

The first v0.2 slice adds silent region recording, MP4 output, and conversion
of short local videos to animated GIF. System audio, microphone capture,
pause/resume, trimming, and full-display recording remain follow-up work.

## User flow

1. Start the existing capture flow with Shift-Command-A.
2. Select, move, or resize a region using the current selection experience.
3. Choose Record Region from the toolbar's More menu.
4. Kiri removes its overlay, starts a visible recording timer, and exposes Stop
   Recording in the menu bar.
5. Stopping finalizes an MP4 and imports it into the local library.
6. A video card offers Convert to GIF when the duration is within the safe
   short-video limit.

## Architecture

`RegionRecorder` owns one `SCStream`, receives screen frames, crops through the
stream configuration's source rectangle, and appends frames to an
`AVAssetWriter` H.264 MP4. It writes to a temporary URL and only hands the file
to `AssetLibrary` after the writer finishes successfully.

`GIFExporter` uses `AVAssetImageGenerator` to sample a bounded number of frames
and ImageIO to encode a looping GIF. This keeps Kiri self-contained; invoking a
user-installed FFmpeg binary was rejected because release builds cannot assume
that dependency exists.

`AppModel` is the single recording state owner. It prevents concurrent capture
or recording sessions, publishes elapsed time for the menu bar, and imports
completed files with the existing `.video` and `.gif` asset kinds.

## Non-functional requirements

- One recording at a time, with idempotent stop.
- A failed or cancelled session must not create a library entry.
- The overlay and Kiri windows must not appear in the recording.
- MP4 dimensions must be positive and even for H.264 compatibility.
- Initial recording target: 30 fps, H.264, cursor visible, no audio.
- GIF export is bounded to 15 seconds, 12 fps, and 720 pixels on the long edge.
- All media remains local; no network service or external executable is used.

## Failure handling

- Permission and unavailable-display errors reuse the existing capture recovery
  UI.
- Writer or stream failures stop the session, remove the partial temporary
  file, and surface a user-facing error.
- GIF conversion rejects non-video assets and videos beyond the duration limit.

## Alternatives considered

- A second recording-only selection overlay: rejected because it would duplicate
  snapping, moving, and eight-handle resizing behavior.
- Bundled or system FFmpeg: rejected for the first slice because it increases
  package size, signing complexity, and process-management failure modes.
- Recording a full display and cropping after stop: rejected because it wastes
  disk bandwidth and can expose pixels outside the user's selected region in a
  recoverable temporary file.
