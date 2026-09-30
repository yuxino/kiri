# Capture toolbar viewport regression (#65)

## Actual native before

![Official v1.6.6 toolbar clipped on the secondary display](before-linux-x11-800x600.png)

This unedited 800×600 screenshot was collected from the official v1.6.6 amd64
Debian package on a cloud Debian 13 / Xfce / X11 desktop with an empty QA profile.
The content is a public test background. Debian 13 / Xfce is outside the documented
Ubuntu 24.04 / GNOME support target.

- Primary virtual output: DUMMY0, 1364×1024 at (0, 0).
- Secondary virtual output: DUMMY1, 800×600 at (1364, 0).
- Both outputs use scale 1. This is virtual mixed-resolution evidence, not physical mixed DPI.
- Capture on DUMMY1: local region (650, 420) to (790, 580), 140×160.
- Toolbar's Done button is clipped and the toolbar overlaps the region's top edge.
- Return saves the correct region; mouse Done is the regression under test.
- Release package SHA256: `3de2f5aaabea99233ba06ac5df14ecf255ad82c3c222e052929f2affca9d4895`.
- Linux package source: `78111e9c0ca39bc055f4a164d2ee9f8d3e192f26` (package provenance, not release target).
- Screenshot SHA256 (original filename .png, JPEG bytes): `40c22c1279e26825dc0d33285f30ca6a892fdf7de20a46472a3642b6c82edb4a`.

## Fix and automated checks

The shared frontend keeps tools, Cancel and Done in a compact primary bar.
More Actions reveals colors and exact output-size fields; choosing a drawing
tool reveals its appearance controls. Each HUD hugs its own content rather than
stretching a dark row across the viewport. ResizeObserver measures the complete
group footprint, and the existing below/above placement uses that width and
height to keep both groups within the active display with an 8-point margin.

- `scripts/toolbar-layout.test.mjs`: original bottom-right region, wide display,
  all corners at 640×480 / 800×600 / 1512×982, changed heights and nonzero bounds.
- `docs/demos/full-flow/test_toolbar.py`: the actual built frontend with the
  documentation-only IPC boundary. Checks all controls at four corners at scale 1 across those three logical
  sizes and English / Chinese / Japanese, then repeats the original bottom-right
  regression at scale 1.25 / 1.5 / 2 for each size/language;
  rechecks More expansion/collapse, exact-size fields, Text / Mosaic / Pen / Select
  transitions and mouse Done. This is 63 corner/scale selections and 36 expanded-control cases.
  More supports Enter/Space, size-field Return does not complete capture, and
  the actual frontend PNG header/dimensions are checked after mouse Done.
  This does not test native display selection, physical backing pixels, OCR,
  clipboard ownership or native PNG output. Its screenshot is a **renderer
  fixture**, not the actual native after image.

## Actual native after

The initial wrapping fix at PR head `18259a1` passed actual same-condition Linux
mouse-Done acceptance; [archived evidence](https://github.com/yuxino/kiri/tree/dcfd591bdf002b48f901a5d49593aa1155825738/docs/qa/capture-toolbar).
The compact shared layout now requires a fresh exact-candidate cloud replay. Use the same two outputs,
English UI, scale 1 and (650, 420) to (790, 580) selection. Record the candidate's
source commit, package SHA256 and provenance, save an unedited screenshot, then
click Done and verify the resulting 140×160 PNG. Restore the original virtual
layout afterward. Do not label renderer fixtures as native after evidence.

Physical mixed-DPI, macOS/Windows native toolbar acceptance and supported Ubuntu
native confirmation remain separate coverage limits. This PR does not change
capture coordinates or supported platforms.
