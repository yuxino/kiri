# ADR 0026: Visible, cancellable recording countdown

- Status: Accepted
- Date: 2026-09-06
- Supersedes: Countdown presentation in ADR 0012 only

## Decision

The maintainer requested a clean coral countdown ring, legible numerals and an
explicit cancel button. Render one small centered ring, a large 3-2-1 and
Cancel Countdown with an Escape hint. No full-display tint, blur or marketing
copy. Preserve keyboard focus, reduced-motion support and localized labels.

A new countdown WebView is hidden until its UI is mounted. Its ready command
validates the pending session, applies the monitor placement and capture
exclusion, shows and focuses it. Only then, after paint, does one monotonic
three-second clock run. Cancelling stops this clock before sending IPC. Both
completion and cancellation carry the session ID; stale messages are no-ops
and may not close or discard another recording. Failures do not restart the
clock silently.

The recording control panel subscribes before retrieving its current state.
A late initial snapshot must not override an event it has already received.

## Capture behavior

Countdown and recording control windows remain excluded from capture. An
external screen recording omitting these protected windows does not prove they
were invisible on the actual desktop. Do not turn off protection merely to
make a demonstration video show them. Verify the visible UI and the exported
recording separately.

## Validation

Cover ready timing, 3-2-1, click/Escape cancellation, late callbacks, StrictMode
remounts, session identity, initial control state, and a recording starting only
once. Run packaged-app acceptance on supported desktop systems; browser UI
tests alone do not verify native focus, display affinity or multi-monitor
placement.

## Palette refinement — 2026-09-06

After reviewing the implementation, the maintainer requested black rather than
coral. The ring, numerals, stop icon, cancel label and keyboard focus indicator
now use near-black (#111111); the track, borders and interaction states use
neutral grays. Keep the small white readability disc and transparent outer
surface. Geometry, timing, session safety and native capture exclusion do not
change. Built-renderer tests verify the palette and export an optional
high-resolution recording of the actual component, with IPC isolated in the
harness rather than weakening native capture protection.
