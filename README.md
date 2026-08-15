<div align="center">
  <img src="src-tauri/icons/128x128.png" width="112" alt="kiri app icon">
  <h1>kiri</h1>
  <p>A fast, local-first capture workspace for macOS and Windows.</p>
  <p>
    <a href="README_ZH.md">简体中文</a>
    · <a href="README_JA.md">日本語</a>
  </p>
</div>

`kiri` comes from the Japanese word 「切り取り」—to clip or cut out.

Capture screenshots, annotate, recognize text, record regions, and keep everything in a local library. No cloud required.

## Screenshots

![Kiri library](docs/screenshots/library.png)

## Features

- **Screenshots** — window or region capture with precise selection.
- **Annotations** — pen, shapes, arrows, text, and mosaic with undo/redo; existing annotations stay selectable and editable.
- **OCR** — local text recognition (macOS Vision / Windows.Media.Ocr).
- **Recording** — region recording with optional audio, pointer, and click highlights; a 3-2-1 countdown, a draggable control bar (Esc to stop), and Retina-quality MP4 output.
- **GIF** — convert short recordings into looping GIFs.
- **Library** — date-grouped captures with favorites, tags, rename, search, copy, reveal, and a recoverable Trash. The sidebar and filter bar let you browse by type, favorites, and tags.

## Download

Download the latest build from GitHub Releases.

- **macOS**: unzip and move `Kiri.app` to Applications. Kiri needs **Input Monitoring** for the global shortcut and **Screen & System Audio Recording** for capture. Everything stays on your Mac unless you export it yourself.

> **macOS permission note**: GitHub release builds are ad-hoc signed (no Apple Developer ID available), so macOS treats each build as a new app and may re-prompt for **Screen Recording** after an upgrade — grant it once in System Settings → Privacy & Security → Screen Recording, then reopen Kiri. Locally built apps (`./scripts/install-app.sh`) use a stable certificate signature, so the grant persists across reinstalls.
- **Windows**: run the installer; no capture permissions are required.

## Build from source

Requires Rust 1.85+, Node.js 20+, and pnpm.

```bash
git clone https://github.com/yuxino/kiri.git
cd kiri
pnpm install
cargo test --manifest-path src-tauri/Cargo.toml
pnpm tauri build --no-bundle   # or ./scripts/package-app.sh for installers
```

> Running the binary produced by a plain `cargo build` shows a blank window:
> the frontend assets are only embedded when building through `pnpm tauri
> build` (or `pnpm tauri dev` for development).

macOS packaging also requires Xcode Command Line Tools.

## Shortcuts

- **⇧⌘A** (macOS) / **Shift+Ctrl+A** (Windows) — open Kiri
- **Esc** — cancel capture
- **Return** — copy capture
- **V** — select / move annotations
- **P / R / L / A / T / M** — pen / rectangle / line / arrow / text / mosaic
- **Delete** — delete selected annotation
- **Esc** (while recording) — stop
- **⌘F** (macOS) / **Ctrl+F** (Windows) — search the library
- **⌘Z / ⇧⌘Z** (macOS) / **Ctrl+Z / Shift+Ctrl+Z** (Windows) — undo / redo

See [ROADMAP.md](ROADMAP.md), [CONTRIBUTING.md](CONTRIBUTING.md), and [SECURITY.md](SECURITY.md).

[MIT](LICENSE) © 2026 yuxino
