# Kiri Stable Privacy Identity Design

## Problem

macOS associates Screen Recording consent with an app's code identity, not only
its visible name or bundle identifier. Kiri previously shipped local builds
with ad-hoc signatures, then moved to a self-signed local certificate. System
Settings can therefore show an enabled Kiri entry that belongs to an older
designated requirement while the current build still fails preflight.

The packaging script also silently falls back to ad-hoc signing when no identity
is supplied, allowing the problem to return.

## Build identity

Packaging must select a stable identity in this order:

1. An explicit `KIRI_CODESIGN_IDENTITY` value.
2. An installed Apple Development certificate.
3. An installed Developer ID Application certificate.
4. The existing `mimi Local Development` certificate used by this project.

If none exists, packaging fails with an explanation. Ad-hoc signing remains an
explicit development escape hatch through `KIRI_ALLOW_ADHOC_SIGNING=1`; it is
never silent. Every package enables hardened runtime and prints the chosen
identity so a permission-breaking signature change is visible.

## One-time permission migration

After the build identity is locked, the stale Screen Recording record for only
`io.yuxino.kiri` can be reset with `tccutil`. This action requires explicit user
confirmation because it changes a privacy setting. Kiri is then restarted from
the same path and signed identity, and the user grants access once in System
Settings. No other application's privacy records are touched.

Long term, an Apple-issued Apple Development or Developer ID certificate should
replace the local certificate when one becomes available. The packaging order
will adopt it automatically, but that identity migration will itself require a
new one-time Screen Recording grant.

## Verification

- Package twice without an environment override and compare designated
  requirements.
- Confirm the package reports a non-ad-hoc identity.
- Verify the code signature and hardened runtime.
- Run all core tests and strict Debug/Release builds without launching Kiri.
