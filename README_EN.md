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

It is a native macOS visual capture tool. Capture a region, annotate it, and
copy it while kiri quietly keeps a local, searchable copy. Your last capture
does not disappear just because the clipboard changed.

## Available today

- Start region capture with the exclusive **⌥⌘2** default or **⌃⇧2**
- Freeze the active display, snap to the frontmost window, or drag a free region
- Resize with eight handles, then double-click or press Return for inline annotation
- Distinguish pen, rectangle, arrow, text, and mosaic tools by color
- Press Return to copy immediately, or save, pin, or open the full editor
- Preview, drag, and continue from the bottom-right Quick Access panel
- Persist each completed capture once and add it to History in the background
- Search, favorite, reveal, and copy captures from the local library
- Move captures to a recoverable trash, restore them, or delete permanently
- Keep source application, dimensions, type, and creation time as metadata

> kiri is an early source preview. Recording, GIF export, and scrolling capture
> are represented in the architecture and roadmap, but are not enabled yet.

## Build from source

Requires macOS 14+ and Swift 6.

```bash
git clone https://github.com/yuxino/kiri.git
cd kiri
swift run kiri-core-tests
./scripts/package-app.sh
open dist/kiri.app
```

If a local signing certificate is available, run
`KIRI_CODESIGN_IDENTITY="Certificate Name" ./scripts/package-app.sh` to keep
the app identity stable across local builds.

macOS asks for Screen & System Audio Recording permission on the first capture.
If capture remains unavailable after approval, quit and reopen kiri.

Captures stay under `~/Library/Application Support/kiri/` and are never
uploaded automatically.

See [ROADMAP.md](ROADMAP.md), [CONTRIBUTING.md](CONTRIBUTING.md), and
[SECURITY.md](SECURITY.md).

[MIT](LICENSE) © 2026 yuxino
