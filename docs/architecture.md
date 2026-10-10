# Kiri architecture

Status: current for the Tauri 2 application.

Kiri is a local-first desktop capture workspace for macOS, Windows, and
Linux. React renders the application windows, Rust owns capture,
persistence, credentials, network access, and platform integration, and Tauri
provides the window and IPC boundary.

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
  Universal DMG verification, and isolated Linux desktop QA.

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
| `pin-*` | Resizable flattened screenshot reference; optionally topmost |
| `toast` | Passive status feedback or an interactive completion preview |
| `confirm` | Destructive-action confirmation |

The backend owns window creation and validates commands against the expected
window and active session. Frontend code never receives credentials or an
unrestricted filesystem path.

Each active screenshot has at most one pin window. It reads the saved flattened
image through the existing local media route. Unpinning or closing that window
does not change the library asset.
The screenshot toolbar can request a direct pin through its existing capture
confirmation. After the overlay is destroyed, Rust opens the saved image's
reference window; a pin failure retains the completion card for retry (ADR 0081).

Windows dispatches confirmation, resident feedback, and library-window
presentation to one serial worker. WebView2 creation never blocks their IPC or
native event callbacks, and repeated resident-window requests preserve order.
Confirmation creation errors return to the caller. Permanent deletion and
empty-trash filesystem work use blocking workers; empty-trash chooses its
deleted IDs and invalidates their thumbnails under the same generation barrier
and library lock. `scripts/qa/confirmation-native.py` exercises real Windows
confirmation windows against generated assets in a temporary managed library.

Linux does not show the floating `control-panel` or `ripple` during recording.
The tray and explicit recording commands provide controls without relying on
portal window exclusion. Linux video viewers expose playback, GIF conversion and basic normal-speed
cut/reorder MP4 export with source audio. `video_export_linux.rs` uses accurate
GStreamer decode seeks and one continuous H.264/AAC encoder. Independent frontend
capabilities hide unsupported speed/effects/annotation tools and protect advanced
saved projects from modification; see ADR 0074.

On macOS, Show in Dock is a persisted application preference, enabled by
default. It selects regular or accessory activation policy at startup and
when changed in Settings. Library and capture activation respect that choice;
the tray and global shortcut remain available when the Dock icon is hidden.

## Language preferences

The seven dictionaries in `src/i18n/` share English keys and formatting
placeholders. Settings uses language self-names in a compact selector. The
backend stores the selected code in `language.json`; saved choices override the
system locale, including Traditional Chinese locales. A successful save
broadcasts `language-changed` to every window and refreshes native tray labels.
Windows subscribe before reading their startup preference and ignore stale
reads after a newer change. See ADR 0079.

## Capture flow

1. The native global shortcut asks Rust to start a capture session and records
   the previously focused application. Registration needs no TCC permission;
   a conflicting binding leaves Kiri running and is surfaced in Settings for
   retry or replacement. Settings can record a modified letter or digit and
   restore the default. The candidate is registered before the old binding is
   released; the native preference is atomically persisted and loaded on launch.
   Shortcut and tray requests acquire an owned scheduling permit before
   dispatch, so key repeat cannot leave a burst of stale capture starts behind
   a slow or timed-out native freeze. Windows runs the complete startup on a
   dedicated thread: desktop capture and creation of a second WebView2
   controller never occupy or re-enter Tauri's main event-loop callback.
   Linux freezes the display on a worker thread, then creates the overlay on
   the GTK main thread. X11 uses the native shortcut plugin. Wayland does not
   claim a successful XWayland grab or modify compositor bindings: users
   configure `kiri --capture` as a desktop shortcut. Supporting desktops also
   offer explicit GlobalShortcuts Portal setup in Settings, with actual returned
   bindings and a dedicated identity-registered session actor (ADR 0075).
   Command guidance remains available on every Wayland desktop. Single-instance dispatch
   routes that command to the running app without opening its library.
