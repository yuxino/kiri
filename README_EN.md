<div align="center">
  <img src="Resources/Assets/kiri-icon.png" width="112" alt="kiri">
  <h1>kiri</h1>
  <p>A quick, recoverable screenshot tool for macOS</p>
  <p>
    <strong>Early preview</strong>
    · <a href="README.md">简体中文</a>
    · <a href="README_JA.md">日本語</a>
  </p>
</div>

`kiri` comes from the Japanese word 「切り取り」—to clip or cut out.

It is a native macOS visual capture tool. Capture and copy a region in one
gesture, or annotate only when needed, while kiri keeps a local, searchable copy. Your last capture
does not disappear just because the clipboard changed.

## Available today

- Start capture with **⇧⌘A**, which Kiri filters before other apps can act on it
- Freeze the active display, snap to the frontmost window, or drag, move, and resize a free region
- See the full toolbar as soon as the region is drawn, while it remains movable and resizable until a tool is chosen
- Use pen, rectangle, line, arrow, text, and continuous brush mosaic tools with undo and redo
- Use the default pointer to select and move existing text, then double-click it to edit again
- Move pen and mosaic strokes, resize rectangles from eight handles, adjust line and arrow endpoints, or press Delete
- Continuously adjust pen width, shape width, text size, and mosaic brush size; text stays resizable while typing or after reopening it
- Choose soft, standard, or strong mosaic pixelation
- Choose from eight annotation colors; text supports complex input, long content,
  and Transparent (default), Dark, or Light backgrounds
- Press Escape to cancel at any capture stage and Return once a region exists to copy immediately
- Save, pin, or open the full editor when needed
- Record a silent region to MP4 and convert library videos up to 15 seconds to looping GIF
- Persist each completed result once and add it to local History
- Search, favorite, reveal, and copy captures from the local library
- Move captures directly from each library card to recoverable Trash, restore them, or delete permanently
- Keep source application, dimensions, type, and creation time as metadata
- Keep both a Dock icon and a menu bar shortcut while Kiri is running

> kiri is an early source preview. Silent region recording and short-video GIF
> export are available in the v0.2 preview; audio, pause, trimming, and scrolling
> capture are still in development.

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

Captures stay under `~/Library/Application Support/kiri/` and are never
uploaded automatically.

## Region recording and GIF (v0.2 preview)

1. Press **⇧⌘A** and select a region.
2. Open More Actions and choose **Record Region**.
3. Stop from Kiri's menu bar or Capture menu; the MP4 enters the library.
4. Open More Actions on a video up to 15 seconds and choose **Convert to GIF**.

The first preview records the pointer without audio. Audio, pause/resume, and
trimming will follow in later v0.2 iterations.

See [ROADMAP.md](ROADMAP.md), [CONTRIBUTING.md](CONTRIBUTING.md), and
[SECURITY.md](SECURITY.md).

[MIT](LICENSE) © 2026 yuxino
