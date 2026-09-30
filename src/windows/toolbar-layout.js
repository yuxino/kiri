/** Overlay-local logical coordinates; the measured toolbar includes its border. */
export function captureToolbarPosition(selection, bounds, size) {
  const margin = 8;
  const gap = 10;
  const maxLeft = Math.max(bounds.x + margin, bounds.x + bounds.width - size.width - margin);
  const maxTop = Math.max(bounds.y + margin, bounds.y + bounds.height - size.height - margin);
  // Keep the mode selector clear when the viewport has room for both HUDs.
  const minTop = Math.min(bounds.y + 96, maxTop);
  const below = selection.y + selection.height + gap;
  const preferredTop = below <= maxTop ? below : selection.y - gap - size.height;
  return {
    left: Math.min(Math.max(bounds.x + margin, selection.x + selection.width / 2 - size.width / 2), maxLeft),
    top: Math.min(Math.max(minTop, preferredTop), maxTop),
  };
}
