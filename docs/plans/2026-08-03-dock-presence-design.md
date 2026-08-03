# Kiri Dock Presence Design

## Goal

Kiri should behave like a normal macOS application: it appears in the Dock while
running, keeps its existing menu bar entry, and continues to use a single process.

## Design

Remove `LSUIElement` from the bundled `Info.plist`. That property opts an app into
agent mode, which intentionally hides it from the Dock. At launch, Kiri also sets
its activation policy to `.regular` so the runtime behavior is explicit and does
not depend on stale Launch Services metadata from an older installation.

The SwiftUI library window and `MenuBarExtra` remain unchanged. This keeps the
current capture workflow and menu bar access while giving users the expected Dock
presence and app-switching behavior.

## Verification

- Build with warnings treated as errors and run the complete core test suite.
- Package and reinstall the canonical app at `/Applications/Kiri.app`.
- Verify the installed plist no longer contains `LSUIElement` and the code
  signature is valid.
- Launch Kiri, confirm it is visible in the Dock, and confirm only the canonical
  Kiri process is running.
