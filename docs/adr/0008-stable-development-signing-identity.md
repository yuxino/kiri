# ADR 0008: Stable development signing identity

- Status: Accepted
- Date: 2026-08-22

## Context

`tauri dev` normally runs Cargo's debug executable directly. A linker-signed,
ad-hoc Mach-O has a code requirement tied to that build, so macOS can treat each
Rust rebuild as a new application and ask for Screen Recording or Input
Monitoring permission again.

An in-process permission cache prevents duplicate requests during one launch,
but it cannot preserve the application's privacy identity across rebuilds.

## Decision

- Keep `pnpm tauri dev` as the cross-platform development command.
- Keep `src-tauri` as a self-contained Cargo workspace.
- On macOS development builds, a Node wrapper installs a Cargo target runner.
  Cargo invokes the runner after linking and before launching Kiri.
- The runner signs only the expected Kiri Mach-O under `src-tauri/target`, uses
  the dedicated identifier `io.yuxino.kiri.dev`, verifies the signature, and
  then launches it.
- `KIRI_DEV_SIGNING_IDENTITY` may select a stable certificate. Otherwise the
  runner chooses an installed Apple Development, Developer ID Application, or
  local-development identity.
- Ad-hoc signing is rejected for the real development app. ADR 0013 extends
  the same fail-closed rule to packaging, installation, branch artifacts, and
  releases.
- Development and installed applications keep separate identifiers so their
  privacy grants are not accidentally shared.

## Consequences

Rust hot rebuilds retain one stable macOS privacy identity when a stable
certificate is available. The runner must remain narrowly scoped so it cannot
be used to sign an arbitrary executable. Cross-platform wrapper and runner
behavior stays covered by `scripts/tauri-cli.test.mjs`.