2. macOS freezes the active display with ScreenCaptureKit. Windows frozen
   stills use the GDI path exposed through `xcap`; Windows Graphics Capture
   remains the recording backend. Linux X11 uses `xcap` for the monitor under
   the pointer and its window bounds. Wayland checks for one connected display
   before starting, then uses compatible system `grim` or the Screenshot
   portal (GNOME). Wayland supplies no window hit-test bounds. Frozen-image
   dimensions and monitor geometry must agree; multi-display Wayland capture
   remains unavailable rather than mapping a selection to an uncertain screen.
   Capture startup is single-flight, so a repeated shortcut cannot enter a
   second native freeze. Windows gives the desktop frame eight seconds to
   arrive, then uses fast lossless PNG encoding and a direct
   `EnumWindows`/DWM collector for window hit-test bounds. The collector
   excludes Kiri's own process before querying window metadata.
   Post-processing logs a warning after thirty seconds without abandoning an
   already captured frame. Stage-specific failures remain visible, and Kiri
   will not accumulate replacement workers while the original worker is still
   active.
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
   library, tears down the session, and restores focus where the platform allows
   it. GTK owns Linux clipboard contents beyond the overlay's lifetime. X11
   can restore the original app; Wayland activation is compositor-controlled.
7. A successful import presents the persisted asset in the resident completion
   window on the originating display. The preview does not take focus; a copy
   failure is reported without discarding the saved asset, and a save failure
   never presents a preview for an asset that does not exist.

Escape cancels the active session and releases its frozen image. There is no
runtime synthetic-desktop or temporary-library mode in development or
production; deterministic capture data belongs in unit tests or an isolated
test harness.

Screenshot selections also finish on a stationary double-click inside the
selected region, away from resize handles and overlay controls. In annotation
mode, only the Select tool's unmarked canvas accepts this action; text keeps its
double-click editing behavior. Both clicks must be eligible and use the same
synchronous completion lock and confirmation pipeline as Return.

Idle Screenshot hover samples a bounded 15×15 sRGB patch from the original
decoded frozen image, before any overlay compositing. Actual image dimensions
map logical pointer coordinates to display-local physical pixels. The passive
loupe hides during gestures, annotation and other capture modes. Its Cmd/Ctrl+C
action calls `copy_capture_color`, which validates the HEX value and active
overlay owner, and keeps the session alive. OCR's `copy_text` still completes
its session. See ADR 0082.

On macOS, transient capture, countdown, recording-control, ripple, and
completion windows explicitly join other applications' full-screen Spaces.
A transparent, non-interactive native `NSPanel` parent supplies full-screen
Space membership while the existing Tao child retains its delegate, WebView,
and keyboard handling. The parent is released when that child is destroyed;
resident feedback windows reuse one parent. Before synchronizing a reused
parent’s frame, detach the child so AppKit does not move it twice. After
applying the full-screen collection behavior, the windows are reordered at
their existing high window level without activating Kiri. See ADR 0047.
In particular, a global capture shortcut pressed over a full-screen video must not switch back to
Kiri's ordinary Space merely to present the capture overlay. The overlay
becomes normally interactive when clicked. Display coordinates use the fixed
Core Graphics main-display baseline rather than the current key window's
screen. Windows retains the selected monitor's
virtual-desktop origin so a secondary-display capture is not shown on the
primary display.

## Screenshot editing flow

Capture HUD placement uses measured panel and mode-selector bounds. The
screenshot toolbar's visible rows receive pointer input while empty layout
space remains usable by the canvas. Recording and OCR panels avoid the movable
mode selector and stay inside the display; recording options scroll separately
from the fixed Start/Cancel footer. Layout changes never initiate remote OCR.
See [ADR 0087](adr/0087-capture-hud-layout.md).

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
The pending crop keeps its original document coordinates while switching to
annotation tools. Crop undo/redo remains available when returning to Crop;
Cancel crop removes only that frame. Save As preserves the unsaved library
baseline and its close warning even after exporting successfully.

The capture overlay and editor load one validated native preference for the
last-used annotation color, visual widths, text background and size, and
mosaic shape, style, strength, and diameter, plus text-watermark styling.
Changes are debounced as field-level patches, merged under the native preference
lock, and broadcast to open windows. The active tool, selection, crop,
and document content are never persisted as appearance preferences.

