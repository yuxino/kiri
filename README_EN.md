<div align="center">
  <img src="src-tauri/icons/128x128.png" width="112" alt="Kiri app icon">
  <h1>Kiri</h1>
  <p>A local-first tool for screenshots, annotation, OCR, and region recording.</p>
  <p>
    <a href="README.md">简体中文</a>
    · <strong>English</strong>
    · <a href="README_JA.md">日本語</a>
  </p>
</div>

Press the shortcut, select a window or region, then capture, annotate, recognize text, or record the screen. Finished captures are copied to the clipboard and saved in the local library.

Kiri is primarily developed and tested on macOS. The Windows build has not completed real-device acceptance testing, so installation, permissions, or some features may not work correctly.

## Features

- **Screenshots and annotation**: capture a window or region, then use pen, shapes, arrows, text, mosaic, undo, and redo.
- **OCR**: recognize text locally by default, or configure an optional remote service with confirmation before every upload.
- **Recording and GIF**: record a region with optional system audio, microphone, pointer, and click highlights; export MP4 or GIF.
- **Local library**: search, favorite, tag, rename, and move captures to recoverable Trash.

## Download and install

Download the latest version from [GitHub Releases](https://github.com/yuxino/kiri/releases/latest).

- **macOS 14+**: download the Apple Silicon (`arm64`) or Intel (`x64`) `.dmg`, open it, and drag `Kiri.app` to Applications. Capture requires **Screen & System Audio Recording** permission. **Input Monitoring** is needed only for click highlights, and **Microphone** only when microphone recording is enabled.
- **Windows**: run the installer. Screen capture does not require an extra system permission; microphone access follows Windows privacy settings. The Windows version has not been tested on a real device and may fail to install or use some features.

> GitHub release builds are currently ad-hoc signed because the project does not have an Apple Developer ID. macOS may request Screen Recording permission again after an upgrade. If Gatekeeper blocks the first launch, Control-click `Kiri.app` and choose **Open**, or use **System Settings → Privacy & Security → Open Anyway**. You do not need to disable Gatekeeper.

Remote OCR is optional. API keys are stored in macOS Keychain or Windows Credential Manager, not in the profile file. Creating or selecting a remote profile sends nothing; each request requires an explicit **Send** or **Retry** action.

Recording and GIF conversion use FFmpeg. If no usable copy is available, Kiri downloads it once when you first record or manually convert a GIF, then caches it locally. Browsing the library never triggers this download, and encoding stays on the device.

## Run from source

Requires Rust 1.88+, Node.js 20.19+ (or 22.12+), and pnpm. macOS packaging also requires Xcode Command Line Tools.

```bash
git clone https://github.com/yuxino/kiri.git
cd kiri
pnpm install
pnpm tauri dev
pnpm tauri build --no-bundle
```

macOS development builds require a stable signing identity. `pnpm tauri dev` uses a separate development identifier and fails clearly when no stable identity is available. Do not launch the executable produced by a plain `cargo build`; it does not contain the frontend assets and opens a blank window.

## Shortcuts

- **⇧⌘A** (macOS) / **Shift+Ctrl+A** (Windows): open Kiri
- **Esc**: cancel capture; stop while recording
- **Return**: confirm a screenshot
- **⌘F** (macOS) / **Ctrl+F** (Windows): search the library
- **⌘Z / ⇧⌘Z** (macOS) / **Ctrl+Z / Shift+Ctrl+Z** (Windows): undo / redo

See [PRIVACY.md](PRIVACY.md), [ROADMAP.md](ROADMAP.md), [CONTRIBUTING.md](CONTRIBUTING.md), [SECURITY.md](SECURITY.md), and the [documentation index](docs/README.md).

[MIT](LICENSE) © 2026 yuxino
