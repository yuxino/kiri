<div align="center">
  <img src="src-tauri/icons/128x128.png" width="112" alt="Kiri app icon">
  <h1>Kiri</h1>
  <p>Screenshots, text recognition, and screen recording. Your captures stay local.</p>
  <p>
    <a href="https://kiri.yuxino.cn">Website</a>
    · <a href="README_ZH.md">简体中文</a>
    · <strong>English</strong>
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

Kiri is a local-first capture tool for **macOS, Windows, and Linux**.

<!-- project-demo-v1 -->
## Demo

https://github.com/user-attachments/assets/367fe955-b396-4f98-b3f2-aa5cb41b6d37

<!-- /project-demo-v1 -->

## What you can do

- **Capture and annotate** a window or region. Crop, draw, add text, or apply mosaic; reopen saved screenshots to edit them. PNG exports preserve transparency; ICC-tagged image imports convert to sRGB. Keep a pending crop while annotating; canceling it keeps your marks. Save As exports a copy without saving library edits. Drag a selected line's endpoints or body to adjust it without changing tools. Pin a saved screenshot as a resizable reference above other windows.
- **Copy text** from your screen or saved images with local OCR. Correct saved recognition in Text History and restore the original text when needed.
- **Read QR codes** with the screenshot toolbar's Recognize QR Codes tool. Open an older screenshot in the editor to use the same tool later. Multiple codes keep their own positions, including repeated content and codes recovered at different contrast thresholds; overlapping local scans help separate codes in dense images. Click a center arrow directly on the original capture or editor image to view a code as a link, text or WeChat content. The library menu uses the same editor path; recognition keeps pending edits and uses compact loading or failure feedback. Readable selected codes are saved automatically in QR Favorites. Open Link uses the default browser and exits capture in one click; long content scrolls while the actions remain available. Standard square WeChat QR codes are supported; circular Mini Program codes are not.
- **Record** a region as MP4 or GIF. GIF export preserves playback timing and reports the encoded duration. On macOS and Windows, include system audio, microphone, and a visible red click highlight.
- **Edit videos** on macOS and Windows: trim, reorder, change speed, and add annotations or privacy masks.
- **Keep everything together** in a local library with imports, search, tags, favorites, and recoverable Trash. Recordings appear with a saving status while they finish processing. Copy videos and GIFs as files from the library or viewer to paste into compatible apps. Paste an image from the clipboard to annotate or recognize it, or export selected captures to a folder without moving the originals. Long titles truncate beside the card actions and show the full name on hover. Narrow windows wrap whole controls without splitting their labels.

## Download

**[Download the latest release →](https://github.com/yuxino/kiri/releases/latest)**

| Platform | Install |
| --- | --- |
| macOS 14+ · Apple silicon & Intel | Open the Universal `.dmg` and drag Kiri to Applications. |
| Windows 11 · x64 | Run the `.exe` installer, or extract the Portable ZIP and run `kiri.exe`. |
| Ubuntu 24.04 · x64 · GNOME / X11 | Download the `.deb`, then run `sudo apt install ./kiri_VERSION_amd64.deb` with its actual filename. |

macOS needs **Screen & System Audio Recording** permission; click highlights also need **Input Monitoring**, and microphone recording requires macOS 15+. The app is not Apple-notarized: if blocked, use **System Settings → Privacy & Security → Open Anyway**. Windows packages are not Authenticode-signed, so SmartScreen may warn.

**Linux:** Wayland capture supports one connected display. Recording is silent, without click highlights or video editing. See the [Linux guide](docs/linux.md) for setup and recording controls.

**Updates:** macOS and Windows installer builds use **Settings → About → Check for Updates**. Linux and Windows Portable users download a new package from Releases. Portable settings and captures stay in the Windows user profile.

**macOS Dock:** Settings → Show in Dock controls the Dock icon immediately and remembers your choice. The tray and capture shortcut remain available when it is hidden.

## Start capturing

Press **⇧⌘A** on macOS or **Shift+Ctrl+A** on Windows / Linux X11, then select a window or drag a region. On Wayland, use the Capture button or bind `kiri --capture` in desktop settings.

Choose Screenshot, Record, or OCR. The screenshot toolbar also offers Recognize QR Codes. **Enter** confirms a screenshot; **Esc** cancels capture. Screenshots go to your clipboard and local library. Screenshot tools, Cancel and Done stay on a compact bar. More Actions reveals colors and exact-size inputs; drawing tools reveal their appearance controls. Both groups stay within the active display. You can change the capture shortcut in Settings on macOS, Windows, and X11.

On macOS, if you change display layout, resolution, or scale after selecting a region, start a new capture before recording. If recording is paused, stop and save it first.

While typing an annotation, Ctrl/Cmd+Z undoes text and Shift+Enter adds a line. Esc leaves the text edit first; a second Esc cancels capture. Closing an edited saved image offers Save, Discard, or Keep editing when changes are unsaved.

On macOS, if you change display layout, resolution, or scale after selecting a region, start a new capture before recording. If recording is paused, stop and save it first.

On GNOME Wayland, if the first capture shows no permission dialog, open Kiri's Library and choose **Request Access** in the error banner. Allow screenshot access in GNOME's dialog, then retry Capture. Kiri discards the authorization image. See the [Linux guide](docs/linux.md).

## Privacy

Captures and media processing stay local. Remote OCR is optional and asks before each upload. Editable screenshots retain an original image locally, including pixels covered by annotations. Read the [privacy policy](PRIVACY.md).

## Development & docs

See [Contributing](CONTRIBUTING.md) to run from source, the [video editing guide](docs/video-editing.md), and the [documentation index](docs/README.md). [Report a bug](https://github.com/yuxino/kiri/issues) · [Roadmap](ROADMAP.md) · [Security](SECURITY.md).

## Thanks

Thanks to [@kerwin2046](https://github.com/kerwin2046) for the initial Linux support in [PR #20](https://github.com/yuxino/kiri/pull/20), and for the time and care put into it.

[MIT](LICENSE) © 2026 yuxino
