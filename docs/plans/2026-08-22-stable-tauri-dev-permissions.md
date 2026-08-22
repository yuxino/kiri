# Stable Tauri Development Permission Identity

## Problem

`tauri dev` launches Cargo's raw debug executable. The linker gives that file
an ad-hoc signature whose designated requirement is tied to the executable's
content hash. Every Rust rebuild therefore looks like a different application
to macOS privacy services, so Screen Recording or Input Monitoring can be
requested again even when the developer already granted access.

The in-process permission cache remains useful for duplicate calls during one
launch, but it cannot survive a development process restart.

## Decision

- Keep the standard cross-platform command: `pnpm tauri dev`.
- Keep `src-tauri` as its own Cargo workspace so an unrelated contributor
  workspace in a parent directory cannot intercept or block the dev command.
- Route the local Tauri CLI through a Node wrapper. Non-macOS platforms and all
  commands other than `dev` pass through unchanged.
- For macOS `dev`, set Cargo's host target runner. Cargo invokes that runner
  after linking and immediately before starting the debug executable.
- The runner signs only Kiri's expected Mach-O under `src-tauri/target`, using
  the dedicated identifier `io.yuxino.kiri.dev`, then verifies and launches it.
- Reuse the same stable signing certificate on every rebuild. An explicit
  `KIRI_DEV_SIGNING_IDENTITY` wins; otherwise prefer an installed Apple
  Development, Developer ID Application, or project local-development
  identity.
- Never fall back to ad-hoc signing silently. It is available only through the
  explicit `KIRI_ALLOW_ADHOC_SIGNING=1` escape hatch, with a warning that
  privacy grants will not survive rebuilds.
- Keep the development identity separate from the installed application's
  `io.yuxino.kiri` identity so development and release privacy grants are not
  accidentally shared.

## Verification

- Keep command detection, Rust host parsing, and Cargo runner environment tests
  platform-neutral so the same suite can run on every development platform.
- Sign and run two different development Mach-O builds. Their cdhash values may
  differ, but `codesign -d -r-` must report the same identifier plus certificate
  requirement.
- Confirm `pnpm tauri --version` and non-development Tauri commands still pass
  through the wrapper.
- Grant capture permissions once to the development identity, rebuild Rust,
  and confirm the next capture starts without another system prompt.
- Keep `/Applications/Kiri.app` signed as `io.yuxino.kiri`; do not reset TCC as
  part of development verification.
