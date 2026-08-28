# ADR 0013: Fail-closed macOS release signing

- Status: Superseded by ADR 0017
- Date: 2026-08-26

## Supersession

ADR 0017 keeps this record's prohibition on ad-hoc signing, but replaces the
temporary deliberately failing macOS CI job. Kiri's current distribution bar
is a GitHub download that can be installed and launched after normal manual
Gatekeeper approval; App Store distribution, Developer ID, and notarization
are not required. Maintainers package macOS DMGs locally with one long-lived
self-signed identity while release CI verifies versions and produces the
Windows draft. The decision below is retained as the historical rationale for
protecting Kiri's macOS privacy identity.

## Context

Kiri's local development and installation scripts can preserve one stable
privacy identity, but the GitHub release workflow explicitly selected ad-hoc
signing. An ad-hoc designated requirement contains the exact build's `cdhash`,
so a later release is a different app to macOS even when its name and bundle
identifier are unchanged. That can repeat Screen Recording, Input Monitoring,
and Microphone authorization after every upgrade.

## Decision

- Every runnable macOS development, package, install, and release path requires
  a private-key-backed stable identity. There is no ad-hoc escape hatch for a
  real Kiri app.
- Local identity selection is deterministic: an explicit certificate wins,
  followed by one Apple Development identity, then the existing shared
  `mimi Local Development` identity. Developer ID is release material and is
  never selected automatically.
- Installation compares the complete designated requirement before replacing
  `/Applications/Kiri.app`. A certificate migration is rejected unless one
  run explicitly sets `KIRI_ALLOW_IDENTITY_CHANGE=1` and accepts one final
  authorization.
- The GitHub macOS release job fails before packaging or uploading while no
  persistent release certificate is configured. It may be re-enabled only
  after CI imports one protected certificate into an ephemeral keychain, pins
  its fingerprint in reviewed configuration, and verifies the final app's
  non-ad-hoc designated requirement before publication.
- Branch and pull-request CI compiles macOS with `--no-bundle`. It does not
  create or upload a runnable ad-hoc `.app` or `.dmg`; executable bundles are
  release artifacts and follow the stricter identity rule above.
- Developer ID plus notarization is preferred. A persistent self-signed release
  identity can preserve TCC continuity but keeps the existing first-launch
  Gatekeeper exception and has no Apple Team ID.

## Consequences

Existing ad-hoc releases may require one last authorization when they migrate
to the future stable release identity. Later releases must retain the same
certificate and `io.yuxino.kiri` identifier. Until that identity exists, Kiri
may publish Windows drafts but must not publish a new macOS package that
recreates the permission bug.
