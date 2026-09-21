<div align="center">
  <img src="src-tauri/icons/128x128.png" width="112" alt="Kiri app icon">
  <h1>Kiri</h1>
  <p>A local-first tool for screenshots, annotation, OCR, and region recording.</p>
  <p>
    <a href="https://kiri.yuxino.cn">Website</a>
    · <a href="README_ZH.md">简体中文</a>
    · <strong>English</strong>
  </p>
</div>

Kiri supports macOS, Windows, and experimental Linux. Press `⇧⌘A` on macOS or `Shift+Ctrl+A` on Windows and Linux, then select a window or region to capture, annotate, recognize text, or record. Screenshots are copied to the clipboard; screenshots, MP4 recordings, and GIFs are saved in the local library.

<!-- project-demo-v1 -->
<h2 align="center">Demo</h2>

https://github.com/user-attachments/assets/367fe955-b396-4f98-b3f2-aa5cb41b6d37

<p align="center">A complete 4K, 60 fps feature tour with English female narration and captions. Play it right here.</p>
<p align="center"><a href="https://kiri.yuxino.cn/#demo">Watch in English</a> · <a href="https://kiri.yuxino.cn/zh/#demo">观看中文版</a> · <a href="docs/demos/full-tour-4k.json">Recording details</a></p>
<!-- /project-demo-v1 -->

## Features

- **Screenshots and annotation**: click a window or drag a region, then use crop, pen, shapes, arrows, text, mosaic, undo, and redo. Annotations created by current releases can be reopened from the completion card or library.
- **OCR**: recognize text locally with macOS Vision or Windows.Media.Ocr by default; optional remote OCR asks before every upload. Text History saves successful results for searching, reading, copying, and viewing the source image. Saved screenshots also offer local text recognition.
- **Recording and GIF**: record a region with optional system audio, microphone, pointer, and click highlights. A compact ring counts down before recording; click the ring or press Esc to cancel. Save as MP4 or GIF.
- **Video editing**: open an MP4 and choose **Trim & Export** in the persistent toolbar above the video. Use the continuous output timeline to seek, drag clips to reorder, drag clip edges to trim, split at the playhead, and delete unwanted sections. Each selected clip supports 0.25–4× speed in both preview and export. The timeline height is adjustable and additional tracks scroll inside it, preserving preview space. Undo and redo cover cuts, order, speed, effects, annotation changes and timing. Add timed 1.5–4× zoom regions with adjustable smooth entry and exit, or blur, pixelated and custom-color solid privacy masks. Effects update directly in the central picture, including while positioning annotations. Add a spotlight, an aspect-preserving crop with custom background and padding, or adjustable fade in/out. Drag the zoomed image to reframe; there is one Play/Pause control for the edit. Screenshot tools—pen, rectangle, line, arrow, editable text, and pixel/blur mosaic brushes—are visible immediately as independent timed tracks, sharing saved appearance settings; the first-use color is red. Drawing selects the new mark for editing: change its color, stroke width or mosaic settings in the inspector, and drag handles to reshape it. Video mosaics support freehand, rectangle and ellipse coverage. Text has an explicit Edit text action, live color/font/background updates, and content labels on its tracks; Escape cancels only the current text edit. Selecting and dragging objects on the picture works across annotations, stickers and masks. Add local PNG/JPEG/WebP image stickers from the same toolbar, move them on the canvas and resize without stretching. Stickers are static, preserve transparency, and have their own timing and undo history. Choose a named mask style and inspect its result directly in the central picture. Tool and property areas keep a stable footprint as tracks are added. Small playback windows expand when entering the editor, within the current screen’s usable area. Effect choices explain their purpose before showing settings; finishing touches stay in a collapsible group. Drag a track’s left grip up or down to reorder it, its middle to move it in time, or its edges to change its duration. Upper overlays cover lower ones in preview and export; whole-picture adjustments have their own ordered group. Track reordering supports undo and automatic scrolling inside the timeline. Save a new library copy without changing the original. Export stays in the top-right toolbar; its menu explains each quality setting and displays its actual output dimensions. High quality keeps source dimensions; Everyday sharing and Compact file cap the long edge at 1080 and 720 pixels without upscaling. Exports retain synchronized audio; macOS preserves pitch, while Windows clip-speed changes also change audio pitch. File size depends on the source and edited duration. Windows effects currently require unrotated source video (including Kiri recordings). Playback controls stay outside the picture, with a custom seek bar, hover time hints, and keyboard support. Scrubbing pauses playback and restores its previous play state on release; choose a speed preset or enter 0.1–8×, remembered for playback only.
- **Microphone check**: with microphone recording enabled, **Check microphone** tests the system default input for five seconds with a device name and live level meter; no test audio is saved.
- **Import your own media**: use **Import media** or drop local PNG, JPEG, WebP, MP4 or MOV files into the library. Images are normalized to PNG with orientation applied; videos are copied. Original files remain unchanged. Imported images use the existing image editor; imported videos use the video editor. Import accepts up to 32 files at once, images up to 32 MB / 8192 pixels per edge within the decoder memory limit, and videos up to 8 GB.
- **Local library**: search, favorite, tag, rename, and move captures to recoverable Trash. Settings can use another local directory or external disk for the library.

