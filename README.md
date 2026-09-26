<div align="center">
  <img src="src-tauri/icons/128x128.png" width="112" alt="Kiri app icon">
  <h1>Kiri</h1>
  <p>Screenshots, text recognition, screen recording, and video editing for macOS and Windows.</p>
  <p>
    <a href="https://kiri.yuxino.cn">Website</a>
    · <a href="README_ZH.md">简体中文</a>
    · <strong>English</strong>
  </p>
</div>

Kiri supports macOS and Windows. Press `⇧⌘A` on macOS or `Shift+Ctrl+A` on Windows, then select a window or region to capture, annotate, recognize text, or record. Screenshots are copied to the clipboard; screenshots, MP4 recordings, and GIFs are saved in the local library.

<!-- project-demo-v1 -->
<h2 align="center">Demo</h2>

https://github.com/user-attachments/assets/367fe955-b396-4f98-b3f2-aa5cb41b6d37

<p align="center">See screenshot annotation, text recognition, recording, and the library in use. A 4K demo with English narration and captions.</p>
<p align="center"><a href="https://kiri.yuxino.cn/#demo">Watch in English</a> · <a href="https://kiri.yuxino.cn/zh/#demo">观看中文版</a> · <a href="docs/demos/full-tour-4k.json">Recording details</a></p>
<!-- /project-demo-v1 -->

## Features

- **Screenshots**: select a window or region, then crop, draw, add text, or cover details with mosaic. Capture uses the display under the pointer, including another app’s native full-screen Space on macOS. Screenshots go to the clipboard and local library. Reopen new captures to edit their annotations.
- **Text recognition**: copy text from the screen or a saved screenshot. OCR runs locally by default; optional remote OCR asks before each upload. Text History lets you search results and revisit the source image.
- **Recording**: save a region as MP4 or a silent GIF, with optional system audio, microphone, pointer, and click highlights. Click the countdown ring or press Esc to cancel before recording. A five-second microphone check shows the input device and level without saving audio.
- **Video editing**: cut and reorder parts of one video, change clip speed, add timed annotations or image stickers, and hide details with privacy masks. Edits save locally so you can reopen the source and continue. Export creates a separate MP4 and can be cancelled before the final save. See the [video editing guide](docs/video-editing.md) for controls, quality settings, and platform limits.
- **Import media**: add local PNG, JPEG, WebP, MP4, or MOV files to the same editors. Imports preserve the original files. You can import up to 32 files at once; images are limited to 32 MB and 8192 pixels per edge, subject to decoder memory limits, and videos to 8 GB.
- **Local library**: find captures by name or tag, add favorites, rename files, and recover items from Trash. Move the library to another local folder or external drive in Settings.

Video projects use one source file at a time; importing several videos does not combine them. On Windows, clip-speed changes also change audio pitch, and video effects require a source without rotation metadata. Kiri recordings meet that requirement.

## Download and install

This README describes the current source. For features in a published version, check its release notes. macOS Universal and Windows x64 installers are listed on [GitHub Releases](https://github.com/yuxino/kiri/releases).

Starting with v1.4.9, Settings can manually check, download, and install signature-verified updates. Every step requires an explicit click; Kiri does not check in the background or install silently. v1.4.8 and older builds need one manual installation of v1.4.9 or newer from Releases before in-app updates become available.

Use **Settings → About → Check for Updates** for routine updates. Download progress shows bytes and, when available, a percentage, followed by signature verification. On Windows, **Install and Restart** closes Kiri briefly and reopens it after the passive update; you do not need to uninstall the existing NSIS version. On macOS, choose restart after installation. Running a downloaded installer manually may instead show maintenance options.

- **macOS 14+**: download the Universal `.dmg` for Apple silicon and Intel, then drag `Kiri.app` to Applications. Capture requires **Screen & System Audio Recording**; **Input Monitoring** is needed only for click highlights. Microphone recording requires macOS 15+.
- **Windows 11 (x64)**: an x64 installer is available. See the [roadmap](ROADMAP.md) for the remaining full capture-flow device acceptance. Run the `.exe` installer; screen capture needs no extra system permission, and microphone access follows Windows privacy settings. The installer is not Authenticode-signed, so SmartScreen may warn.

For Windows without installation, download `Kiri-<version>-Windows-x64-Portable.zip`, extract it, and run `kiri.exe`. This is an extract-and-run build: Kiri still keeps its library and settings in your Windows user profile, so moving the ZIP does not move that data. The portable build opens Releases for manual ZIP updates; its in-app NSIS installer update is unavailable.

The Windows installer follows your system language (English, Simplified Chinese or Japanese). Installation, update and uninstall messages are localized; you can choose Kiri's interface language separately in Settings. Its half-body Kiri artwork and shared presentation are maintained in [desktop-installer](https://github.com/yuxino/desktop-installer).

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

The capture shortcut can be changed or restored in **Settings → General**. Use Control, Alt, or Command with a letter or number. If another application owns the new combination, Kiri keeps the previous binding. The following capture shortcut is the default.

- **⇧⌘A** (macOS) / **Shift+Ctrl+A** (Windows): open Kiri
- **Esc**: cancel capture; stop while recording
- **Return**: confirm a screenshot
- **C**: crop in the screenshot editor
- **⌘F** (macOS) / **Ctrl+F** (Windows): search the library
- **⌘Z / ⇧⌘Z** (macOS) / **Ctrl+Z / Shift+Ctrl+Z** (Windows): undo / redo

See [PRIVACY.md](PRIVACY.md), [ROADMAP.md](ROADMAP.md), [CONTRIBUTING.md](CONTRIBUTING.md), [SECURITY.md](SECURITY.md), and the [documentation index](docs/README.md).

[MIT](LICENSE) © 2026 yuxino
