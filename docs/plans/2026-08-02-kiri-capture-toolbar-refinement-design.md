# Kiri Capture Toolbar Refinement

## Direction

The capture overlay should feel like a polished Japanese creative utility: soft, compact, and characterful without becoming decorative noise. The visual language uses a lavender primary color, a restrained blossom-pink sparkle accent, continuous rounded corners, frosted surfaces, and native SF Symbols. Native symbols keep every control crisp at Retina scale and avoid licensing or stylistic inconsistency from mixed icon packs.

## Interaction

The toolbar begins with a small Kiri sparkle and an explicit close control. Drawing tools sit inside a softly tinted capsule, separating creation from history and completion actions. The selected tool receives a lavender tint, border, and quiet fill; Done is the only strongly filled action. Hover labels explain both intent and shortcuts.

Escape always cancels the complete capture session, whether the user is selecting or annotating. Region reselection remains available through right-click and the More menu, so Escape never has two meanings. Return copies, Command-Z undoes, and Shift-Command-Z redoes.

## Responsiveness

Done hides the full-screen overlay before rendering the final pixels, allowing the source application to reappear immediately. Rendering targets a direct bitmap context and returns its CGImage without the previous NSImage-to-TIFF-to-bitmap conversion. If rendering fails, Kiri restores the overlay instead of silently losing the session.

## Acceptance criteria

- Done dismisses the overlay before export work begins.
- Export avoids TIFF encoding and preserves source pixel dimensions.
- Escape cancels from selection and annotation phases.
- Close and reselect actions are visibly discoverable.
- Tool, hover, selected, disabled, and primary states remain legible in light and dark appearances.
- All icons resolve from SF Symbols on the supported macOS version.