Text and numbered callouts share a compact toolbar picker. A callout persists
one number, optional multiline description, badge center and label rectangle;
its handles move the badge and label independently. Both parts translate when
cropped and use the common preview/export renderer. Badge style and size join
the local appearance preference; note content stays in the editable document.
Descriptions are edited directly in a transparent canvas textarea; the inspector
contains number, style, size and color controls. Like normal text editing (ADR
0060), the textarea owns its live value, composition, caret and native history.
A callout draft keeps the badge and connector visible while the textarea presents
the description. Clicking another canvas location, switching tools or exporting
commits that draft as one edit to the existing callout. Return adds a line;
Cmd/Ctrl+Return commits, and Escape cancels the draft without removing its badge.
Dragging the badge or description moves that part independently, including on
the first drag. During typing, the transparent frame's border and padding move
the description; text selection keeps native behavior. The V1 document fields
and crop/export boundary remain unchanged. See ADRs 0083, 0085 and 0089.

Screenshot label bubbles share the text mark and inline editor, with an optional
`labelDirection` field. Clicking its dot keeps that pointing location fixed and
moves the bubble to its other side. Shared document geometry drives canvas/export,
selection bounds and the accessible dot control. Edge layout fits the text on
the requested side; crops keep the intersecting body, tip or dot. See ADRs 0084
and 0089.

Capture and saved-image editing share context-sensitive property controls.
Selection reads the mark's properties without modifying preferences; explicit
changes update that selection with one history entry per gesture. Fixed-height
saved-image rows reserve complete controls for each viewport width and keep
canvas coordinates stable during selection and typing. Existing text and label
bubbles drag directly with Select, Text, or Label active; double-click or the
explicit edit action opens their text editor. Numbered descriptions share
measurement with the saved renderer, including a small rounding margin. Old
short description frames gain sufficient height when edited. See ADR 0090.
Mosaic shapes are available in both windows; document-origin pixel grids and
effect ordering keep existing same-style stronger coverage stable as strokes grow
or overlap, including a draft stroke.

The V1 document also accepts editable text-watermark marks. Shared geometry
defines single-mark rotation, primary-anchor hit testing and tiled layout.
Watermarks render after ordinary marks and reuse the native inline text editor.
Cropping translates a tiled anchor without changing its phase, including when
the anchor leaves the canvas. JavaScript and Rust bound visible tile density and
text length before accepting an edit. The existing flat-image fallback protects
older applications from silently rewriting unsupported content. New watermarks
are tiled; the tool and edit button reuse the selected or last existing watermark
and keep a live draft focused. Older single watermarks retain their stored layout
until their text or styling is changed. See ADRs 0088 and 0090.

## Managed library flow

`AppState` owns one mutex-guarded library context containing the active root,
library UUID, copy generation, availability, and loaded `AssetLibrary`.
Commands and custom protocol reads resolve metadata and files through that
same context.

Library rename publishes a non-overwriting file path, atomically updates the
index, and then cleans up the previous path. ID-keyed sidecars and thumbnails
stay in place; video content identity ignores filename changes. Rename shares
the thumbnail-generation barrier and waits for active GIF conversion. Media
serving opens a file under the library lock and streams the retained handle.
Explicit Copy File uses the native file clipboard independently of image-pixel
Copy. Windows folder actions use Shell paths/PIDLs instead of Explorer argument
parsing. See [ADR 0086](adr/0086-library-file-actions.md).

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

## QR flow

The screenshot toolbar, saved-image editor and library menu run the local
portable decoder on one background worker. Capture uses the shared, pixel-aligned
frozen crop helper. Results retain normalized corners for each physical
code, validated against its observed finder shapes and projective geometry.
The UI shows a circular marker at each code's projected center and waits for
selection before showing content, even for a single code (ADR 0063).
The original grayscale pass and three
fixed contrast passes are merged by physical position, preserving repeated
payloads and capping the combined result at 64 codes (ADR 0062).
Request UUIDs and window/capture owners
prevent canceled or superseded publication; saving also checks the library
identity/generation. Saved-image scans are scoped to the library window or the
editor whose label matches that asset UUID. Editor scans use the exact opened
revision's clean source, temporarily hiding the still-mounted draft canvas and
crop controls. The library menu opens or targets that editor. Both capture and
editor results place markers directly on the original image and show selected
content in a nearby bounded panel; loading and failure stay compact.
Closing the results returns to the selected region and preserves its annotations.
Selecting a readable code saves it automatically. Opening waits for pending saves,
then delegates a validated HTTP/HTTPS URL to the default browser with one action.
Successful opening ends the owned capture or leaves the editor open with its draft
restored; failures retain the result. No image or
payload is uploaded.

