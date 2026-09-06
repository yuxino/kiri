# ADR 0029: Preserve the frontmost macOS full-screen Space for shortcut capture

- Status: Accepted
- Date: 2026-09-07

## Problem

Kiri already marked capture windows as eligible for other applications' native
full-screen Spaces. The global shortcut nevertheless focused the new WebView
and then activated Kiri. When a browser or player owned a full-screen Space,
that activation could switch away from the content before the overlay became
usable. A window created before its collection behavior was updated could also
remain behind the full-screen owner.

## Decision

For macOS transient capture windows, apply `CanJoinAllSpaces`,
`CanJoinAllApplications`, and `FullScreenAuxiliary`, keep the existing high
window level, and call AppKit `orderFrontRegardless` after applying that policy.
The call is deliberately non-activating. Do not call Tauri `set_focus` or
activate Kiri during macOS capture-overlay creation. The user's click on the
overlay establishes normal interaction focus. Windows keeps its existing
explicit focus/activation path.

Countdown, recording controls, completion feedback and click ripple use the
same non-activating ordering rule so they remain visible in the captured
application's full-screen Space without stealing that Space.

## Verification

Rust policy tests require the full-screen and cross-application collection
flags. macOS CI compiles both architectures and runs the platform tests.
Windows CI must remain unchanged. A final native acceptance check should press
the global shortcut while a video/browser is in macOS native full screen and
confirm that the overlay appears on that same Space; CI cannot prove Mission
Control/Space switching behavior.
