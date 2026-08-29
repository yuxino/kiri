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
- `src-tauri/src/core/library_location.rs` owns the active library marker,
  availability, and whole-library migration rules.
- `src-tauri/src/capture/` and `src-tauri/src/platform/` contain platform
  implementations.
- `scripts/` contains release checks, icon generation, stable macOS signing,
  and Universal DMG verification.

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
| `viewer-*` | Image, video, or GIF viewer |
| `toast` | Passive status feedback or an interactive completion preview |
| `confirm` | Destructive-action confirmation |

The backend owns window creation and validates commands against the expected
window and active session. Frontend code never receives credentials or an
unrestricted filesystem path.

## Capture flow

1. The native global shortcut asks Rust to start a capture session and records
   the previously focused application. Registration needs no TCC permission;
   a conflicting binding leaves Kiri running and is surfaced in Settings for
   retry.
2. macOS freezes the active display with ScreenCaptureKit. Windows uses the
   Windows Graphics Capture path exposed through `xcap`.
3. Rust keeps one reference-counted allocation for the full frozen PNG and
   shares it with the session, OCR preparation, and custom protocol. The
   overlay receives a capture-scoped, unguessable `kiri://` URL. The image is
   not written to disk.
4. The overlay performs window hit testing, region selection, and annotation in
   a fixed logical document coordinate space.
5. Screenshot confirmation stages the validated annotation document and sends
   the rendered selected PNG with an owner-bound one-time token. Rust validates
   and decodes the PNG under capture-sized allocation limits. When marks exist,
   Rust also crops a pixel-aligned clean source from the still-live frozen
   display and stores it with the document without changing `library.json`.
6. Rust copies the flattened PNG to the clipboard, imports it into the local
   library, tears down the session, and restores focus.
7. A successful import presents the persisted asset in the resident completion
   window on the originating display. The preview does not take focus; a copy
   failure is reported without discarding the saved asset, and a save failure
   never presents a preview for an asset that does not exist.

Escape cancels the active session and releases its frozen image. There is no
runtime synthetic-desktop or temporary-library mode in development or
production; deterministic capture data belongs in unit tests or an isolated
test harness.

On macOS, transient capture, countdown, recording-control, ripple, and
completion windows explicitly join other applications' full-screen Spaces.
Display coordinates use the fixed Core Graphics main-display baseline rather
than the current key window's screen. Windows retains the selected monitor's
virtual-desktop origin so a secondary-display capture is not shown on the
primary display.

## Screenshot editing flow

The flattened PNG remains the shareable asset. A marked screenshot also owns a
versioned document in `Annotations/<uuid>.json` and an immutable clean source in
`Annotations/<uuid>.source.png`. Legacy and unannotated images have no project
until their first annotated editor save.

An editor-only command loads one content-addressed snapshot. Its revision binds
the current flattened bytes, the exact presence and bytes of the document and
source files, and whether the project is absent, valid, or invalid. The editor
loads its image through `kiri://annotation-source/<uuid>?revision=<sha256>`;
the protocol returns the exact source from a newly verified matching snapshot,
not a path that can change between validation and reading. Valid projects use
the clean source and persisted marks. Missing or invalid projects use the
current flat image; invalid data produces a visible warning and is never
applied.

Save stages a bounded document with a one-time token tied to the matching
`editor-<uuid>` window. A pending crop is pixel-aligned in the WebView, while
Rust derives the replacement clean source from the exact opened revision. The
WebView translates intersecting marks into the new canvas and removes marks
fully outside it; Rust validates the resulting document. The library then
compare-and-swaps against the opened revision and updates the flat image,
project, and indexed dimensions together with best-effort rollback for ordinary
write failures. Any intervening flat, document, source, or dimension change
rejects the stale save.

Native Save As destinations are represented in the WebView by a single-use
token rather than a filesystem path. Save As writes the prepared output only;
it never mutates the library asset or editable project.

The capture overlay and editor load one validated native preference for the
last-used annotation color, visual widths, text background and size, and
mosaic style, strength, and diameter. Changes are debounced and shared across
windows through the app config directory. The active tool, selection, crop,
and document content are never persisted as appearance preferences.

## Managed library flow

`AppState` owns one mutex-guarded library context containing the active root,
library UUID, copy generation, availability, and loaded `AssetLibrary`.
Commands and custom protocol reads resolve metadata and files through that
same context.

The default root is created in the operating-system application-data location
only when no saved library exists. After that, any remembered root is accepted
only when its marker matches the saved library. If the root is unavailable, the
context stays offline; Kiri neither creates a replacement there nor silently
opens another library. Capture and recording starts are blocked until the user
retries or locates the existing library. Returning to the default location
requires the active library to be available and uses the same verified
migration path.

Changing location uses a native folder picker, copies the index, assets, and
annotation projects into a staged library, validates the result, then switches
the saved location and context. The destination receives a new copy generation,
so the unchanged source cannot later be mistaken for the current copy.

Asset availability is checked separately from media playback. The viewer has
distinct loading, missing, unreadable, and playback-failed states. Restoring a
missing asset uses a native file picker and atomically copies a validated file
back to its managed filename; removing the record is allowed only while the
file is still missing.

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

