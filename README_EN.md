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

Press the shortcut, select a window or region, then capture, annotate, recognize text, or record the screen. Screenshots are copied to the clipboard automatically; screenshots, MP4 recordings, and GIFs are all saved in the local library.

Kiri is primarily developed and tested on macOS. The Windows build has not completed real-device acceptance testing, so installation, permissions, or some features may not work correctly.

## Features

- **Screenshots and re-editable annotation**: capture a window or region, then use pen, shapes, arrows, text, mosaic, undo, and redo. Annotations created by current Kiri builds stay editable on the device; click the completion thumbnail or double-click a library image to continue editing. Marks already flattened into older screenshots cannot be reconstructed, but you can start a new annotation project from the current image.
- **OCR**: recognize text locally by default, or configure an optional remote service with confirmation before every upload.
- **Recording and GIF**: record a region with optional system audio, microphone, pointer, and click highlights; export MP4 or GIF.
- **Local library**: search, favorite, tag, rename, and move captures to recoverable Trash.

> Editable screenshot projects are now on `main`; the current v1.4.4 download does not include this feature yet.

## Download and install

Download the latest version from [GitHub Releases](https://github.com/yuxino/kiri/releases/latest).

- **macOS 14+**: download the Apple Silicon (`arm64`) or Intel (`x64`) `.dmg`, open it, and drag `Kiri.app` to Applications. Capture requires **Screen & System Audio Recording** permission. **Input Monitoring** is needed only for click highlights, and **Microphone** only when microphone recording is enabled.
- **Windows**: run the installer. Screen capture does not require an extra system permission; microphone access follows Windows privacy settings. The current installer is not Authenticode-signed, so Windows SmartScreen may show a warning. The Windows version has not been tested on a real device and may fail to install or use some features.

> The currently downloadable v1.4.4 macOS GitHub releases (`arm64` and `x64`) use the project's maintained local self-signed identity, not ad-hoc signing. Both packages have been checked for version, the `io.yuxino.kiri` identifier, architecture, signature structure, and Designated Requirement. The identity is not Developer ID and the packages are not notarized, so Gatekeeper may still block the first launch. Control-click `Kiri.app` and choose **Open**, or use **System Settings → Privacy & Security → Open Anyway**. You do not need to disable Gatekeeper.
>
> Kiri's current distribution bar is a GitHub download that users can install and launch after following macOS's manual approval prompt. App Store distribution, Developer ID, and Apple notarization are not required. A maintainer packages and verifies macOS DMGs on a trusted Mac with the project's long-lived local self-signed identity, then attaches them to the Release manually. GitHub Actions verifies the version and produces the Windows draft; it does not package macOS, and that does not block a macOS release. If the signing identity changes later, macOS may ask users to grant Screen Recording, Input Monitoring, or Microphone access again.

Remote OCR is optional. API keys are stored in macOS Keychain or Windows Credential Manager, not in the profile file. Creating or selecting a remote profile sends nothing; each request requires an explicit **Send** or **Retry** action.

To keep new annotations re-editable, Kiri stores the flattened screenshot, a clean source, and the annotation document locally. The source may still contain pixels covered by mosaic or shapes. Editing never uploads it; Trash keeps it recoverable, and permanent deletion removes it with the screenshot.

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
