# Kiri architecture

Status: current for the Tauri 2 application.

Kiri is a local-first desktop capture workspace for macOS and Windows. React
renders the application windows, Rust owns capture, persistence, credentials,
network access, and platform integration, and Tauri provides the window and IPC
boundary.

## Canonical project layout

- `src/` contains the React UI, annotation model, translations, and typed IPC
  client.
- `src-tauri/` is the only Rust workspace and the only Tauri application.
- `src-tauri/src/core/` contains portable geometry, policy, library, and OCR
  profile models.
- `src-tauri/src/capture/` and `src-tauri/src/platform/` contain platform
  implementations.
- `scripts/` contains release checks, icon generation, and stable macOS signing.

There is deliberately no second root Cargo workspace or parallel Tauri app.
Commands should use `--manifest-path src-tauri/Cargo.toml` when they are run
outside the Tauri CLI.

## Window model

All windows share the Vite entry point and select their React root through the
`?window=` query parameter. Each root is loaded as its own dynamic chunk, so a
small utility window does not parse and retain the library, overlay, and editor
modules.

| Label | Purpose |
| --- | --- |
| `library` | Capture library and Settings |
| `overlay` | Active-display capture, selection, annotation, and OCR consent |
| `countdown` | Recording countdown |
| `control-panel` | Recording pause/resume/stop controls |
| `ripple` | Optional recorded click highlight |
| `editor-*` | Full screenshot editor |
| `pin-*` | Floating pinned image |
| `viewer-*` | Image, video, or GIF viewer |
| `toast` | Transient status feedback |
| `confirm` | Destructive-action confirmation |

The backend owns window creation and validates commands against the expected
window and active session. Frontend code never receives credentials or an
unrestricted filesystem path.

## Capture flow

1. The native global shortcut asks Rust to start a capture session and records
   the previously focused application.
2. macOS freezes the active display with ScreenCaptureKit. Windows uses the
   Windows Graphics Capture path exposed through `xcap`.
3. Rust keeps one reference-counted allocation for the full frozen PNG and
   shares it with the session, OCR preparation, and custom protocol. The
   overlay receives a capture-scoped, unguessable `kiri://` URL. The image is
   not written to disk.
4. The overlay performs window hit testing, region selection, and annotation.
5. Screenshot confirmation sends only the rendered selected PNG to Rust. Rust
   validates its header and decodes it under capture-sized allocation limits,
   then imports it into the local library, copies it to the clipboard, tears
   down the session, and restores focus.

Escape cancels the active session and releases its frozen image. There is no
runtime synthetic-desktop or temporary-library mode in development or
production; deterministic capture data belongs in unit tests or an isolated
test harness.

## OCR flow

Local OCR is the default and runs through macOS Vision or Windows.Media.Ocr.
The normal local path does not use the network.

Remote OCR profiles contain only non-secret metadata. API keys live in macOS
Keychain or Windows Credential Manager. For a remote profile, Rust prepares
only the selected crop and returns a disclosure containing the profile,
destination origin, model, pixel dimensions, and byte size. A visible Send or
Retry action is required for every request. Return performs local OCR for that
selection. Redirects, automatic retries, provider switching, and upload
fallbacks are disabled.

Prepared crops are bounded, expire from memory, and are tied to both the active
capture and profile revision. Provider HTTP requests originate in Rust; the
WebView CSP does not allow direct provider access.

## Recording and GIF flow

Platform capture produces BGRA video frames and optional PCM audio. macOS uses
ScreenCaptureKit. Windows uses Windows Graphics Capture plus WASAPI through
`cpal`. Rust feeds the actual pixel and audio formats to FFmpeg and produces a
30 fps H.264/HEVC MP4 with AAC audio. Hardware encoding is probed first and
falls back to `libx264`.

The native-to-encoder video handoff has a hard two-frame capacity. On macOS,
ScreenCaptureKit's native IOSurface queue is independently limited to three
frames. On Windows, WGC delivery is throttled to the 30 fps recording policy
when the OS supports it, and the selected region is copied directly from the
mapped row-stride buffer without first duplicating the full display. Capture
callbacks never grow an unbounded queue of raw Retina/DPI frames: when FFmpeg
cannot keep up, a frame is dropped and the event is sampled in the log. FFmpeg
resolution and hardware-encoder probing finish before native capture starts.

Each audio input has an independent byte-bounded queue sized to roughly 250 ms
of its native PCM format, plus a 128-chunk ceiling. Encoder attachment discards
the short startup pre-roll atomically. A dropped chunk, native device fault,
audio-pipe failure, or post-attachment handoff longer than 150 ms invalidates
and removes the segment instead of saving a recording with silent A/V drift.

Pause closes the current segment; resume starts a compatible segment; stop
merges the segments into one library asset. If a live segment loses integrity,
previously completed segments are moved out of cleanup ownership and imported
as a partial recording. FFmpeg probes and segment finalization have hard
deadlines; long merges use an output-progress watchdog so legitimate long
re-encodes are not constrained by a short total timeout. Kiri control windows
are excluded from exported frames, while an enabled click-ripple window is
intentionally included.

Kiri does not bundle FFmpeg. A recording or explicit GIF conversion resolves a
validated local copy first, otherwise downloads a version-pinned archive,
checks its SHA-256, validates the executable, and caches it. Library browsing
and thumbnail generation never trigger that download.

## Persistence boundaries

- macOS library: `~/Library/Application Support/kiri`
- Windows library: `%APPDATA%\\kiri`
- Assets are indexed by `library.json`; Trash is recoverable and never empties
  automatically.
- OCR profile metadata is stored in the app configuration directory; secrets
  never appear in that JSON, IPC responses, or logs.
- Credential replacement and deletion use a non-secret journal so interrupted
  Keychain/Credential Manager updates can be reconciled on startup.
- Video playback requires a valid single byte range and reads at most 1 MiB
  per protocol response; missing or malformed ranges are rejected instead of
  materializing an entire recording. The library mounts only near-viewport,
  640-pixel previews; image thumbnails downsample through ImageIO on macOS and
  WIC on Windows before PNG encoding. Generated thumbnails use a 32 MiB/256-
  entry LRU cache with a 15-second decoder deadline. Edited assets invalidate
  only their own browser preview, permanently deleted assets are evicted
  immediately, and a pinned image is released when its window is destroyed.

Tests must use temporary directories and fake transports. They must never read,
write, or delete the user's capture library.

## Source-of-truth order

Current source and tests win, followed by `AGENTS.md`, accepted ADRs, and this
architecture document. `README` describes user-visible behavior and the privacy
documents define network and credential promises. Completed plans remain in Git
history instead of the working tree.

## Verification

```bash
pnpm test:release-tools
pnpm build
cargo test --locked --manifest-path src-tauri/Cargo.toml --all-targets
cargo check --locked --manifest-path src-tauri/Cargo.toml --all-targets
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
git diff --check
```

Capture, recording, permission, focus, or overlay changes also require a
stable-signed packaged-app check on macOS and the corresponding Windows CI and
real-device acceptance.
