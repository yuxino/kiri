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
- Use a Simplified Chinese or English interface that follows the preferred macOS language
- Freeze the active display, then manually drag, move, and resize an exact region with eight handles
- See the full toolbar as soon as the region is drawn, while it remains movable and resizable until a tool is chosen
- Use pen, rectangle, line, arrow, text, and continuous brush mosaic tools with undo and redo
- Use the default pointer to select and move existing text, then double-click it to edit again
- Move pen and mosaic strokes, resize rectangles from eight handles, adjust line and arrow endpoints, or press Delete
- Continuously adjust pen width, shape width, text size, and mosaic brush size; text stays resizable while typing or after reopening it
- Choose soft, standard, or strong mosaic pixelation
- Choose from eight annotation colors; text supports complex input, long content,
  and Transparent (default), Dark, or Light backgrounds
- Press Escape to cancel at any capture stage and Return once a region exists to copy immediately
- Default screenshots copy to the clipboard and return focus to the original app without opening Kiri's library
- Save, pin, or open the full editor when needed
- Choose Screenshot or Record in the capture overlay's first-level mode switch instead of finding recording inside screenshot tools
- Configure remembered countdown, system audio, microphone, pointer, and click-highlight switches before recording
- Use a centered 3-2-1 countdown that does not dim the selected region, and cancel it with Escape
- Record high-quality MP4 at the display's Retina scale instead of upscaling a low-resolution capture
- Pause, resume, or stop from a visible control bar; Kiri controls and paused time are omitted from the final video
- Show a violet click ripple live while recording and preserve the same feedback in the exported video
- Recordings save in the background and restore the original app; the library opens only when requested
- Persist each completed result once and add it to local History
- Search, favorite, reveal, and copy captures from the local library
- Move captures directly from each library card to recoverable Trash, restore them, or delete permanently
- Keep source application, dimensions, type, and creation time as metadata
- Keep both a Dock icon and a menu bar shortcut while Kiri is running

> kiri is an early source preview. Region recording with audio and pointer
> feedback plus short-video GIF export are available in the v0.2 preview;
> trimming and scrolling capture are still in development.

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
2. Choose **Record** in the first-level mode switch, then select the region.
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
