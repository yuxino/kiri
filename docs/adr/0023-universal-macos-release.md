# ADR 0023: One Universal macOS release

> Media acceptance note: ADR 0024 replaced the FFmpeg path referenced below
> with AVFoundation and ImageIO; current release acceptance follows ADR 0024.

- Status: Accepted
- Date: 2026-08-29
- Supersedes: ADR 0017's separate `arm64` and `x64` DMG format
- Preserves: ADR 0017's local packaging and stable signing policy

## Context

Kiri supports both Apple silicon and Intel on macOS 14 or later. Separate DMGs
made ordinary users interpret architecture names on the GitHub asset list. An
Intel user who opens the Apple silicon DMG sees Finder mark the app as
unsupported even though a compatible Intel build exists.

The application bundle contains one native executable and architecture-neutral
resources. Tauri can build a Universal binary from the already-supported
`aarch64-apple-darwin` and `x86_64-apple-darwin` targets.

## Decision

- New releases provide one Universal macOS DMG for both Apple silicon and
  Intel. Users do not choose an architecture.
- The maintainer builds it locally with `pnpm package:macos-release`. The helper
  requires both Rust targets and the same long-lived signing identity required
  by every other runnable Kiri package.
- The helper verifies the DMG, Applications link, bundle identifier, version,
  minimum macOS version, both Mach-O slices, strict signature, and a non-ad-hoc
  designated requirement before the artifact can be published.
- GitHub Actions continues to compile platform targets and produce the Windows
  draft. It does not receive the maintainer's macOS private key or publish an
  ad-hoc substitute.
- Architecture-specific DMGs from older releases remain historical artifacts;
  they are not the default download format for new releases.

## Consequences

The macOS download is larger than either former single-architecture DMG, but
there is one obvious file and one installation path. Apple silicon provides
native acceptance evidence; the x86_64 slice still requires Intel hardware for
complete capture, permission, recording, and FFmpeg acceptance.
