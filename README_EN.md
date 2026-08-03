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
- Confirm the region and enter inline annotation without a separate capture mode
- Use pen, rectangle, line, arrow, text, and continuous brush mosaic tools with undo and redo
- Choose small, medium, or large mosaic brushes and soft, standard, or strong pixelation
- Choose from eight annotation colors; text supports complex input, long content,
  and Transparent (default), Dark, or Light backgrounds
- Press Escape to cancel at any capture stage and Return in annotation to copy
- Save, pin, or open the full editor when needed
- Persist each completed result once and add it to local History
- Search, favorite, reveal, and copy captures from the local library
- Move captures to a recoverable trash, restore them, or delete permanently
- Keep source application, dimensions, type, and creation time as metadata
- Keep both a Dock icon and a menu bar shortcut while Kiri is running

> kiri is an early source preview. Recording, GIF export, and scrolling capture
> are represented in the architecture and roadmap, but are not enabled yet.

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

Captures stay under `~/Library/Application Support/kiri/` and are never
uploaded automatically.

See [ROADMAP.md](ROADMAP.md), [CONTRIBUTING.md](CONTRIBUTING.md), and
[SECURITY.md](SECURITY.md).

[MIT](LICENSE) © 2026 yuxino
