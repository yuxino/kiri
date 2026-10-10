import {layoutTextLines, textLineRuns} from "./text-layout.js";

/** Shared sizing for the native description editor and its saved canvas mark. */
export function calloutLabelSize({text, fontSize, boundsWidth, measureText, uiScale = 1, width}) {
  const padding = Math.max(4, fontSize * .5);
  const limit = Math.max(1, boundsWidth);
  if (width === undefined) {
    const longest = text.split(/\r?\n/).reduce((value, line) =>
      Math.max(value, textLineRuns(line, measureText).width), 0);
    // Native CSS text and canvas measurements can round differently, especially
    // after scaling. Leave the same two CSS pixels used by ordinary text editors.
    width = Math.min(limit, Math.max(Math.min(72, limit),
      Math.min(280, Math.ceil(longest) + padding * 2 + 2 * uiScale)));
  }
  const lineCount = layoutTextLines(text, Math.max(1, width - padding * 2), measureText).length;
  return {width, height: Math.ceil(lineCount * fontSize * 1.25 + padding * 2), lineCount};
}

/** Keep saved placement/width unless a repaired height needs room at the bottom. */
export function repairCalloutLabelHeight(rect, size, boundsHeight) {
  const height = Math.min(boundsHeight, size.height);
  // Older saved marks retain fractional dimensions. A subpixel rounding unit
  // must not turn merely opening a fitting note into a document edit.
  return height > rect.height + 1
    ? {...rect, y: Math.max(0, Math.min(rect.y, boundsHeight - height)), height} : rect;
}
