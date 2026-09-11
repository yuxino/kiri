# ADR 0034: Linux screenshot MVP and staged recording

## Status

Accepted

## Context

Kiri ships on macOS and Windows with platform-native capture, encoding, OCR,
and credential storage. Linux users need the same local-first capture workspace,
but Wayland permissions, window enumeration, global shortcuts, and the
no-downloaded-FFmpeg media contract make a single-step full parity port unsafe.

## Decision

1. Ship Linux in stages. The first usable release freezes the display through
   **xdg-desktop-portal Screenshot**, supports region drag selection and
   annotation, clipboard-first screenshots, the local library, remote OCR via
   Secret Service, and global shortcut `Shift+Ctrl+A`.
2. Target **Wayland first** via portals. X11 sessions that expose the same
   portal share the path; Kiri does not maintain a separate X11 capture stack.
3. Window hover outlines are best-effort. When the compositor does not expose
   usable window bounds, `window_rects` may be empty and users drag a region.
4. Region recording uses portal **ScreenCast** → PipeWire frames and encodes
   with **system GStreamer** plugins already installed on the host. Kiri never
   downloads or launches an FFmpeg binary.
5. Linux recording initially omits system audio, microphone, pause/resume, and
   click highlights when those capabilities are not portable. Local on-device
   OCR remains unavailable until a later decision; remote OCR stays available.
6. Package the first Linux builds as **AppImage**. Flatpak and signed in-app
   updates for Linux remain follow-up work.

## Consequences

- `capture/linux.rs` and `platform/linux.rs` become the Linux `current`
  backends beside macOS and Windows.
- CI must install WebKitGTK, portal, PipeWire, and GStreamer development
  packages to compile on Ubuntu.
- Product docs must state Linux limitations (window hover, local OCR, audio,
  click highlights) without weakening the local-first and no-FFmpeg promises.
