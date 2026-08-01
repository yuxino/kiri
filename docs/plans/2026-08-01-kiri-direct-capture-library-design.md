# Kiri Direct Capture and Library Onboarding Design

## Goal

Remove confirmation friction from region capture and make the library's first
screen explain the product instead of presenting a large, poorly aligned
empty window.

## Research takeaways

ShareX's default region workflow completes a rectangular selection on mouse
release; Enter or double-click is reserved for its optional multi-region
mode. Snapzy's native macOS live-area flow also captures the chosen region at
mouse-up, then routes the result through configurable post-capture actions.
Flameshot keeps annotation shortcuts discoverable through capture help and
tool labels. Kiri should use the same separation: selection is one gesture,
annotation is the next state, and export is a clear final action.

## Capture flow

The initial overlay shows one compact instruction: drag to capture a region,
click a highlighted window, or press Escape to cancel. While dragging, the
dimension badge remains visible and the instruction changes to “Release to
capture.” Releasing a valid drag immediately crops the selection and enters
inline annotation. Clicking a highlighted window follows the same path.

There is no double-click affordance. Return in annotation copies and closes;
Escape returns to selection for the exceptional case where the user wants a
different region. The annotation toolbar includes a quiet second line that
states the completion and recovery shortcuts. Tool buttons retain hover help,
selected state, undo/redo availability, and the More menu.

## Library layout

The root view always fills the window and anchors its toolbar to the top. The
header uses one title, a small item count, search, trash/library navigation,
and a primary Capture button. It does not repeat the app name and subtitle in
a second oversized title block.

The initial empty state is a compact onboarding card centered in the usable
content area. It contains a capture icon, “Ready for your first capture,” a
short value statement, the primary Capture Region action, the shortcut, and
a three-step flow: drag a region, annotate if needed, Return to copy. Search
with no matches and an empty Trash use distinct lightweight states instead of
claiming there are no captures. A small progress state prevents the onboarding
card from flashing before the library finishes its first load.

## Visual system and accessibility

The library uses system background materials, SF Symbols, system accent
color, 12–16 point spacing, and restrained rounded surfaces. Controls retain
text labels where their meaning is not obvious. Keyboard shortcuts remain
visible in tooltips or the onboarding flow. Error banners stay at the top and
recovery actions remain keyboard accessible.

## Verification

Pure selection geometry and model tests continue to run in KiriCore. AppKit
and SwiftUI code compiles in debug and release with warnings as errors. The
library is rendered to an offscreen image for layout inspection; no capture
overlay or app window is shown during automated checks. Packaging uses the
existing stable signing identity and does not reset privacy permissions.
