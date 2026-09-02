# ADR 0024: Native media pipelines

- Status: Accepted
- Date: 2026-09-02

## Context

Kiri previously launched a downloaded FFmpeg executable on macOS for MP4
recording, paused-segment merging, video metadata, thumbnails, and GIF export.
Windows had already moved recording and GIF work to system media APIs. The
download added a network exception to a local-first product, increased the
runtime dependency and validation surface, and made the two platforms follow
different trust models.

The current macOS capture backend already delivers bounded BGRA frames and PCM
audio from ScreenCaptureKit. macOS 14 also provides all required media
primitives through AVFoundation and ImageIO. `SCRecordingOutput` was considered
but would require macOS 15 and would bypass Kiri's existing region, queue,
pause-segment, and click-ripple boundaries.

## Decision

- Keep ScreenCaptureKit capture and the existing bounded Rust handoff queues.
- Encode macOS segments with `AVAssetWriter`, H.264 video, and AAC audio.
- Convert captured audio to Kiri's 48 kHz stereo PCM policy before handing it
  to AVAssetWriter.
- Merge paused segments with `AVMutableComposition` and
  `AVAssetExportSession` using passthrough export.
- Read video metadata and first frames with AVFoundation.
- Create looping GIFs with `AVAssetImageGenerator` and ImageIO.
- Keep Windows on Media Foundation and Windows imaging APIs.
- Remove FFmpeg discovery, download, archive verification, process execution,
  pipe plumbing, and dependency crates from both runtime paths.

The Objective-C bridge is compiled into Kiri and contains no separately
installed or downloaded runtime component.

## Consequences

Recording, recovery validation, thumbnail generation, paused-segment merging,
and GIF conversion remain local and work offline without a third-party media
executable. macOS keeps its macOS 14 minimum and its existing pause/resume
semantics. Native media framework behavior must be covered by macOS round-trip
tests and packaged-app recording acceptance; Windows still requires its own
exact-artifact native acceptance.
