# ADR 0017: Maintainer-packaged macOS releases

- Status: Accepted
- Date: 2026-08-29
- Supersedes: The deliberately failing macOS CI job in ADR 0013
- Preserves: ADR 0013's prohibition on ad-hoc signing

## Context

Kiri is distributed directly from GitHub. Its current acceptance bar is that
ordinary users and developers can download, manually approve when macOS asks,
install, and launch the app. App Store distribution, Developer ID signing, and
Apple notarization are useful stronger options, but are not requirements for
this project.

The macOS privacy identity still matters. Replacing a long-lived signing
identity with ad-hoc signing, or unexpectedly changing the identity, can make
macOS request Screen Recording, Input Monitoring, and Microphone authorization
again. GitHub Actions does not have the maintainer's local signing private key,
and that private key must not be exported merely to make CI package a DMG.

The temporary policy job in the tag workflow made every otherwise successful
release run red. That correctly blocked CI macOS packaging, but incorrectly
represented the whole release as failed even though macOS packaging is a
separate maintainer-run step.

## Decision

- The supported distribution level is a developer-installable GitHub release.
  Users may need to Control-click **Open** or use **System Settings → Privacy &
  Security → Open Anyway**. Disabling Gatekeeper is never required.
- Runnable macOS development, packaging, and installation continue to require
  a private-key-backed, long-lived signing identity. Ad-hoc signing remains
  prohibited for real Kiri bundles.
- A maintainer packages and verifies `arm64` and `x64` DMGs on a trusted Mac
  using the project's maintained local self-signed identity, then attaches the
  verified artifacts to the GitHub Release manually.
- Release CI verifies the tagged source version and lets Windows produce the
  draft release. It does not run a macOS packaging job and does not fail merely
  because CI lacks the maintainer's macOS signing credentials.
- CI must not substitute an ad-hoc macOS package. Developer ID and notarization
  may be adopted later, but they are not prerequisites for publication.
- Release notes and installation documentation disclose the manual Gatekeeper
  path. If the signing identity changes, users are warned that macOS may ask
  them to grant privacy permissions again.

## Consequences

A healthy tag workflow now means that version validation and the Windows draft
completed; it does not claim that CI produced a macOS artifact. macOS release
evidence comes from the maintainer's local package verification and the files
attached to the draft. Historical failed workflow runs remain unchanged.

The stable local identity improves upgrade continuity but is still self-signed,
has no Apple Team ID, and is not notarized. Gatekeeper manual approval remains
an expected installation step at this distribution level.
