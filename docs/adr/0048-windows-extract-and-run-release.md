# ADR 0048: Windows extract-and-run release

- Status: Accepted
- Date: 2026-09-26

## Context

The signed in-app Windows updater targets the NSIS installer. A ZIP containing
`kiri.exe` can run without installation on Windows 11, but using that updater
would install Kiri and turn the extract-and-run copy into an installed copy.

## Decision

Package the same release executable used for NSIS with a `kiri.portable` marker
beside it. The application checks that marker on Windows. A marked copy offers
the Releases page for manual ZIP updates and never starts the NSIS update flow.
If marker detection fails, treat the Windows copy as portable so the updater
cannot unexpectedly install it.

The ZIP is installation-free, not self-contained data storage. Kiri keeps its
existing user-profile library, settings, and Credential Manager entries. There
is no second library mode or data migration.

## Verification

Release CI verifies the extracted ZIP contains the marker and an executable
whose hash matches the release build. On a disposable Windows desktop it also
installs the NSIS package and launches both installed and extracted copies,
checking their update routes and native capture shortcut. Update UI tests cover
the portable route and preserve the installed Windows and macOS updater paths.
Passing CI is not a physical-device or SmartScreen acceptance claim.
