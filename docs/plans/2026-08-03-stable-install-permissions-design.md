# Stable Installation and Permission Identity

## Problem

Development and QA have launched Kiri from multiple bundle paths, including
`dist/kiri.app` and temporary `work/*.app` outputs. macOS can run those copies
at the same time even though they share `io.yuxino.kiri`. Accessibility then
shows one ambiguous Kiri row while the active process may be a different copy.

## Design

- Produce one canonical bundle name: `Kiri.app`.
- Install and run the user-facing build only from `/Applications/Kiri.app`.
- Stop running Kiri bundle processes before atomically replacing the installed
  app, so an update cannot leave an older executable resident in memory.
- Sign every build with the same non-ad-hoc signing identity.
- On launch, terminate other running processes with the Kiri bundle ID.
- While running, observe application launches and terminate later duplicate
  Kiri copies as well. Request graceful termination first, then force-terminate
  only that duplicate if an older menu-bar build ignores the request.
- Run a one-second duplicate scan as a fallback because LaunchServices launch
  notifications are not reliable for multiple copies of the same bundle ID.
- Request and verify Input Monitoring with `CGPreflightListenEventAccess` and
  `CGRequestListenEventAccess` before creating the active keyboard event tap.
  Distinguish it from Accessibility in the recovery UI. Modern macOS protects
  global keyboard event taps through this privacy service even when an
  Accessibility row is already enabled.
- If tap creation still fails after Input Monitoring is granted, consult the
  Accessibility preflight only to choose the correct recovery message.

## Verification

- Build with warnings as errors and run all core tests.
- Verify the canonical bundle signature before and after installation.
- Launch `/Applications/Kiri.app` and confirm it is the only Kiri process.
- Confirm no Accessibility warning is visible.
- Trigger `⇧⌘A` with another application frontmost and verify only Kiri acts.
