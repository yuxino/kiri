<div align="center">
  <img src="Resources/Assets/kiri-icon.png" width="112" alt="kiri app icon">
  <h1>kiri</h1>
  <p>A quick, recoverable screenshot tool for macOS</p>
  <p>
    <strong>Early preview</strong>
    · <a href="README.md">简体中文</a>
    · <a href="README_JA.md">日本語</a>
  </p>
</div>

`kiri` comes from the Japanese word 「切り取り」—to clip or cut out.

The refreshed icon features a colorful chibi girl with violet-blue hair, a star clip, and a subtle capture frame. Its lavender, sky-blue, and peach palette is shared by the GitHub README and the macOS app.

It is a native macOS visual capture tool. Capture and copy a region in one
gesture, or annotate only when needed, while kiri keeps a local, searchable copy. Your last capture
does not disappear just because the clipboard changed.

## Three steps

1. Press **⇧⌘A** and choose **Screenshot, Record, or OCR** at the bottom.
2. Click a violet-outlined window or drag an exact region. OCR always requires a manual drag.
3. Press Return to copy a screenshot, copy reviewed text, or start recording from the setup card.

<p align="center">
  <img src="Resources/Assets/kiri-library-preview.png" width="820" alt="Kiri library and first-capture screen">
</p>

## Core capabilities

### Screenshot and annotate

- Freeze the display, select a window with one restrained outline, or drag, move, and resize an exact region.
- Re-select and edit pen, rectangle, line, arrow, text, and continuous mosaic annotations with undo and redo.
- Adjust widths, font size, brush diameter, and mosaic strength live; text supports complex input and transparent, dark, or light backgrounds.
- Copy and return to the original app by default, or save, pin, and continue in the full editor.

### OCR

- Recognize only a manually dragged region; clicking a window never scans the full display.
- Moving or resizing the region re-runs recognition, and the result remains editable before copying.
- Process screenshots and text locally with macOS Vision without uploads.

### Region recording

- Record Retina-scale MP4 with optional countdown, system audio, microphone, pointer, and click highlight.
- Pause, resume, and stop from a floating control bar that is excluded from the exported video.
- Save in the background and convert recordings up to 15 seconds into looping GIFs.

### Local library

- Keep source app, dimensions, media type, and creation time for images, videos, and GIFs.
- Search, favorite, copy, open, reveal in Finder, and use recoverable Trash.
- Follow the preferred macOS language in English or Simplified Chinese, with both Dock and menu bar entry points.

> kiri is an early source preview. Region recording with audio and pointer
> feedback and short-video GIF export are available in preview.

## Download

Download `Kiri-v0.1.0-macos.zip` from
[GitHub Releases](https://github.com/yuxino/kiri/releases/latest), unzip it, and
move `Kiri.app` to Applications.

v0.1.0 is an early preview that has not been notarized by Apple. On first launch,
you may need to Control-click Kiri in Finder, choose Open, and confirm. Only
download builds from this repository's Releases page.

## Build from source

Requires macOS 14+ and Swift 6.

```bash
git clone https://github.com/yuxino/kiri.git
cd kiri
swift run kiri-core-tests
./scripts/install-app.sh
open /Applications/Kiri.app
```

The installer always produces `Kiri.app` at `/Applications/Kiri.app`. Run only
that installed copy. It closes running Kiri copies before an update, and the
installed app automatically closes older copies with the same
bundle ID so macOS permissions stay attached to one stable application path.

The underlying packaging script prefers Apple Development, Developer ID, or Kiri's stable
local certificate and refuses silent ad-hoc signing that would break Screen
Recording consent. Use `KIRI_CODESIGN_IDENTITY="Certificate Name"` to select a
certificate explicitly. Set `KIRI_ALLOW_ADHOC_SIGNING=1` only for disposable
builds that do not need persistent permission.

macOS asks for Input Monitoring permission for the exclusive shortcut and
Screen & System Audio Recording permission on the first capture.
If capture remains unavailable after approval, quit and reopen kiri. Kiri calls
the system permission request at most once per launch; later attempts show an
Open Settings or Quit Kiri recovery action instead of prompting repeatedly.

## Capture shortcuts

- **⇧⌘A**: Start Kiri capture system-wide and reserve the shortcut exclusively
- **Escape**: Cancel from region selection, annotation, or text entry
- **Return**: Commit active text and copy the capture
- **V**: Select, move, or reshape existing annotations; double-click text to edit it again
- **P / R / L / A / T / M**: Pen, rectangle, line, arrow, text, and mosaic
- **Delete**: Remove the annotation selected with the pointer tool
- **⌘Z / ⇧⌘Z**: Undo / redo

## OCR

1. Press **⇧⌘A** and choose **OCR** in the bottom mode switch.
2. Manually drag the region to recognize. Kiri does not substitute a window click or scan the whole display.
3. Review or edit the result, then copy it. Moving or resizing the region runs recognition again.

Captures stay under `~/Library/Application Support/kiri/` and are never
uploaded automatically.

## Region recording and GIF (v0.2 preview)

1. Press **⇧⌘A** and choose **Record** in the first-level mode switch.
2. Click a window or drag the recording region, then adjust it if needed.
3. Set countdown, system audio, microphone, pointer, and click highlights in
   the setup card that appears automatically.
4. Choose **Start Recording**; press Escape during 3-2-1 to cancel.
5. Pause, resume, or stop from the floating control bar, Kiri menu, or Capture menu.
6. The MP4 enters the library; videos up to 15 seconds can use **Convert to GIF**.

Audio is off by default and Kiri asks for microphone permission only when that
switch is enabled. Kiri draws click highlights itself; microphone capture
requires macOS 15 or later.

See [ROADMAP.md](ROADMAP.md), [CONTRIBUTING.md](CONTRIBUTING.md), and
[SECURITY.md](SECURITY.md).

[MIT](LICENSE) © 2026 yuxino