QR Favorites stores an optional searchable `qrText` field and a selected-code PNG
crop through one atomic library import. Duplicate active payloads reuse a record;
removal uses recoverable Trash. The normal capture grid excludes QR records while
the QR Favorites destination provides search and content/image reuse.
Standard square WeChat QR codes use this local path; circular Mini Program
codes are unsupported. See [ADR 0061](adr/0061-local-qr-recognition-and-favorites.md)
and [ADR 0063](adr/0063-qr-center-markers-and-supported-geometry.md).
[ADR 0064](adr/0064-direct-qr-selection-and-opening.md) records direct selection,
automatic saving, content labels and one-action browser handoff.
[ADR 0068](adr/0068-qr-markers-in-saved-image-editor.md) extends original-image
markers to the editor and library menu while preserving pending edits.

## OCR flow

Local OCR is the default and runs through macOS Vision, Windows.Media.Ocr, or
system Tesseract on Linux. Linux selects installed `eng`, `chi_sim`, and `jpn`
models from the configured or system data directory; it never downloads models.
The normal local path does not use the network. OCR crop preparation waits for
pointer release and uses that final endpoint, never an intermediate drag
frame. Explicitly switching a completed screenshot selection to OCR reuses
that crop once. Plain clicks and partial drags do not prepare or send a crop
(ADR 0028).

Remote OCR profiles contain only non-secret metadata. API keys live in macOS
Keychain, Windows Credential Manager, or the Linux Secret Service. For a remote profile, Rust prepares
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

Successful, nonempty local and remote OCR results persist as image assets with
optional `ocrText` metadata and an independent PNG snapshot (ADR 0030). A single
index commit records both, rolling back new files if persistence fails. The
capture grid excludes these records; Text History searches their corrected text and
reads them without reopening the capture overlay. Trash and library migration
use the existing asset lifecycle. Snapshots cannot enter the annotation editor.

Historic screenshot OCR is an explicitly local command from the library,
viewer, or editor. It reads bounded saved flattened PNG bytes and holds at most
one background recognition operation. A library identity/generation check
prevents a delayed result being saved into a different library. Overlay result
persistence checks the active capture owner while holding the capture lock;
canceled sessions cannot write late history. Clipboard copying from history is
separate from overlay completion and does not cancel another capture session.

## Recording and GIF flow

Platform capture produces BGRA video frames and optional PCM audio. macOS uses
ScreenCaptureKit and sends those buffers to AVAssetWriter for a 30 fps H.264
MP4 with optional AAC audio and explicit BT.709 color metadata, keeping playback
and Core Image export consistent at both small and HD dimensions. Legacy videos
without color tags still depend on platform color-space inference.
Windows uses Windows Graphics Capture plus WASAPI
through `cpal`, then sends the buffers to Media Foundation. Linux uses a portal
ScreenCast session and PipeWire frames with system GStreamer for H.264 with optional AAC
MP4. Portal consent is separate from the frozen screenshot: the user must pick
the same display, and the stream dimensions are validated before region capture.
No platform resolves, downloads, or launches an external media encoder.

The recording panel explicitly chooses the final MP4 or GIF output before
capture starts; existing saved options without this field default to MP4. GIF
output disables audio for that recording session without erasing the user's
saved MP4 audio preferences. After the MP4 staging file is finalized, Kiri
converts it locally to a looping, silent GIF at 12 fps with a 720-pixel long
edge. Windows decodes the staging MP4 with Media Foundation and encodes the GIF
inside the application; macOS decodes one scaled AVAssetImageGenerator frame at a time and writes it
with the streaming Rust GIF encoder, so decoded frames do not accumulate until
finalization. GIF sampling uses the video track range, excluding any longer audio tail.
A decode-only pass validates the requested frames before expensive GIF encoding.
Saved-video conversions can cancel checking, encoding and finalization; native work
stops between frames and incomplete temporary output is removed. Library import
has an atomic commit boundary after which cancellation is unavailable.
Saved-video conversions report measured frame progress on macOS,
then finalization and library saving; unknown progress remains indeterminate.
Operation-specific failures remain visible with retry and dismiss actions,
including repeated failures. The library subscribes before fetching an active
conversion snapshot so opening it during conversion restores the current state
without overwriting newer events (ADR 0077). There is no duration cutoff for a
recording with a positive known duration. If GIF encoding or import fails,
Kiri imports the valid MP4 staging file instead of losing the recording. The
native recording session returns to idle and restores the source application's
focus before long merge/GIF work, so background finalization does not block the
next capture. Native finalization errors fail closed and preserve a recoverable
MP4 whenever GIF conversion cannot complete.

