# ADR 0009: Operation-local transient feedback

- Status: Accepted
- Date: 2026-08-25

## Context

Kiri completes some work after its initiating window has disappeared or focus
has returned to another application. Screenshot and recording feedback must
therefore remain visible outside the library. Library actions, however, also
had an in-window notice path. Emitting both paths for one completion could put
duplicate feedback far apart, and the resident global toast stayed at the
display where it was first created.

GIF conversion also starts on a background thread. Closing its menu without a
persistent working state made the action appear unresponsive, while clicks in
the portaled menu could bubble to the capture card and accidentally enter batch
selection.

## Decision

- Global completion feedback appears near the top-center of the work area on
  the display where the operation originated.
- The resident toast window is repositioned before every presentation. It
  follows the focused Kiri window by default; screenshot and recording flows
  retain their originating monitor before closing transient windows.
- Global completions use only the global toast. Library-scoped mutations use
  only the library notice, positioned consistently below the library header.
- GIF conversion publishes start and finish state. The library shows a
  persistent, non-blocking progress indicator until conversion finishes, then
  the normal completion or error feedback appears.
- Portaled card menus stop click propagation so menu actions never change card
  selection.

## Consequences

Completion feedback remains noticeable without covering the center of the
user's work. It stays on the relevant display across Retina, mixed-DPI, and
multi-display arrangements. Library actions no longer produce competing
notices, GIF conversion has an immediate and durable working state, and all
card-menu actions are isolated from batch selection.