## Download and install

Published macOS Universal and Windows x64 installers are listed on [GitHub Releases](https://github.com/yuxino/kiri/releases).

Starting with v1.4.9, Settings can manually check, download, and install signature-verified updates. Every step requires an explicit click; Kiri does not check in the background or install silently. v1.4.8 and older builds need one manual installation of v1.4.9 or newer from Releases before in-app updates become available.

Use **Settings → About → Check for Updates** for routine updates. Download progress shows bytes and, when available, a percentage, followed by signature verification. On Windows, **Install and Restart** closes Kiri briefly and reopens it after the passive update; you do not need to uninstall the existing NSIS version. On macOS, choose restart after installation. Running a downloaded installer manually may instead show maintenance options.

- **macOS 14+**: download the Universal `.dmg` for Apple silicon and Intel, then drag `Kiri.app` to Applications. Capture requires **Screen & System Audio Recording**; **Input Monitoring** is needed only for click highlights. Microphone recording requires macOS 15+.
- **Windows 11 (x64)**: an x64 installer is available. See the [roadmap](ROADMAP.md) for the remaining full capture-flow device acceptance. Run the `.exe` installer; screen capture needs no extra system permission, and microphone access follows Windows privacy settings. The installer is not Authenticode-signed, so SmartScreen may warn.
- **Linux (experimental)**: build from source for now (`pnpm tauri build` produces an AppImage when Linux bundle targets are enabled). Wayland stills prefer the system `grim` tool when available (recommended on Hyprland/Sway) and otherwise use the xdg-desktop-portal Screenshot dialog; recording uses ScreenCast → PipeWire with system GStreamer and never downloads FFmpeg. On Hyprland, `Shift+Ctrl+A` is registered through the compositor. Window hover outlines, local OCR, system audio, microphone, and click highlights are limited or unavailable depending on the compositor.

The Windows installer follows your system language (English, Simplified Chinese or Japanese). Installation, update and uninstall messages are localized; you can choose Kiri's interface language separately in Settings. Its half-body Kiri artwork and shared presentation are maintained in [desktop-installer](https://github.com/yuxino/desktop-installer).

macOS releases use the project's maintained local self-signed identity, not Developer ID signing or Apple notarization. If the first launch is blocked, Control-click `Kiri.app` and choose **Open**, or select **Open Anyway** in System Settings → Privacy & Security.

## Privacy

Captures, local OCR, and encoding stay on your computer by default. Remote OCR is optional; API keys stay in macOS Keychain, Windows Credential Manager, or the Linux Secret Service, and every request requires an explicit **Send** or **Retry** action.

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

- **⇧⌘A** (macOS) / **Shift+Ctrl+A** (Windows and Linux): open Kiri
- **Esc**: cancel capture; stop while recording
- **Return**: confirm a screenshot
- **C**: crop in the screenshot editor
- **⌘F** (macOS) / **Ctrl+F** (Windows): search the library
- **⌘Z / ⇧⌘Z** (macOS) / **Ctrl+Z / Shift+Ctrl+Z** (Windows): undo / redo

See [PRIVACY.md](PRIVACY.md), [ROADMAP.md](ROADMAP.md), [CONTRIBUTING.md](CONTRIBUTING.md), [SECURITY.md](SECURITY.md), and the [documentation index](docs/README.md).

[MIT](LICENSE) © 2026 yuxino
