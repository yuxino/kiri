# ADR 0047: Native panel parents for full-screen capture windows

- Status: Accepted
- Date: 2026-09-22
- Supplements: ADR 0029

## Evidence

On macOS, Kiri v1.6.1 freezes a secondary display successfully while another
application owns its native full-screen Space, but the capture overlay remains
outside that Space. Its WebView and accessibility tree render correctly even
though the actual display contains no overlay. A system display screenshot and
`CGWindowListCopyWindowInfo` distinguish that failure from a successful capture.
Collection behavior flags and `orderFrontRegardless` alone are insufficient for
a regular `NSWindow` in this case.

## Decision

Give each transient capture or feedback window a transparent, borderless,
nonactivating `NSPanel` parent. AppKit child-window membership brings the existing
Tao window into the full-screen Space. Keep Tao's window class, delegate, WebView,
keyboard handling and focusability intact. The parent ignores mouse input and
does not hide when another application becomes active.

Keep parents in a main-thread registry keyed by native window number. Reuse a
resident window's parent; detach its child before synchronizing the parent's
frame so AppKit cannot apply a display-position change twice. Destroying a child
removes its parent from the registry, detaches the child and closes the parent.
No activation-policy change, class replacement or new dependency is needed.

## Verification

Use the isolated `macos-virtual-display.m` fixture with its `fullscreen` option
and the actual signed Kiri installation. Test native shortcut input, system
screen visibility, selection, keyboard confirmation/cancellation, clipboard
pixels, focus restoration and repeated capture teardown. Include negative
origins and mixed Retina scaling. Check recording exports because countdown,
controls, ripple and feedback share the presentation helper.

See `docs/qa/issue-21-fullscreen/README.md` for results and limitations. This
reproduces a macOS second-display failure; it does not establish the original
reporter's operating system or prove Windows physical multi-display behavior.
