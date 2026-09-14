# ADR 0035: Linux Wayland frozen capture and Hyprland shortcut

## Status

Accepted (amends [ADR 0034](0034-linux-screenshot-mvp.md))

## Context

ADR 0034 chose xdg-desktop-portal Screenshot for Linux frozen stills and assumed
`tauri-plugin-global-shortcut` could register `Shift+Ctrl+A` the same way as on
Windows. On Hyprland and similar wlroots sessions that path fails in practice:

1. The Screenshot portal often answers once and then hangs on later silent
   requests, which blocks cancel-and-retry and successive captures.
2. `global-hotkey` is X11-only. On Wayland the plugin may report a successful
   grab via XWayland while never receiving compositor key events.
3. GTK/`GDK_SCALE` monitor sizes frequently disagree with the physical PNG from
   the compositor, so sizing the overlay from `PNG / GDK_SCALE` leaves a
   too-small window and a selection that cannot cover the full display.

Hyprland's own screenshot guidance uses the system `grim` binary. Modern
Hyprland Lua configs reject `hyprctl keyword` and require `hyprctl eval` with
`hl.bind`.

## Decision

1. Prefer the system **`grim`** binary for Wayland frozen stills when it is on
   `PATH`. On Hyprland, capture the focused output (`grim -o <name>`). Fall
   back to the xdg-desktop-portal Screenshot path when grim is unavailable
   (GNOME/KDE, sandboxed builds). This stays within the local-first /
   no-downloaded-media-executable contract: grim is a host tool, not a
   fetched encoder.
2. Keep portal **ScreenCast** → PipeWire → system GStreamer for region
   recording unchanged from ADR 0034.
3. On Hyprland, skip the X11 global-shortcut plugin grab. Install
   `CTRL + SHIFT + A` through `hyprctl eval 'hl.bind(...)'` (legacy
   `keyword bind` remains a fallback) so the compositor writes a FIFO under
   `$XDG_RUNTIME_DIR/kiri/`. Kiri listens on that FIFO and starts capture.
   X11 sessions continue to use the existing plugin.
4. Overlay geometry prefers Hyprland monitor layout when present, then Tauri
   monitor hints, and must **cover the active display** even when the PNG
   pixel size disagrees with GTK reports. Map the PNG through
   `backing_scale = png_width / logical_width` instead of shrinking the
   overlay to `PNG / GDK_SCALE`.
5. Linux capture startup stays off the GTK main thread; overlay and other
   window creation return to the main thread. Clipboard writes use Wayland
   data-control via `arboard` when available.

## Consequences

- Linux screenshot QA should verify grim-equipped Hyprland/Sway sessions and
  portal-only sessions separately.
- Product docs must mention the optional `grim` dependency for reliable
  wlroots stills and that Hyprland registers the shortcut via the compositor.
- ADR 0034 remains the staged Linux product scope (recording limits, AppImage,
  no local OCR yet); this ADR only replaces the frozen-still and shortcut
  mechanisms where Wayland reality required it.