Stopping publishes a transient `recording-save-jobs` snapshot before native
encoder shutdown. The library renders saving placeholders throughout shutdown,
merge, conversion and import; reopening the library gets the same snapshot with
`get_recording_save_jobs`. Each background worker owns a separate save job, so
returning the live recording session to idle and starting another capture do
not hide unfinished processing. Completion refreshes normal assets and pending
recording recovery before removing the placeholder. Incomplete files are never
exposed as library assets, and placeholder metadata is not persisted.

The native-to-encoder video handoff has a hard two-frame capacity. On macOS,
ScreenCaptureKit's native IOSurface queue is independently limited to three
frames. On Windows, WGC delivery is throttled to the 30 fps recording policy
when the OS supports it, and the selected region is copied directly from the
mapped row-stride buffer without first duplicating the full display. Capture
callbacks never grow an unbounded queue of raw Retina/DPI frames: when the
encoder cannot keep up, a frame is dropped and the event is sampled in the log.
Both native encoders are prepared locally before capture starts without a
network request or helper-process probe.

Each audio input has an independent byte-bounded queue sized to roughly 250 ms
of its native PCM format, plus a 128-chunk ceiling. Encoder attachment discards
the short startup pre-roll atomically. A dropped chunk, native device fault,
audio-pipe failure, or post-attachment handoff longer than 150 ms invalidates
and removes the segment instead of saving a recording with silent A/V drift.

Windows pause/resume keeps one Media Foundation session open while capture
callbacks are gated. macOS pause closes the current segment and resume starts a
compatible segment; stop merges those segments into one library asset. If a
live segment loses integrity, previously completed segments are moved out of
cleanup ownership and imported as a partial recording. AVFoundation validates
and losslessly exports macOS segments as one MP4. Kiri control windows are excluded from exported frames,
while an enabled click-ripple window is intentionally included.

Linux uses tray pause/resume/stop actions or the explicit
`--toggle-recording-pause` and `--stop-recording` commands. Its floating control
panel is suppressed because portal capture cannot exclude it reliably. Optional
system audio, microphone recording and the explicit microphone check use the
local PulseAudio-compatible service; click highlights remain unavailable.

If a valid finalized MP4 cannot be imported because the active library is
unavailable or rejects the write, Kiri moves it into a local recovery area
with a manifest. The library exposes the pending count and a retry action.
Recovery files are removed only after a durable import.

The countdown is a compact 112px black ring with a 44px numeral and no separate
cancel row (ADR 0027). Escape or a click on the ring cancels. Initial focus goes
to the surface; Tab reaches the accessible ring action. Only the small numeral surface
has a light backing; the selected display is never dimmed or blurred. The
window is placed, protected and focused after the renderer is ready, then the
three-second clock starts after paint. Countdown IPC is bound to the recording
session so late messages cannot affect a replacement or active recording.
Reduced motion keeps the digit changes and steps the ring without continuous
motion. The control panel subscribes before reading its initial state, so
loading the window cannot lose the starting state or its cancel action.

Windows uses Media Foundation plus the bundled Rust GIF encoder for MP4
recording, recovery validation, thumbnails, and MP4-to-GIF conversion. macOS
uses AVFoundation for recording, decoding and pause-segment merging, ImageIO
for thumbnails, and the streaming Rust GIF encoder. Linux WebKitGTK video playback uses a process-scoped loopback HTTP capability
with bounded streaming and validated asset IDs/ranges (ADR 0069), because its
media decoder rejects custom `kiri` URIs. Images keep the private protocol.

