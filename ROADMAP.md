# kiri roadmap

kiri grows from a reliable still-capture tool into a local visual capture
workspace. Dates are intentionally omitted until each milestone is stable.

## v1.3 — Tauri multi-platform rewrite

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
- [x] Click and rubber-band library selection with batch actions
- [x] Clipboard-first screenshot completion with focus restoration
- [x] Local OCR (macOS Vision, Windows.Media.Ocr)
- [x] Region recording (SCK / WGC) with ffmpeg H.264 + AAC pipeline
- [x] Optional system audio, pointer, and click highlights
- [x] 3-2-1 countdown and multi-segment recording pipeline
- [x] GIF export (≤15 s, 12 fps, 720 px long edge)
- [x] English, Simplified Chinese, and Japanese, following the OS language

## v1.4 — Secure remote OCR and release reliability

- [x] Local OCR remains the default and requires no account or network
- [x] Multiple optional Alibaba Cloud, OpenAI, and image-capable OpenAI Chat Completions-compatible profiles
- [x] Per-selection confirmation showing destination, model, and image details
- [x] Explicit Send/Retry only, with no automatic retry, provider switch, or fallback upload
- [x] API keys stored in macOS Keychain or Windows Credential Manager
- [x] Settings view for language and OCR profile management
- [x] Escape cancellation remains reliable when an overlay control has focus
- [x] Correct selection dimming without stacked capture masks
- [x] Stable macOS development identity for persistent privacy permissions
- [x] One transparent desktop icon source with dev, production, and CI validation
- [x] Release jobs verify release tools and app icons before packaging
- [x] FFmpeg is downloaded on the first recording or explicit GIF conversion when absent, then cached locally

Release validation still open:

- [ ] macOS packaged-app acceptance (capture, permissions, focus, and recording export)
- [ ] Windows acceptance testing (capture, recording, audio, OCR, ripple)
- [ ] Pause/resume and exported-control exclusion acceptance on both platforms
- [ ] Mixed-scale multi-display acceptance testing
- [ ] Signed and notarized release builds (macOS) + Authenticode (Windows)
- [ ] Verify the final arm64, x64, and Windows v1.4.0 installers

## Later

- [ ] Blur annotation
- [ ] Full-display recording
- [ ] MP4 trimming
- [ ] Inline video and GIF playback
- [ ] Recording duration and file-size safeguards
- [ ] Smart collections
- [ ] Optional user-controlled sync
