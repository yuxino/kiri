# Kiri UI System

Kiri is a native macOS utility. Its interface should disappear when the task
starts, communicate state without stealing focus, and keep the quick path to
one gesture.

## Interaction hierarchy

1. **Capture & Copy** is the only primary action and the global shortcut path.
2. **Capture & Annotate** is explicit and secondary.
3. Save, pin, reveal, favorite, trash, and editor actions stay contextual.
4. History is automatic and never blocks clipboard completion.

Kiri hides its own visible windows before taking a frame, restores them after
completion or cancellation, and never shows a post-capture image preview.

## Visual tokens

- Typography: San Francisco system text; title3 semibold for page titles,
  subheadline medium for assets, caption for metadata.
- Spacing: 8 compact, 12 standard, 18 roomy, and 22 page/grid padding.
- Radius: 8 controls, 10 previews, 14 cards, and 20 onboarding surfaces.
- Color: semantic macOS backgrounds, labels, separators, and the user's accent
  color. No fixed light-only surfaces.
- Motion: 160 ms hover feedback and 180 ms transient feedback. Motion never
  delays capture completion.

## Components and states

- Header: responsive one- or two-row layout, native segmented Library/Trash
  navigation, search, one secondary annotation action, and one primary capture
  action.
- Asset card: asynchronously decoded thumbnail, visible Copy action, favorite,
  overflow menu, double-click Open, file drag, and equivalent context menu.
- Trash card: visible Restore and destructive delete behind confirmation.
- Feedback: short top-center text confirmation; errors remain inline with a
  recovery action when one exists. No bottom-corner preview.
- Empty/loading/search/error: each state has distinct copy and a relevant next
  action rather than a blank surface.

## Accessibility and keyboard

- All icon-only controls have labels and help text.
- Custom AppKit controls retain visible keyboard focus and pressed states.
- Return completes annotation, Escape or right-click goes back/cancels,
  Command-Z and Shift-Command-Z undo and redo, and Command-F focuses library
  search.
- Disabled and busy controls remain visible and explain system state.

## Non-disruptive QA

Run `./scripts/render-ui-snapshots.sh OUTPUT_DIRECTORY` to render populated,
compact, dark, empty, loading, search, trash, and error states without opening or
activating an app window. The renderer uses generated fixtures in a temporary
library and never reads the user's capture library.
