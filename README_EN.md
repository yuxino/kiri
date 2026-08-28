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

Press `⇧⌘A` on macOS or `Shift+Ctrl+A` on Windows, then select a window or region to capture, annotate, recognize text, or record. Screenshots are copied to the clipboard; screenshots, MP4 recordings, and GIFs are saved in the local library.

Kiri is primarily developed and tested on macOS. Windows has not completed real-device acceptance testing, so installation or some features may fail.

## Features

- **Screenshots and re-editable annotation**: click a window or drag a region, then use pen, shapes, arrows, text, mosaic, undo, and redo. Annotations created by current `main` can be reopened from the completion card or library; marks flattened into older screenshots cannot be reconstructed.
- **OCR**: recognize text locally with macOS Vision or Windows.Media.Ocr by default; optional remote OCR asks before every upload.
- **Recording and GIF**: record a region with optional system audio, microphone, pointer, and click highlights; save as MP4 or GIF.
- **Local library**: search, favorite, tag, rename, and move captures to recoverable Trash. Settings can use another local directory or external disk for the library.

> Editable screenshot projects and library location/recovery are now on `main`; the current v1.4.4 download does not include these features yet.

If the library is offline, retry or locate it again. Choose a file to replace a missing asset. Recordings that could not be imported are kept for retry.

## Download and install

Download the latest version from [GitHub Releases](https://github.com/yuxino/kiri/releases/latest).

- **macOS 14+**: download the Apple Silicon (`arm64`) or Intel (`x64`) `.dmg`, open it, and drag `Kiri.app` to Applications. Capture requires **Screen & System Audio Recording**; **Input Monitoring** is needed only for click highlights. Microphone recording requires macOS 15+ and requests **Microphone** permission only when enabled.
- **Windows**: run the installer. Screen capture needs no extra system permission; microphone access follows Windows privacy settings. The installer is not Authenticode-signed, so SmartScreen may warn, and Windows has not completed real-device testing.

> The current v1.4.4 macOS releases use the project's maintained local self-signed identity, not ad-hoc, Developer ID signing, or Apple notarization. First launch may require Control-clicking `Kiri.app` and choosing **Open**, or selecting **Open Anyway** in System Settings → Privacy & Security. Gatekeeper does not need to be disabled.
>
> A maintainer packages the macOS DMGs on a trusted Mac and attaches them to the Release; GitHub Actions produces the Windows draft. If the signing identity changes later, macOS may request the related permissions again.

Remote OCR is optional. API keys stay in macOS Keychain or Windows Credential Manager; creating or selecting a profile sends nothing, and every request requires an explicit **Send** or **Retry** action.

Current `main` stores the flattened screenshot, clean source, and annotation document locally. The source may still contain pixels hidden by mosaic or shapes; editing never uploads it, Trash keeps it recoverable, and permanent deletion removes it with the screenshot.

Recording and GIF conversion use FFmpeg. If no usable copy is available, Kiri downloads and caches it when you first record or manually convert a GIF; browsing the library never triggers this download, and encoding stays local.

## Run from source

Requires Rust 1.88+, Node.js 20.19+ (or 22.12+), and pnpm. macOS packaging also requires Xcode Command Line Tools.

```bash
git clone https://github.com/yuxino/kiri.git
cd kiri
pnpm install
pnpm tauri dev
pnpm tauri build --no-bundle
```

macOS development builds require a stable signing identity, and `pnpm tauri dev` fails clearly when none is available. Do not run the executable produced by a plain `cargo build`; it does not contain the frontend assets.

## Shortcuts

- **⇧⌘A** (macOS) / **Shift+Ctrl+A** (Windows): open Kiri
- **Esc**: cancel capture; stop while recording
- **Return**: confirm a screenshot
- **⌘F** (macOS) / **Ctrl+F** (Windows): search the library
- **⌘Z / ⇧⌘Z** (macOS) / **Ctrl+Z / Shift+Ctrl+Z** (Windows): undo / redo

See [PRIVACY.md](PRIVACY.md), [ROADMAP.md](ROADMAP.md), [CONTRIBUTING.md](CONTRIBUTING.md), [SECURITY.md](SECURITY.md), and the [documentation index](docs/README.md).

[MIT](LICENSE) © 2026 yuxino
