# ADR 0027: Compact, single-surface recording countdown

- Status: Accepted
- Date: 2026-09-06
- Supersedes: Presentation and initial focus target in ADR 0026 only

## Decision

The maintainer found the 192px countdown too large and the button underneath
unnecessary. Use one centered 112px surface, a thin 2.5px near-black progress
ring and a 44px medium-weight numeral. There is no text, stop icon, keyboard
hint or separate cancel button below it in the normal state. Keep the small
white readability disc; do not add a shadow or dim the display.

Escape still cancels. Clicking the ring is an additional cancellation path,
with the existing localized accessible name. An empty button covers exactly
the ring, not a second visual element. It remains reachable with Tab and can
be activated with Enter or Space. Focus the containing surface when native
readiness completes, so the initial countdown does not acquire a focus halo;
explicit keyboard navigation still shows a focus indicator on the ring.

Errors remain visible and cancellation remains retryable. Preserve the
session-bound monotonic clock, start-after-paint ordering, capture exclusion
and recording control-state hydration. No native recording behavior changes.

## Demonstration

Rebuild and record the updated component as part of the existing complete
project walkthrough. Replace the main MP4, GIF and poster; do not add another
README entry or a standalone countdown animation. Keep detailed fixture and
capture boundaries in provenance, not in the short feature description.

## Verification

Test geometry, black palette, thin stroke, absence of the separate row,
initial focus, Tab/Enter/Space, click/Escape, cancellation retry, readiness
failure, 3-2-1 and one session-bound start. Small-window and reduced-motion
layouts stay centered. Keep native Windows visibility/focus/capture-exclusion
and start/pause/resume/stop acceptance unchanged.
