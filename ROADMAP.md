# kiri roadmap

kiri grows from a reliable still-capture tool into a local visual capture
workspace. Dates are intentionally omitted until each milestone is stable.

## v0.3 — Tauri multi-platform rewrite

The macOS + Windows rewrite in Tauri 2 (1:1 migration from the Swift
original; behavior specs live in `docs/spec/swift/`).

- [x] Tauri 2 + Rust + React project skeleton
- [x] Platform-independent core (geometry, recording policy, shortcut, library)
- [x] Frozen-display capture (macOS ScreenCaptureKit, Windows xcap/WGC)
- [x] Exclusive global shortcut (⇧⌘A / Shift+Ctrl+A)
- [x] Overlay: mode selector, window hover outline, region drag, 8 handles
- [x] Annotation canvas: pen, rectangle, line, arrow, text, mosaic
- [x] Annotation history (undo/redo), inline text editing, live sizing
- [x] Local library with search, favorites, and recoverable trash
- [x] Clipboard-first screenshot completion with focus restoration
- [x] Local OCR (macOS Vision, Windows.Media.Ocr)
- [x] Region recording (SCK / WGC) with ffmpeg H.264 + AAC pipeline
- [x] Optional system audio, pointer, and click highlights
- [x] 3-2-1 countdown, pause/resume segment merging
- [x] GIF export (≤15 s, 12 fps, 720 px long edge)
- [x] English + Simplified Chinese, following the OS language

Before a stable v0.3 release:

- [ ] Windows acceptance testing (capture, recording, audio, OCR, ripple)
- [ ] Mixed-scale multi-display acceptance testing
- [ ] Signed and notarized release builds (macOS) + Authenticode (Windows)
- [ ] ffmpeg bundled in release artifacts (scripts/ensure-ffmpeg.mjs)

## Later

- [ ] Blur annotation
- [ ] Full-display recording
- [ ] MP4 trimming
- [ ] Inline video and GIF playback
- [ ] Recording duration and file-size safeguards
- [ ] Tags and smart collections
- [ ] Optional user-controlled sync
