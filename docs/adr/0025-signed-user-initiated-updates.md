# ADR 0025: Signed user-initiated updates

- Status: Accepted
- Date: 2026-09-02
- Supersedes: ADR 0014

## Context

ADR 0014 deliberately stopped at a manual GitHub Releases link because Kiri did
not yet have signed updater artifacts. Kiri now has one dedicated updater key,
platform release packages, and a static release manifest. Keeping browser-only
installation would discard that trust chain and make routine updates harder.
Background checks or silent installation would conflict with Kiri's local-first,
explicit-action product contract.

## Decision

- Use the official Tauri updater and process plugins.
- Compile one fixed HTTPS `latest.json` endpoint and the updater public key into
  the application. The private key is never stored in the repository.
- Keep update checks manual. Checking, downloading, and installing each require
  a separate visible user action.
- Display the release version and release notes as inert text.
- Report download progress from actual byte events. If total size is unknown,
  show indeterminate progress rather than a fabricated percentage.
- Treat successful updater signature verification as the boundary between
  download and install-ready states. Do not provide an unsigned fallback.
- After macOS installation, relaunch only when the user presses the restart
  action. On Windows, explain that Kiri closes while the passive NSIS installer
  completes the update.
- Show the fixed GitHub Releases page only as a recovery action after failure.

Release packaging signs the Universal macOS updater archive and Windows NSIS
installer with the same updater key. The static manifest maps Apple silicon and
Intel to the Universal archive and prefers NSIS on Windows.

## Consequences

Routine updates remain explicit while gaining signature verification and
in-app progress. Existing builds that predate this updater need one manual
installation of the first signed-updater release; later compatible releases can
use the in-app flow. Rotating the updater key requires a planned transition
because already-installed builds trust the embedded public key.
