# Issue #21: capture shortcut acceptance

These are native Retina window screenshots, not a browser mock or a recording.
The before image is the installed v1.6.0 Settings page. The after images are
from the branch's signed macOS debug bundle installed at the canonical app
path, using the same existing local signing identity. The original installation,
Chinese language, and default shortcut were restored after testing. No captures
were saved or removed from the user's library.

- [Before: no shortcut editor](before.png)
- [After: change and restore controls](after.png)
- [Conflicting replacement preserves the current shortcut](conflict-preserved.png)

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
inspected. Automated targeted key delivery did not produce a global-hotkey
callback in either the original installation or the candidate; physical-key
capture triggering still needs acceptance.

## Automated checks and limits

260 Rust tests, cargo check, 153 frontend/release-tool tests, the frontend
build, and git diff --check passed. Strict Clippy reports the same four existing
video-export findings on both this candidate and an isolated unmodified
`6bbd307` baseline: argument count, manual range check, collapsible conditional,
and a cloned reference in a test. No unrelated video code was changed.

The second-display report in #21 does not identify an OS, version, display
layout, or trigger sequence. This Mac has one connected display; this change
does not claim to fix or reproduce that part of the issue. Windows runtime
acceptance and CI are separate from the macOS results above.
