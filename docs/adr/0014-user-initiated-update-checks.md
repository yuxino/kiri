# ADR 0014: User-initiated update checks

- Status: Accepted
- Date: 2026-08-26

## Context

Kiri previously exposed no installed-version or update information. A full
Tauri updater would require signed updater artifacts and platform release
identity work that the current release pipeline does not yet provide. Adding a
background release check would also create a new automatic network behavior in
a local-first application.

## Decision

- Settings shows the installed application version in a low-frequency About
  section.
- Kiri contacts the fixed public GitHub latest-release API only after the user
  presses **Check for Updates**.
- The request has short connection and request timeouts, no redirects, no
  retries, and a 64 KiB response limit. Rust parses only the release tag and
  compares it with the installed semantic version.
- An available update changes the visible action to **View Update**. That
  separate action opens Kiri's fixed Releases page in the system browser.
- Kiri does not accept a navigation URL from the response and does not download
  or install application updates.

## Consequences

Users can verify whether their installed build is current without creating
background traffic or weakening the release trust chain. Installation remains
manual until signed updater artifacts and stable platform release identities
are designed and shipped together.
