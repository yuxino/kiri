# ADR-0002: Use native macOS media frameworks for recording and GIF export

## Status

Accepted

## Context

Kiri v0.2 needs region recording, MP4 export, and short-video GIF conversion.
The distributed application must work after dragging Kiri.app into Applications
without requiring Homebrew or another executable.

## Decision

Use ScreenCaptureKit for region frame delivery, AVFoundation for H.264 MP4
writing and frame extraction, and ImageIO for animated GIF encoding.

## Consequences

### Positive

- No external runtime dependency.
- Media work stays inside Kiri's signed process and permission model.
- The implementation uses frameworks already shipped with supported macOS.

### Negative

- GIF palette and compression controls are less extensive than FFmpeg.
- Audio synchronization requires a later extension to the writer pipeline.

### Neutral

- Kiri must link AVFoundation, CoreMedia, CoreVideo, and ImageIO.

## Alternatives considered

- Invoke a user-installed FFmpeg binary: rejected because availability and
  version cannot be guaranteed.
- Bundle FFmpeg: deferred because binary size, licensing review, signing, and
  update management are disproportionate for the first recording slice.
