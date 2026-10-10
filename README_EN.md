<div align="center">
  <img src="src-tauri/icons/128x128.png" width="112" alt="Kiri app icon">
  <h1>Kiri</h1>
  <p>Screenshots, text recognition, and screen recording. Your captures stay local.</p>
  <p>
    <a href="https://kiri.yuxino.cn">Website</a>
    · <a href="README.md">简体中文</a>
    · <strong>English</strong>
    · <a href="README_ZH_TW.md">繁體中文</a>
    · <a href="README_JA.md">日本語</a>
    · <a href="README_DE.md">Deutsch</a>
    · <a href="README_KO.md">한국어</a>
    · <a href="README_FR.md">Français</a>
  </p>
  <p>
    <a href="https://github.com/yuxino/kiri/releases/latest"><img src="https://img.shields.io/github/v/release/yuxino/kiri?style=flat&amp;logo=github&amp;logoColor=white" alt="Latest release"></a>
    <a href="https://github.com/yuxino/kiri/actions/workflows/build.yml?query=branch%3Amain"><img src="https://img.shields.io/github/actions/workflow/status/yuxino/kiri/build.yml?style=flat&amp;logo=githubactions&amp;logoColor=white&amp;branch=main&amp;event=push&amp;label=CI" alt="CI status on main"></a>
    <a href="LICENSE"><img src="https://img.shields.io/github/license/yuxino/kiri?style=flat&amp;logo=opensourceinitiative&amp;logoColor=white" alt="MIT license"></a>
  </p>
  <p>
    <a href="https://github.com/yuxino/kiri/releases/latest"><img src="https://img.shields.io/badge/macOS-14%2B-555?style=flat&amp;logo=apple&amp;logoColor=white" alt="macOS 14+"></a>
    <a href="https://github.com/yuxino/kiri/releases/latest"><img src="https://img.shields.io/badge/Windows-x64-0078D4?style=flat&amp;logo=data%3Aimage%2Fsvg%2Bxml%3Bbase64%2CPHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHZpZXdCb3g9IjAgMCAyNCAyNCI%2BPHBhdGggZmlsbD0id2hpdGUiIGQ9Ik0wIDBoMTF2MTFIMHptMTMgMGgxMXYxMUgxM3pNMCAxM2gxMXYxMUgwem0xMyAwaDExdjExSDEzeiIvPjwvc3ZnPg%3D%3D&amp;logoColor=white" alt="Windows x64"></a>
    <a href="docs/linux.md"><img src="https://img.shields.io/badge/Linux-Ubuntu%2024.04-FCC624?style=flat&amp;logo=linux&amp;logoColor=white" alt="Linux"></a>
  </p>
</div>

<p align="center">Kiri is a screenshot and screen recording app for macOS, Windows, and Linux. Annotate screenshots, copy text from images, and keep your captures on your computer.</p>

![Kiri annotation interface with an illustrated sample](docs/assets/readme-preview.png)

## Features

- Capture a window or region, crop it, add drawings, text or mosaic, and pin screenshots as floating references.
- Choose Numbered callout from the Text tool's arrow menu. Click to place a number and type its description directly on the image; Return adds a line. Drag the number and transparent description separately, and adjust size, color, and filled or outline style.
- Copy text with local OCR and read QR codes from images.
- Record MP4 with optional system audio and microphone, or make a silent GIF. Converting saved videos to GIF checks decoding first and can be cancelled before saving; macOS shows frame progress and keeps failure details visible.
- Trim and reorder clips from one video, then export a new MP4.
- Search, tag, favorite, and export local captures. Recover accidental deletions from Trash.

## Get started

1. Press `⇧⌘A` on macOS or `Shift+Ctrl+A` on Windows / Linux X11, then select a window or drag a region.
2. Choose Screenshot, Record, or OCR.
3. Press `Enter` or double-click an unmarked area inside the selection to finish a screenshot; `Esc` cancels. While annotating, switch to the Select tool first; double-clicking text still edits it. Screenshots are copied to your clipboard and saved in the library. Click the toolbar’s pin icon to save and pin in one step, or choose **Pin** on the completion card. The borderless reference can be dragged and resized proportionally from its lower-right corner. Hover to unpin or close it.

In Screenshot mode, hover to magnify the original pixels and see their coordinates and HEX color. Press `⌘C` on macOS or `Ctrl+C` to copy the color while keeping the selection. The loupe hides once annotation starts.

Use the arrow beside Text to choose Label bubble (B). Click the image to type a note, then click its dot to move the bubble to the other side of that same point. Adjust the font size and dot color, or double-click the text to edit it again.

Before annotating or recording, click the sliders button to edit the selection's width and height in pixels directly beside its edges.

Choose one of seven UI languages in **Settings → General → Language**. Your choice applies to every Kiri window and survives relaunches. On first launch, Kiri follows the system language.

Renaming a library item also renames its saved file and keeps its file type. Use **Copy File** in the context menu to paste the file into a folder; **Copy** continues to copy image pixels for chats and editors.

Select an existing annotation to change its style. Mosaic offers freehand, rectangle, and ellipse shapes, with pixel or blur effects and adjustable strength. Use Watermark (W) to type a tiled pattern directly on the image and adjust opacity, angle, and spacing. Select the tool again to continue editing existing content, including after saving.

Captures stay local. Remote OCR is optional and asks before each upload. Linux setup, MP4 audio recording, Wayland shortcuts, and platform limits are covered in the [Linux guide](docs/linux.md).

[Setup & help](docs/usage.md) · [Video editing](docs/video-editing.md) · [Report a bug](https://github.com/yuxino/kiri/issues) · [Contributing](CONTRIBUTING.md)

## Contributors

Thanks to everyone who writes code, reports issues, tries Kiri, or shares it (๑•̀ㅂ•́)و✧

Special thanks to [@kerwin2046](https://github.com/kerwin2046) for the [initial Linux support](https://github.com/yuxino/kiri/pull/20), and [@LLLin000](https://github.com/LLLin000) for the [Windows text-scaling fix](https://github.com/yuxino/kiri/pull/61).

[All contributors](https://github.com/yuxino/kiri/graphs/contributors)

## Community

Thanks to the people in [V2EX](https://www.v2ex.com/), [LINUX DO](https://linux.do/), [Appinn](https://www.appinn.com/), and [NodeLoc](https://www.nodeloc.com/) for trying Kiri, sharing feedback, and spreading the word.

[MIT](LICENSE) © 2026 yuxino
