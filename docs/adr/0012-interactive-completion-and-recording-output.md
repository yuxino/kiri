# ADR 0012: Interactive completion and recording output

- Status: Accepted
- Date: 2026-08-26
- Supersedes: The screenshot and recording completion-toast portion of ADR
  0009, and the former 15-second GIF eligibility limit

## Context

Screenshot completion is intentionally one clipboard-first action, but a
passive success message does not show what was captured or make the next common
actions available. Recording completion is slower and can include MP4 merging
or GIF conversion, so a brief toast is also a poor representation of its
processing and recovery states.

GIF creation previously required recording an MP4 and converting it from the
library, with a short-duration eligibility limit. Users should be able to
choose the intended output before recording without putting a valid recording
at risk. The recording countdown also needs to remain legible over arbitrary
content without making one product color or output type feel more important.

## Decision

Kiri uses one resident operation-feedback window in two modes:

- Ordinary notices remain compact, short-lived, and click-through.
- A successfully persisted screenshot, MP4, or GIF uses an interactive
  completion card on the display where the operation originated. Showing it
  does not take focus from the originating application.
- The card displays a bounded thumbnail and completion detail. Clicking the
  thumbnail opens the existing image/video/GIF viewer.
- Screenshot completion still copies pixels automatically before saving. Its
  card can copy the image again. MP4 and GIF actions copy an operating-system
  file item rather than file bytes or a text path.
- Moving the asset to Trash is immediate and recoverable. The full preview
  collapses into a compact three-second Undo row that restores the same asset;
  permanent deletion remains confined to the confirmed Trash workflow.
- Ready cards close automatically after eight seconds; the compact Undo row
  closes after three seconds. Only an action in progress delays dismissal;
  hover and focus do not disable either deadline. A new completion arriving
  during Undo is queued so it cannot replace the recovery action; only the
  newest queued completion needs to be retained.
- Passive notices, completion cards, and the compact Undo row use flat surfaces
  without drop shadows.
- The feedback window is content-protected, excluded from capture, and hidden
  before a new capture begins.

Recording options include an explicit MP4/GIF output selector:

- MP4 is the default for existing and new preferences. It uses the normal
  30 fps high-quality video pipeline and includes enabled audio inputs.
- GIF may be selected before recording or created later from any recording
  with a positive known duration. There is no arbitrary maximum duration.
- GIF output is a looping, silent 12 fps animation with its long edge capped at
  720 pixels. Audio inputs are disabled for that GIF recording session, while
  the user's saved MP4 audio preferences remain intact.
- Recording still finalizes a local MP4 staging file first. GIF conversion is
  local; if GIF encoding or import fails, Kiri imports that valid MP4 and tells
  the user that the fallback was preserved.
- Once native capture closes, Kiri returns the recording flow to idle and
  restores the source application's focus before background merge/GIF work.
  A progress watchdog terminates and reaps an FFmpeg process that stops growing
  its output instead of leaving capture permanently blocked.

The recording countdown uses a neutral presentation:

- A single background-free numeral shows 3, 2, and 1 at the center of the
  display in system display typography. There is no ring, disc, panel, hint
  pill, blur, or shadow; a restrained contrasting outline preserves legibility
  over light backgrounds.
- It does not dim the selected recording region or use the accent color as an
  output cue.
- Escape cancels, and reduced-motion preferences remove the entrance animation
  without changing the timing.

## Consequences

Capture completion now provides visible confirmation and the next useful
actions without opening the library or stealing focus. Recoverable deletion
remains consistent with `AssetLibrary`, and permanent deletion does not become
easier to trigger accidentally.

Direct GIF recording adds local post-processing time and potentially large
files for long recordings, but it does not introduce an artificial cutoff or
risk the valid MP4 staging result. UI and acceptance coverage must distinguish
pixel copying from file copying, exercise GIF fallback, verify Undo ordering,
and confirm that neither the countdown nor completion window appears in an
exported capture.
