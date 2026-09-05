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

Kiri supports macOS and Windows. Press `⇧⌘A` on macOS or `Shift+Ctrl+A` on Windows, then select a window or region to capture, annotate, recognize text, or record. Screenshots are copied to the clipboard; screenshots, MP4 recordings, and GIFs are saved in the local library.

<!-- project-demo-v1 -->
## Demo

[![kiri — Demo](docs/demos/preview.gif)](docs/demos/demo.mp4)

[Full video (MP4)](docs/demos/demo.mp4) · [About this demo](docs/demos/README.md)

Rectangles, line width, arrows, text, pen, pixel/blur mosaic, undo/redo and crop controls. **10x actions with 0.8-second result holds.** Actual frontend with sample data. No native capture, OCR or export validation.
<!-- /project-demo-v1 -->

## Features

- **Screenshots and annotation**: click a window or drag a region, then use crop, pen, shapes, arrows, text, mosaic, undo, and redo. Annotations created by current releases can be reopened from the completion card or library.
- **OCR**: recognize text locally with macOS Vision or Windows.Media.Ocr by default; optional remote OCR asks before every upload.
- **Recording and GIF**: record a region with optional system audio, microphone, pointer, and click highlights; save as MP4 or GIF.
- **Local library**: search, favorite, tag, rename, and move captures to recoverable Trash. Settings can use another local directory or external disk for the library.

## Download and install

Published builds are listed on [GitHub Releases](https://github.com/yuxino/kiri/releases); stable macOS packages and Windows candidates can have different publication status.

Starting with v1.4.9, Settings can manually check, download, and install signature-verified updates. Every step requires an explicit click; Kiri does not check in the background or install silently. v1.4.8 and older builds need one manual v1.4.9 installation from Releases before in-app updates become available.

- **macOS 14+**: download the Universal `.dmg` for Apple silicon and Intel, then drag `Kiri.app` to Applications. Capture requires **Screen & System Audio Recording**; **Input Monitoring** is needed only for click highlights. Microphone recording requires macOS 15+.
- **Windows 11 (x64)**: supported in current source. The v1.4.8 installer remains a draft candidate while native capture acceptance is completed, so it is not yet publicly released. Run the `.exe` installer; screen capture needs no extra system permission, and microphone access follows Windows privacy settings. The installer is not Authenticode-signed, so SmartScreen may warn.

macOS releases use the project's maintained local self-signed identity, not Developer ID signing or Apple notarization. If the first launch is blocked, Control-click `Kiri.app` and choose **Open**, or select **Open Anyway** in System Settings → Privacy & Security.

## Privacy

Captures, local OCR, and encoding stay on your computer by default. Remote OCR is optional; API keys stay in macOS Keychain or Windows Credential Manager, and every request requires an explicit **Send** or **Retry** action.

Re-editable screenshots keep an unannotated source locally, which may still contain pixels hidden by mosaic or shapes. Saving a crop also removes out-of-frame pixels. macOS uses AVFoundation and ImageIO for MP4 recording, merging, thumbnails, and GIF creation; Windows uses Media Foundation and system imaging components. Neither platform downloads FFmpeg, and media processing remains local.

## Run from source

Requires Rust 1.88+, Node.js 20.19+ (or 22.12+), and pnpm. macOS requires Xcode Command Line Tools; Windows requires the MSVC C++ build tools.

```bash
git clone https://github.com/yuxino/kiri.git
cd kiri
pnpm install
pnpm tauri dev
pnpm tauri build --no-bundle
```

macOS development builds also require a stable signing identity. Run and build through the Tauri commands; a plain `cargo build` executable does not contain the frontend assets.

## Shortcuts

- **⇧⌘A** (macOS) / **Shift+Ctrl+A** (Windows): open Kiri
- **Esc**: cancel capture; stop while recording
- **Return**: confirm a screenshot
- **C**: crop in the screenshot editor
- **⌘F** (macOS) / **Ctrl+F** (Windows): search the library
- **⌘Z / ⇧⌘Z** (macOS) / **Ctrl+Z / Shift+Ctrl+Z** (Windows): undo / redo

See [PRIVACY.md](PRIVACY.md), [ROADMAP.md](ROADMAP.md), [CONTRIBUTING.md](CONTRIBUTING.md), [SECURITY.md](SECURITY.md), and the [documentation index](docs/README.md).

[MIT](LICENSE) © 2026 yuxino