Linux uses system GStreamer plugins for PipeWire ScreenCast capture,
H.264 MP4 encoding, thumbnails, and GIF export. No platform downloads or
executes FFmpeg; library browsing and thumbnail generation are local and
offline.

## Signed update flow

This flow applies to macOS and installed Windows builds. Linux uses manually
downloaded replacement `.deb` packages and does not create signed updater
artifacts or expose the in-app installer. The release page is its explicit
manual update route.

Settings reads the installed version from Tauri's application metadata and
does not run a background updater. A visible **Check for Updates** action asks
Tauri's official updater plugin to read a fixed HTTPS `latest.json` manifest.
The configured endpoint and updater public key are compiled into Kiri; release
notes can supply display text but never an executable path, alternate endpoint,
or public key.

Check, download, and install are separate visible actions. Download progress
uses the plugin's actual byte events and stays indeterminate when the server
does not provide a length. The downloaded platform archive must pass minisign
verification before the UI reaches its install-ready state. On macOS,
installation returns to Settings and a further explicit action relaunches the
app. On Windows, installation launches the passive NSIS updater and exits Kiri;
the interface does not promise an in-app restart step. The fixed GitHub
Releases page is exposed only as recovery after an updater failure.

Release packaging creates a signed Universal macOS updater archive and a
signed Windows NSIS installer. `latest.json` maps both macOS architectures to
the Universal archive and selects NSIS on Windows. Builds older than the first
signed-updater release require one manual GitHub Releases installation before
this in-app path is available.

## Completion feedback

One resident `toast` window serves two distinct modes. Ordinary notices are
short-lived and ignore pointer input. Persisted screenshot, MP4, and GIF assets
use an interactive completion card with a bounded thumbnail, status detail,
and actions to continue editing an image, open video/GIF in the viewer, copy,
or move the asset to recoverable Trash. Image cards additionally offer Pin, which calls
the existing screenshot-only reference window command from the `toast` window
and dismisses the card only after success. The command remains restricted to
the library and completion windows and rejects OCR, trashed, and nonimage assets.
The library keeps a separate eye action for flat image quick preview.
Images copy as clipboard pixels; MP4 and GIF
assets copy as operating-system file items, never as a text path or a full
in-memory video payload.

The library exposes Copy for every media kind, including its context menu and
focused-card Cmd/Ctrl+C shortcut. The viewer exposes file copying in preview
mode; draft video edits must be saved before copying their output. Text fields
and text selections retain their normal copy behavior, and failed clipboard
actions display an error.

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
  default Windows library is `%APPDATA%\\kiri`; Linux uses `$XDG_DATA_HOME/kiri`
  or `~/.local/share/kiri`. Settings may move the one active
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
- Batch export opens the selected flattened image or media files from the active
  library, then copies them to a user-chosen folder without changing the index.
  Existing destination names receive numbered copies; per-file failures are
  reported and do not discard successful copies.
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
real-device acceptance. Linux CI builds and installs a `.deb`, runs native
GStreamer tests, and invokes `scripts/qa/linux-native.sh` on Xvfb with disposable
HOME/XDG directories. Screenshots of real test windows, clipboard pixels, and
library persistence are distinct from GNOME Wayland/hardware acceptance; see
[the Linux guide](linux.md).

## Video trimming and microphone checks

The editing and microphone features in this section apply to macOS and Windows.
Linux exposes video playback and GIF conversion only.

