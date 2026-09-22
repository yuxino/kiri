# Issue #21: capture shortcut acceptance

These are native Retina window screenshots, not a browser mock or a recording.
The before image is the installed v1.6.0 Settings page. The after images are
from the branch's signed macOS debug bundle installed at the canonical app
path, using the same existing local signing identity. Chinese language and the
default shortcut were restored after testing. Only agent-created screenshot,
OCR, and recording fixtures were moved to recoverable Kiri Trash; the user's
original library items were not changed.

- [Before: no shortcut editor](before.png)
- [After: change and restore controls](after.png)
- [Conflicting replacement preserves the current shortcut](conflict-preserved.png)

- [Actual 2× second-display screenshot export](secondary-retina-result.png)

## Native checks

A separate Carbon helper registered Shift+Command+A with
`kEventHotKeyExclusive`. With the upstream dependency's zero registration flags,
Kiri incorrectly reported its registration as enabled. With the one-line
exclusive-registration patch:

1. Startup displayed the occupied default and offered Change Shortcut and Retry.
2. Recording Control+Option+K changed the status to Enabled.
3. Restoring the still-occupied default showed an error and kept Control+Option+K.
4. Quitting and reopening retained Control+Option+K and registered it at startup.
5. Plain A was rejected; Escape cancelled shortcut entry.
6. After the helper exited, restoring Shift+Command+A succeeded.

English and Chinese controls were inspected in the native window. Japanese
translations have the same key set and passed the build, but were not visually
inspected. Targeted app key delivery initially bypassed global hotkeys in both
builds. A standalone native HID-event test helper subsequently triggered the
real Carbon callback: the default and custom combinations opened capture, and
the released default stopped triggering after replacement.

That system-input check also reproduced a recording-field defect: entering the
current shortcut opened capture instead of confirming the field. Settings now
holds an explicit, transient editing state; the native registration stays owned
and confirms the existing combination without capture. Blur, Escape, unmount,
and library focus loss end that state. The Windows native acceptance script
also exercises this boundary, conflicts, restart persistence, and restoration.

## System-level second-display simulation

The host has one physical Retina display (1512×982 points at 2×). The standalone
[virtual-display fixture](../../../scripts/qa/macos-virtual-display.m) adds an
OS-visible second display and a plain native source window. Kiri's production
capture path is unchanged: ScreenCaptureKit freezes the actual virtual display,
then the normal overlay, OCR, clipboard, library, and recorder run. This is not
a synthetic-capture mode inside Kiri, nor physical external-monitor acceptance.

- Installed v1.6.0: 1280×800 at 1×, positioned right `(1512, 0)`, left
  `(-1280, 0)`, and above `(0, -800)`: native shortcut, overlay bounds, region
  drag, and Escape passed.
- Candidate: right and below `(0, 982)` at 1×: screenshot completion and local
  OCR passed. OCR returned all three source lines, including `1234567890`.
- Candidate: right at 2× (2560×1600 backing): screenshot exported 1479×542
  pixels for the selected region. Custom Control+Option+K opened that display.
  MP4 recording exported 1478×542 (even encoder width), with start, pause,
  resume, and stop passing. Duration was 24.6 seconds; extracted frames at
  0.1, 12.0, and 24.2 seconds contained the source and no Kiri controls.
- Final v1.6.1 candidate: the current shortcut confirmed its recording field
  without opening capture, then triggered normally after editing ended. Left
  `(-1280, 0)` at 2× passed overlay, region drag, and Escape again.

The virtual display's `hiDPI` hint alone is insufficient: macOS can reuse a
saved 1× mode. The fixture explicitly selects a matching backing-pixel mode
and verifies `NSScreen.backingScaleFactor` before reporting success.

## Automated checks and limits

260 Rust tests, cargo check, 153 frontend/release-tool tests, the frontend
build, and git diff --check passed. Strict Clippy reports the same four existing
video-export findings on both this candidate and an isolated unmodified
`6bbd307` baseline: argument count, manual range check, collapsible conditional,
and a cloned reference in a test. No unrelated video code was changed.

The second-display report in #21 does not identify an OS, version, display
layout, or trigger sequence. It was not reproduced in the configurations
above; #21 remains open for that report. Windows runtime acceptance on the
hosted single-display desktop is separate from these macOS virtual-display
results and does not establish physical Windows multi-monitor acceptance.
