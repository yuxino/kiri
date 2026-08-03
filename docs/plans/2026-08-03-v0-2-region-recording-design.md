# Kiri v0.2 Region Recording Design

## Scope

The first v0.2 slice adds region recording, MP4 output, and conversion of short
local videos to animated GIF. A polished recording setup adds a 3-2-1
countdown, system audio, microphone, cursor, and click-highlight controls.
Pause/resume, trimming, and full-display recording remain follow-up work.

## User flow

1. Start the existing capture flow with Shift-Command-A.
2. Choose Screenshot or Record from the first-level mode switch.
3. In Record mode, select, move, or resize a region. Kiri automatically shows
   five remembered switches in a compact setup card.
4. Start recording. Kiri requests microphone access only when required, shows
   a cancellable 3-2-1 countdown, removes its overlay, starts a visible timer,
   and exposes Stop Recording in the menu bar.
5. Stopping finalizes an MP4 and imports it into the local library.
6. A video card offers Convert to GIF when the duration is within the safe
   short-video limit.

While recording, a compact excluded Kiri panel shows elapsed time, pause or
resume, and stop. Pausing finalizes the current safe segment. Resuming starts a
new segment with identical capture settings; stopping concatenates the segments
without including paused wall-clock time.

## Architecture

`RegionRecorder` owns one `SCStream` and crops through the stream
configuration's source rectangle. On macOS 15 and later, native
`SCRecordingOutput` combines the selected screen, system audio, and microphone
media in an H.264 MP4. The macOS 14 fallback retains the explicit
`AVAssetWriter` pipeline and supports screen plus system audio. It writes to a
temporary URL and only hands the file to `AssetLibrary` after finalization.

`GIFExporter` uses `AVAssetImageGenerator` to sample a bounded number of frames
and ImageIO to encode a looping GIF. This keeps Kiri self-contained; invoking a
user-installed FFmpeg binary was rejected because release builds cannot assume
that dependency exists.

`AppModel` is the single recording state owner. It prevents concurrent capture
or recording sessions, publishes elapsed time for the menu bar, and imports
completed files with the existing `.video` and `.gif` asset kinds.

`RecordingSegmentMerger` concatenates video and all available audio tracks with
`AVMutableComposition`, then exports a single MP4. Partial segments stay in the
temporary directory and are removed after import or on failure.

## Non-functional requirements

- One recording at a time, with idempotent stop.
- A failed or cancelled session must not create a library entry.
- The overlay and Kiri windows must not appear in the recording.
- Default copy and recording flows must hide Kiri's library and return focus to
  the application that was active before capture.
- MP4 dimensions must be positive and even for H.264 compatibility.
- Recording target: 30 fps H.264 with independently controlled system audio,
  microphone, cursor, and click feedback.
- Audio is opt-in. Microphone permission is requested only when its switch is
  enabled, and no audio leaves the device.
- Microphone capture and native click feedback require macOS 15 or later; the
  setup card communicates unavailable controls on macOS 14.
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