The video viewer owns an ordered list of retained source intervals and timed
normalized zoom/mask rectangles and independent annotation tracks. The screenshot
annotation canvas publishes editable marks into a shared, bounded undo history.
Navigation and export commit pending text before reading the document snapshot. A separate
video decoder extracts twelve small timeline thumbnails, while preview playback
skips removed source intervals. The central canvas always composites the edited frame. Annotation drafts publish live marks to the compositor; the transparent annotation input layer retains handles and uses the camera transform so drawing and dragging stay aligned after zoom/crop. Explicit layer order is shared by preview and both native encoders. Overlays (annotations, stickers, masks and spotlight) composite back to front, followed by whole-picture adjustments (zoom, crop/background and fade) in their own order. Older payloads without a layer keep their historical composition. Timeline grips reorder within these two stacks; the bar moves source timing across retained clips and its edges clamp to valid trim limits. macOS uses an sRGB working context to match Canvas and Windows blending. Screenshot blur brushes and live video privacy masks share a bounded CPU blur, using premultiplied pixels and clamped edges; the preview does not depend on Canvas filter support. Three box passes approximate the native Gaussian blur, so minor pixel differences between preview and encoded output are expected. `export_video_copy` accepts an asset ID only from its matching viewer.
A single-flight worker opens the readable source under the library lock,
copies a snapshot outside the lock, then uses AVFoundation or Windows MediaComposition to concatenate the retained
intervals and render a new MP4. Native
metadata validates nonoverlapping source intervals in output order (at most 128), per-clip speed from 0.25 to 4, effect
rectangles and source-time effect ranges (at most 128). macOS maps composition
time back to source time for CI effects. Timed macOS effects and annotations render
independently of source sample arrivals, at the nominal frame rate bounded to
30–120 fps; long held frames in variable-rate screen recordings still receive
their timed masks, zoom ramps and fades. Source-time mapping uses the composition's
exact rational cut boundaries, including the first frame after a fractional-speed
clip. Plain resizing preserves source frame timing. Windows splits intervals at effect
boundaries, crops zoom clips through MediaTranscoder into temporary native MP4
segments, then composes them and maps black overlays into each output viewport.
Styled masks, animated zooms, spotlight, crop/background and fade use the bounded Media Foundation frame pass instead of static crop segments; source-time transitions are sampled at the source frame rate (capped at 120 fps). Zoom ramps are clamped to half the effect duration and use the same smoothstep viewport function as the preview. Legacy payloads default to black solid masks and zero transition duration.
Rotated Windows inputs reject effects explicitly to avoid misplaced masks. Import checks the original library
identity and generation before adding a separate asset; temporary files are
removed on failure and the original asset is never overwritten. The library
receives only completed output. High quality retains source dimensions; Share
and Small cap the longest edge at 1080 and 720 pixels, without upscaling.

Changed-rate macOS audio is decoded per source track to temporary PCM and rendered
through AVAudioEngine's offline time-pitch unit before insertion at its final
timeline position. Fixed-size buffers bound memory use; original gaps and shorter
audio ranges keep their timing. Normal-speed audio retains direct insertion.
The offline engine does not connect to a microphone or physical audio output.
Preparation reports measured progress and checks cancellation while decoding and
rendering; temporary audio is removed on success, failure and cancellation.

The viewer assigns a UUID request ID to each export. Window-scoped
`video-export-progress` events report preparation, measured native rendering
progress and final saving. `cancel_video_export` accepts only a matching viewer
and request. A shared control keeps cancellation and the final import gate
mutually exclusive; the single-flight lease remains held until native work and
cleanup finish. AVFoundation sessions and Windows asynchronous operations are
cancelled at the platform boundary. Snapshot copying and frame/audio passes also
check cancellation. The final library-save phase is not cancellable.

Editable state lives separately in optional `VideoProjects/<asset UUID>.json`
files ([ADR 0045](adr/0045-local-video-projects-and-cancellable-export.md)). The
schema preserves source-time segments, effects, annotation marks, embedded PNG
stickers, the output preset and playhead. `video_project_commands` restricts
load/save to the asset's viewer and runs validation/storage off the main thread
under the library lock. Atomic writes require the current revision, which binds
the prior document, library identity/generation and source size/mtime metadata.
Corrupt documents and changed sources are protected from automatic overwrite.
The frontend uses a debounced serial save queue, includes pending text snapshots
without committing IME edits, and flushes before normal close. Projects migrate
with the library, survive Trash, and are cleaned after durable permanent deletion.

Timed annotations carry cropped PNG overlays or mosaic alpha masks with source-time
ranges and normalized geometry. Native validation limits annotations to 128, PNG
dimensions to 4096, encoded payloads to 64 MiB and decoded storage to 128 MiB.
macOS composites live-frame pixelation/blur and overlays with Core Image. Windows
uses a bounded Media Foundation frame pass preserving source timestamps, then
restores original audio before the existing trim/effect composition. Mosaic samples
the changing source frame, never a frozen editor snapshot. Playback speed is a local
viewer preference and does not change export timing.

