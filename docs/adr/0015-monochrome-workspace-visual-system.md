# ADR 0015: Monochrome workspace visual system

- Status: Accepted
- Date: 2026-08-26
- Supersedes: ADR 0004 for application UI styling

## Context

Kiri's lavender kawaii-professional styling made the product recognizable, but
the color, rounded cards, shadows, and separate light/dark component treatments
also made a high-frequency capture tool feel busier than its workflow. The UI
needs a more focused identity that stays quiet around the user's own visual
content and behaves as one product across every window.

## Decision

Kiri uses a monochrome workspace system:

- Library and Settings use an off-white canvas, white work surfaces, black
  type, and neutral gray rules.
- Capture, annotation, OCR, recording, viewing, and completion surfaces use
  near-black materials with white type and controls.
- Light workspace navigation uses white selected surfaces with dark type,
  precise outlines, and short monochrome markers. Full black/white inversion
  is reserved for compact primary actions and dark capture surfaces rather
  than filling large control areas.
- Decorative purple, pink, yellow, blue, gradients, tinted shadows, and colored
  focus rings are removed. Red remains only for destructive actions and real
  error states.
- User media is never desaturated. Annotation color choices remain available
  because they change the user's output rather than decorate Kiri's chrome.
- The in-app brand mark and installed app icon retain their original artwork.
  Brand colors identify the product but are not promoted into the surrounding
  control palette.
- Cards use fine borders and restrained radii. Shadows are omitted from normal
  surfaces and reserved only where a floating menu needs separation from
  arbitrary content.
- Motion remains brief and functional; it never substitutes for hierarchy.

## Consequences

The shared palette, contrast pairs, focus treatment, radii, and action styling
live in `src/styles/design-system.css`. Feature windows consume those tokens and
must not introduce decorative colors locally. Visual acceptance covers every
window at narrow and normal sizes, both light and dark work surfaces, keyboard
focus, destructive states, and arbitrary full-color user media.
