import type { Rect } from "../annotation/geom";
export function captureToolbarPosition(
  selection: Rect,
  bounds: Rect,
  size: { width: number; height: number },
): { left: number; top: number };
