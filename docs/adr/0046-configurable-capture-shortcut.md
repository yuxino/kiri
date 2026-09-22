# ADR 0046: Configurable native capture shortcut

- Status: Accepted
- Date: 2026-09-22

## Context

Issue #21 reports that a conflicting capture shortcut cannot be changed.
Retry alone cannot resolve a combination retained by another application.

## Decision

Keep Shift+Command+A on macOS and Shift+Control+A on Windows as defaults.
Settings can record a letter or digit with Control, Alt, or Command/Windows,
optionally Shift. Escape cancels input; a separate action restores the default.
Bare typing keys and modifier-only combinations are rejected.

Continue using native registration without Input Monitoring permission. macOS
uses an exclusive Carbon registration (a one-line global-hotkey patch), matching
Windows conflict detection instead of allowing shared registrations. Register
the candidate before releasing the current binding. Reject a conflict without
changing the preference or current binding. Stage the preference in the native
config directory and atomically replace it; attempt to restore the previous
registration if saving fails. Report failures and refresh the actual status.
Load the saved binding at startup. An unavailable saved binding stays visible
for retry or replacement instead of silently selecting another combination.

## Verification

Cover validation and serialization in unit tests, and check native registration,
conflicts, restart persistence, and reset in the packaged application. Multi-display
capture remains a separate investigation and is not implied fixed by this change.