Windows CI runs the native video renderer against isolated generated fixtures,
checks the exported duration and decoded pixels before/during/after timed effects,
and retains the input, output and frames as `windows-video-export-review` artifacts.
This complements the existing isolated countdown/recording desktop check; UTM is
not required for these checks. Hardware-specific drivers and consumer Windows
permission behavior remain outside the hosted runner's coverage.
The desktop check repeats click and Escape countdown cancellation three times
each before recording, pausing, resuming and stopping. On failure, its isolated
runner retains Kiri-specific Windows crash events and minidumps; the matching
optimized executable and debug symbols are kept separately for diagnosis.
Kiri carries Wry's upstream Windows controller-teardown fix on the 0.55 series
required by Tauri 2.11. The parent-window subclass is removed before its
controller reference is released, preventing nested messages from accessing
freed controller data. See `src-tauri/vendor/README.md` for provenance and the
condition for removing this temporary backport.

The explicit microphone check samples the system default input through cpal
for at most five seconds, sends only device name and levels to the owning
overlay, and never writes samples. It validates the capture session before and
after permission requests. Cancellation and starting a recording invalidate the
check, and closing the overlay ends it. Remembered microphone preferences do
not automatically start this test.

Clip order follows the submitted array; source intervals need not be chronological.
Source-time effects remain attached to footage while the frontend projects their
intersections into the continuous output timeline. Output duration is the sum of
`(end - start) / speed`. macOS scales composition video/audio ranges and maps filter
time back through clip speed. Windows renders source effects before its bounded
frame/audio rate conversion. Viewer playback speed is independent of clip speed.

## Local media import and video stickers

The library's import command is restricted to the library window. Native file
selection and WebView file drops share a bounded background import path. Images
use bounded Rust decoders, apply orientation and normalize to a temporary PNG;
explicit Paste Image reads desktop clipboard pixels only on request and sends
them through the same bounded PNG import path. Text inputs keep normal paste;
video files are copied to an isolated snapshot and probed through platform media
APIs. The import checks library identity/generation before publishing each asset.
Per-file failures are reported alongside successful imports without discarding
successful copies or changing the original files.

Video annotation controls remain mounted in a fixed toolbar and share the same
persisted appearance settings as screenshots. Empty preferences default to Cherry;
saved choices are not migrated. Image stickers live in the edit/undo document as
normalized local PNGs with source-time ranges and normalized geometry. They use
the existing native annotation overlay export boundary, after other annotations
and before privacy masks/zoom. Static stickers preserve alpha; corner resizing
preserves aspect ratio. Mask styles are named choices; their result is shown in the central composed
picture. The annotation inspector derives its controls from the selected mark,
with explicit live style changes and one history entry per slider gesture.
Mosaic documents optionally carry a brush, rectangle or ellipse shape; omitted
shape fields retain legacy freehand coverage. Preview and video rasterization
share the same clipping path. A stable picture surface handles selecting and
dragging between annotations, stickers and privacy effects without losing the pointer. No image is uploaded.

Editor transport and export follow [ADR 0041](adr/0041-single-canvas-video-editing.md). Export settings live in the header, playback has one control, and only the timeline track list scrolls.

## Linux audio clock and privacy boundary

Linux audio is part of the GStreamer recording encoder, not the screen portal
stream. Static capability checks inspect installed plugins without opening
audio devices. Explicit recording or microphone-check actions resolve the local
PulseAudio-compatible service, including PipeWire's Pulse service. System audio
is the default output's verified monitor; microphone capture requires the
default non-monitor input. Kiri never substitutes one for the other or changes
sound settings.

Recording starts the audio branches with the first accepted video frame. The
shared clock with signed native latency, rate conversion, bounded PCM queues and one mixed AAC
track keep memory bounded and avoid independent audio clocks. Device changes,
source stalls, discontinuities and queue overflow fail the recording instead of
producing a successful silent file. Pause closes each segment, and merge keeps
the AAC track on the same completed-segment boundaries as video. See
[ADR 0073](adr/0073-linux-recording-audio.md) for details and acceptance limits.
