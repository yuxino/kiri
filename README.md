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

Capture screenshots, annotate, recognize text, record regions, and keep everything in a local library. No cloud account required; optional remote OCR is always user-controlled.

## Interface

The library, recoverable Trash, filters, and settings live in one compact local
workspace. Capture overlays stay out of completed screenshots and recordings.

## Features

- **Screenshots** — window or region capture with precise selection.
- **Annotations** — pen, shapes, arrows, text, and mosaic with undo/redo; existing annotations stay selectable and editable.
- **OCR** — local text recognition by default (macOS Vision / Windows.Media.Ocr), plus multiple optional profiles for Alibaba Cloud, OpenAI, or other image-capable services that implement the OpenAI Chat Completions API. Before every remote request, Kiri shows the destination, model, and selected-image details; only an explicit Send or Retry action uploads that selected region. Failures never retry, switch providers, or fall back to another upload automatically.
- **Recording** — region recording with optional audio, pointer, and click highlights; a 3-2-1 countdown, a draggable control bar (Esc to stop), and Retina-quality MP4 output.
- **GIF** — convert short recordings into looping GIFs.
- **Library** — date-grouped captures with favorites, tags, rename, search, copy, reveal, and a recoverable Trash. The sidebar and filter bar let you browse by type, favorites, and tags.

## Download

Download the latest build from GitHub Releases.

- **macOS 14+**: download the Apple Silicon (`arm64`) or Intel (`x64`) `.dmg`, open it, then drag `Kiri.app` to Applications. Kiri needs **Screen & System Audio Recording** for capture, **Input Monitoring** only when recording with click highlights, and **Microphone** only when microphone recording is enabled. Captures stay on your Mac unless you export them or explicitly send the current OCR selection to a configured provider.

> **macOS permission note**: GitHub release builds are ad-hoc signed (no Apple Developer ID available), so macOS treats each build as a new app and may re-prompt for **Screen Recording** after an upgrade — grant it once in System Settings → Privacy & Security → Screen Recording, then reopen Kiri. Locally built apps (`./scripts/install-app.sh`) use a stable certificate signature, so the grant persists across reinstalls.
>
> On first launch, Gatekeeper may block an ad-hoc-signed build. Control-click `Kiri.app` and choose **Open**, or use System Settings → Privacy & Security → **Open Anyway**. You do not need to disable Gatekeeper.

- **Windows**: run the installer; no screen-capture permission is required. If microphone recording is enabled, access is controlled by Windows privacy settings.

Remote OCR is optional. Provider API keys are entered inside Kiri and stored in macOS Keychain or Windows Credential Manager, never in the profile JSON. Local OCR remains the initial engine. Creating or selecting a profile sends nothing; each selected image still requires an explicit Send or Retry action.

Recording and GIF conversion use FFmpeg. If a usable copy is not already available, Kiri downloads it once when you first record or explicitly convert a video to GIF, then keeps it in the operating-system cache. Browsing the library never triggers the download. The request contains no screenshot, recording, library, or account data; encoding remains local afterward.

## Build from source

Requires Rust 1.88+, Node.js 20.19+ (or 22.12+), and pnpm.

```bash
git clone https://github.com/yuxino/kiri.git
cd kiri
pnpm install
cargo test --manifest-path src-tauri/Cargo.toml
pnpm tauri dev                # development with frontend hot reload
pnpm tauri build --no-bundle   # or ./scripts/package-app.sh for signed macOS installers
```

The transparent desktop icon master is `src-tauri/icons/app-icon-source.png`.
After changing it, run `pnpm icons:generate`; both development and production
builds run `pnpm icons:verify` and reject opaque-corner PNG, ICNS, or ICO assets.

On macOS, `pnpm tauri dev` signs each rebuilt debug executable with a stable
certificate and the dedicated development identifier `io.yuxino.kiri.dev`. This keeps Screen
Recording grants—and Input Monitoring when click highlights are used—stable
across Rust rebuilds. It uses an
installed Apple Development / Developer ID certificate, or an existing local
development certificate; set `KIRI_DEV_SIGNING_IDENTITY` to choose one explicitly.
The command fails clearly when no stable identity is available instead of
silently using an ad-hoc signature that would trigger repeated permission
prompts.

> Running the binary produced by a plain `cargo build` shows a blank window:
> frontend assets are embedded by `pnpm tauri build` and served by Vite during
> `pnpm tauri dev`.

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

See [PRIVACY.md](PRIVACY.md), [ROADMAP.md](ROADMAP.md), [CONTRIBUTING.md](CONTRIBUTING.md), [SECURITY.md](SECURITY.md), and the [documentation index](docs/README.md).

[MIT](LICENSE) © 2026 yuxino