Copying recognized text completes the capture session and closes its full-screen
overlay before the global success notice is presented. The confirmation must
never remain hidden behind the OCR result surface.

## Recording and GIF flow

Platform capture produces BGRA video frames and optional PCM audio. macOS uses
ScreenCaptureKit. Windows uses Windows Graphics Capture plus WASAPI through
`cpal`. Rust feeds the actual pixel and audio formats to FFmpeg and first
produces a 30 fps H.264 MP4 with AAC audio. Hardware encoding is probed first
and falls back to `libx264`.

The recording panel explicitly chooses the final MP4 or GIF output before
capture starts; existing saved options without this field default to MP4. GIF
output disables audio for that recording session without erasing the user's
saved MP4 audio preferences. After the MP4 staging file is finalized, Kiri
converts it locally to a looping, silent GIF at 12 fps with a 720-pixel long
edge. There is no duration cutoff for a recording with a positive known
duration. If GIF encoding or import fails, Kiri imports the valid MP4 staging
file instead of losing the recording. The native recording session returns to
idle and restores the source application's focus before long merge/GIF work,
so background finalization does not block the next capture. FFmpeg jobs must
keep growing their output; a stalled child process is terminated and reaped.

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

If a valid finalized MP4 cannot be imported because the active library is
unavailable or rejects the write, Kiri moves it into a local recovery area
with a manifest. The library exposes the pending count and a retry action.
Recovery files are removed only after a durable import.

The countdown is a single background-free 3-2-1 numeral in system display
typography. It has no ring, disc, panel, hint pill, blur, or shadow, remains
centered without dimming the selected region, supports Escape cancellation and
reduced-motion preferences, and stays visually independent of the selected
output format.

Kiri does not bundle FFmpeg. A recording or explicit GIF conversion resolves a
validated local copy first, otherwise downloads a version-pinned archive,
checks its SHA-256, validates the executable, and caches it. Library browsing
and thumbnail generation never trigger that download.

## Update check flow

Settings reads the installed version from Tauri's application metadata. Kiri
does not run a background updater. A visible **Check for Updates** action asks
Rust to make one bounded request to the fixed public GitHub latest-release API,
with redirects and retries disabled, then compares the returned tag as a
semantic version. The response never supplies an executable path or an
arbitrary navigation target. When a newer version exists, a separate user
action opens Kiri's fixed Releases page in the system browser. Kiri does not
download or install application updates.

## Completion feedback

One resident `toast` window serves two distinct modes. Ordinary notices are
short-lived and ignore pointer input. Persisted screenshot, MP4, and GIF assets
use an interactive completion card with a bounded thumbnail, status detail,
and actions to continue editing an image, open video/GIF in the viewer, copy,
or move the asset to recoverable Trash. The library keeps a separate eye action
for flat image quick preview. Images copy as clipboard pixels; MP4 and GIF
assets copy as operating-system file items, never as a text path or a full
in-memory video payload.

Moving an asset to Trash collapses the preview into a compact three-second Undo
row that calls the normal library restore operation. Permanent deletion is not
exposed from completion feedback. Ready cards close automatically after eight
seconds; only an in-flight action delays either deadline, so pointer or window
focus cannot leave feedback stuck onscreen. If another completion arrives
during Undo, only the newest pending completion is shown afterward. Feedback
surfaces are flat, without a drop shadow. The window appears on the originating
display without taking focus and is protected/excluded from subsequent captures.

## Persistence boundaries

- The default macOS library is `~/Library/Application Support/kiri`; the
  default Windows library is `%APPDATA%\\kiri`. Settings may move the one active
  library to another local directory or external disk.
- The active root contains a schema/version marker, library UUID, and copy
  generation. A saved custom location must match that marker before Kiri loads
  its index; each migration changes the generation while preserving lineage.
- Assets are indexed by `library.json`; Trash is recoverable and never empties
  automatically.
- Editable screenshot state is stored only in `Annotations/`. Moving to Trash
  retains it; permanent deletion removes the sidecar and clean source together
  with the flat asset. A clean source can contain pixels covered by annotations
  in the flattened image. Saving a crop removes out-of-frame pixels from both
  the flat asset and clean source.
- Editor sources and saves are content-addressed. Hash or revision mismatch
  fails closed instead of pairing marks with changed image bytes.
- Batch asset mutations validate every identifier, publish `library.json` once,
  and update memory only after that write succeeds. Permanent deletion removes
  files only after the new index is durable.
- OCR profile metadata is stored in the app configuration directory; secrets
  never appear in that JSON, IPC responses, or logs.
- Credential replacement and deletion use a non-secret journal so interrupted
  Keychain/Credential Manager updates can be reconciled on startup.
- Completed recordings awaiting import live in a local recovery area outside
  the active library and retain only the media plus the metadata needed to
  retry the import.
- Video playback requires a valid single byte range and reads at most 1 MiB
  per protocol response; missing or malformed ranges are rejected instead of
  materializing an entire recording. The library mounts only near-viewport,
  640-pixel previews; image thumbnails downsample through ImageIO on macOS and
  WIC on Windows before PNG encoding. Generated thumbnails use a 32 MiB/256-
  entry LRU cache with a 15-second decoder deadline. Edited assets invalidate
  only their own browser preview, and permanently deleted assets are evicted
  immediately.

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
