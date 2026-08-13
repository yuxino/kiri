// Geometry helpers — port of SelectionGeometry.swift + canvas hit-test math.
// All coordinates are top-left oriented (y down), matching Canvas 2D.

export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface Point {
  x: number;
  y: number;
}

export type SelectionHandle =
  | "topLeft"
  | "top"
  | "topRight"
  | "right"
  | "bottomRight"
  | "bottom"
  | "bottomLeft"
  | "left";

export const ALL_HANDLES: SelectionHandle[] = [
  "topLeft",
  "top",
  "topRight",
  "right",
  "bottomRight",
  "bottom",
  "bottomLeft",
  "left",
];

export function rect(x: number, y: number, width: number, height: number): Rect {
  return { x, y, width, height };
}

export function minX(r: Rect): number {
  return r.x;
}
export function minY(r: Rect): number {
  return r.y;
}
export function maxX(r: Rect): number {
  return r.x + r.width;
}
export function maxY(r: Rect): number {
  return r.y + r.height;
}
export function midX(r: Rect): number {
  return r.x + r.width / 2;
}
export function midY(r: Rect): number {
  return r.y + r.height / 2;
}

export function standardized(r: Rect): Rect {
  return rect(
    r.width >= 0 ? r.x : r.x + r.width,
    r.height >= 0 ? r.y : r.y + r.height,
    Math.abs(r.width),
    Math.abs(r.height),
  );
}

export function intersection(a: Rect, b: Rect): Rect {
  const x = Math.max(minX(a), minX(b));
  const y = Math.max(minY(a), minY(b));
  const x2 = Math.min(maxX(a), maxX(b));
  const y2 = Math.min(maxY(a), maxY(b));
  if (x2 < x || y2 < y) return rect(0, 0, 0, 0);
  return rect(x, y, x2 - x, y2 - y);
}

export function contains(r: Rect, p: Point): boolean {
  return p.x >= minX(r) && p.x <= maxX(r) && p.y >= minY(r) && p.y <= maxY(r);
}

export function clampPoint(p: Point, bounds: Rect): Point {
  return {
    x: Math.min(Math.max(p.x, minX(bounds)), maxX(bounds)),
    y: Math.min(Math.max(p.y, minY(bounds)), maxY(bounds)),
  };
}

export function normalized(from: Point, to: Point): Rect {
  return rect(
    Math.min(from.x, to.x),
    Math.min(from.y, to.y),
    Math.abs(to.x - from.x),
    Math.abs(to.y - from.y),
  );
}

export function isValidSelection(r: Rect, minimumSide = 3): boolean {
  return r.width >= minimumSide && r.height >= minimumSide;
}

export function handlePoint(handle: SelectionHandle, selection: Rect): Point {
  const r = standardized(selection);
  switch (handle) {
    case "topLeft":
      return { x: minX(r), y: minY(r) };
    case "top":
      return { x: midX(r), y: minY(r) };
    case "topRight":
      return { x: maxX(r), y: minY(r) };
    case "right":
      return { x: maxX(r), y: midY(r) };
    case "bottomRight":
      return { x: maxX(r), y: maxY(r) };
    case "bottom":
      return { x: midX(r), y: maxY(r) };
    case "bottomLeft":
      return { x: minX(r), y: maxY(r) };
    case "left":
      return { x: minX(r), y: midY(r) };
  }
}

export function hitTestHandle(
  p: Point,
  selection: Rect,
  radius: number,
): SelectionHandle | null {
  if (!isValidSelection(selection) || radius < 0) return null;
  return (
    ALL_HANDLES.find((handle) => {
      const center = handlePoint(handle, selection);
      return Math.hypot(p.x - center.x, p.y - center.y) <= radius;
    }) ?? null
  );
}

export function resized(
  selection: Rect,
  handle: SelectionHandle,
  point: Point,
  bounds: Rect,
  minimumSide = 8,
): Rect {
  const r = standardized(selection);
  const limits = standardized(bounds);
  const minimum = Math.max(1, minimumSide);
  const clamped = clampPoint(point, limits);

  let newMinX = minX(r);
  let newMaxX = maxX(r);
  let newMinY = minY(r);
  let newMaxY = maxY(r);

  switch (handle) {
    case "topLeft":
    case "left":
    case "bottomLeft":
      newMinX = Math.min(clamped.x, newMaxX - minimum);
      break;
    case "topRight":
    case "right":
    case "bottomRight":
      newMaxX = Math.max(clamped.x, newMinX + minimum);
      break;
    case "top":
    case "bottom":
      break;
  }

  switch (handle) {
    case "topLeft":
    case "top":
    case "topRight":
      newMinY = Math.min(clamped.y, newMaxY - minimum);
      break;
    case "bottomLeft":
    case "bottom":
    case "bottomRight":
      newMaxY = Math.max(clamped.y, newMinY + minimum);
      break;
    case "left":
    case "right":
      break;
  }

  newMinX = Math.max(newMinX, minX(limits));
  newMaxX = Math.min(newMaxX, maxX(limits));
  newMinY = Math.max(newMinY, minY(limits));
  newMaxY = Math.min(newMaxY, maxY(limits));

  return rect(newMinX, newMinY, newMaxX - newMinX, newMaxY - newMinY);
}

export function moved(selection: Rect, translation: Point, bounds: Rect): Rect {
  const r = standardized(selection);
  const limits = standardized(bounds);
  if (r.width > limits.width || r.height > limits.height) {
    return intersection(r, limits);
  }
  const x = Math.min(
    Math.max(minX(r) + translation.x, minX(limits)),
    maxX(limits) - r.width,
  );
  const y = Math.min(
    Math.max(minY(r) + translation.y, minY(limits)),
    maxY(limits) - r.height,
  );
  return rect(x, y, r.width, r.height);
}

export function distanceToSegment(
  p: Point,
  a: Point,
  b: Point,
): number {
  const dx = b.x - a.x;
  const dy = b.y - a.y;
  const lengthSquared = dx * dx + dy * dy;
  if (lengthSquared === 0) return Math.hypot(p.x - a.x, p.y - a.y);
  const t = Math.max(
    0,
    Math.min(1, ((p.x - a.x) * dx + (p.y - a.y) * dy) / lengthSquared),
  );
  return Math.hypot(p.x - (a.x + t * dx), p.y - (a.y + t * dy));
}

export function polylineDistance(p: Point, points: Point[]): number {
  if (points.length === 0) return Infinity;
  if (points.length === 1) return Math.hypot(p.x - points[0].x, p.y - points[0].y);
  let best = Infinity;
  for (let i = 0; i < points.length - 1; i++) {
    best = Math.min(best, distanceToSegment(p, points[i], points[i + 1]));
  }
  return best;
}

export function pointBounds(points: Point[]): Rect {
  const xs = points.map((p) => p.x);
  const ys = points.map((p) => p.y);
  const x = Math.min(...xs);
  const y = Math.min(...ys);
  return rect(x, y, Math.max(...xs) - x, Math.max(...ys) - y);
}

export function inset(r: Rect, dx: number, dy: number): Rect {
  return rect(r.x - dx, r.y - dy, r.width + dx * 2, r.height + dy * 2);
}
