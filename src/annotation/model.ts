// Annotation data model + history — port of AnnotationCanvasView.swift /
// AnnotationHistory.swift. All coordinates are canvas points relative to the
// image rect (top-left origin).

import type { Point, Rect } from "./geom";
import {
  distanceToSegment,
  maxX,
  maxY,
  minX,
  minY,
  pointBounds,
  polylineDistance,
  standardized,
} from "./geom";

export type Tool = "select" | "pen" | "rectangle" | "line" | "arrow" | "text" | "mosaic";

export type ColorPreset =
  | "violet"
  | "cherry"
  | "orange"
  | "yellow"
  | "mint"
  | "blue"
  | "white"
  | "black";

export const COLOR_PRESETS: ColorPreset[] = [
  "violet",
  "cherry",
  "orange",
  "yellow",
  "mint",
  "blue",
  "white",
  "black",
];

export const COLOR_HEX: Record<ColorPreset, string> = {
  violet: "#7D69F5",
  cherry: "#FA476E",
  orange: "#FF7D2E",
  yellow: "#FFD129",
  mint: "#29C78F",
  blue: "#2994FF",
  white: "#FFFFFF",
  black: "#141414",
};

export type TextBackgroundStyle = "transparent" | "dark";
export type MosaicIntensity = "soft" | "standard" | "strong";
export type MosaicStyle = "pixel" | "blur";

export const MOSAIC_VIEW_BLOCK_SIZE: Record<MosaicIntensity, number> = {
  soft: 7,
  standard: 12,
  strong: 20,
};

export type AnnotationMark =
  | { kind: "pen"; id: number; points: Point[]; color: ColorPreset; width: number }
  | { kind: "rectangle"; id: number; rect: Rect; color: ColorPreset; width: number }
  | { kind: "line"; id: number; start: Point; end: Point; color: ColorPreset; width: number }
  | { kind: "arrow"; id: number; start: Point; end: Point; color: ColorPreset; width: number }
  | {
      kind: "text";
      id: number;
      text: string;
      rect: Rect;
      color: ColorPreset;
      background: TextBackgroundStyle;
      fontSize: number;
    }
  | {
      kind: "mosaic";
      id: number;
      points: Point[];
      brushDiameter: number;
      intensity: MosaicIntensity;
      style: MosaicStyle;
    };

export interface AppearanceSettings {
  colorPreset: ColorPreset;
  textBackgroundStyle: TextBackgroundStyle;
  mosaicIntensity: MosaicIntensity;
  mosaicStyle: MosaicStyle;
  penWidth: number;
  shapeWidth: number;
  textFontSize: number;
  mosaicBrushDiameter: number;
}

export const DEFAULT_APPEARANCE: AppearanceSettings = {
  colorPreset: "violet",
  textBackgroundStyle: "transparent",
  mosaicIntensity: "standard",
  mosaicStyle: "pixel",
  penWidth: 3,
  shapeWidth: 3,
  textFontSize: 18,
  mosaicBrushDiameter: 20,
};

interface HistoryStep {
  before: AnnotationMark[];
  after: AnnotationMark[];
  undoResult: AnnotationMark | null;
  redoResult: AnnotationMark | null;
}

export class AnnotationHistory {
  private visible: AnnotationMark[] = [];
  private undoSteps: HistoryStep[] = [];
  private redoSteps: HistoryStep[] = [];

  get elements(): AnnotationMark[] {
    return this.visible;
  }

  get canUndo(): boolean {
    return this.undoSteps.length > 0;
  }

  get canRedo(): boolean {
    return this.redoSteps.length > 0;
  }

  append(element: AnnotationMark): void {
    const before = this.visible.slice();
    this.visible = [...this.visible, element];
    this.record({ before, after: this.visible.slice(), undoResult: element, redoResult: element });
  }

  replace(index: number, element: AnnotationMark): AnnotationMark | null {
    if (index < 0 || index >= this.visible.length) return null;
    const before = this.visible.slice();
    const replaced = this.visible[index];
    const after = before.slice();
    after[index] = element;
    this.visible = after;
    this.record({ before, after: after.slice(), undoResult: element, redoResult: element });
    return replaced;
  }

  remove(index: number): AnnotationMark | null {
    if (index < 0 || index >= this.visible.length) return null;
    const before = this.visible.slice();
    const removed = this.visible[index];
    const after = before.filter((_, i) => i !== index);
    this.visible = after;
    this.record({ before, after: after.slice(), undoResult: removed, redoResult: removed });
    return removed;
  }

  /**
   * Replaces the visible array without recording history — used for live
   * previews (e.g. dragging the text-size slider). Callers must follow up
   * with a history-recording operation (append/replace/remove) or the
   * change is lost to undo.
   */
  overwrite(elements: AnnotationMark[]): void {
    this.visible = elements.slice();
  }

  undo(): AnnotationMark | null {
    const step = this.undoSteps.pop();
    if (!step) return null;
    this.visible = step.before.slice();
    this.redoSteps.push(step);
    return step.undoResult;
  }

  redo(): AnnotationMark | null {
    const step = this.redoSteps.pop();
    if (!step) return null;
    this.visible = step.after.slice();
    this.undoSteps.push(step);
    return step.redoResult;
  }

  clear(): void {
    this.visible = [];
    this.undoSteps = [];
    this.redoSteps = [];
  }

  private record(step: HistoryStep): void {
    this.undoSteps.push(step);
    this.redoSteps = [];
  }
}

// ---------------------------------------------------------------------------
// Hit testing (spec §6.1)
// ---------------------------------------------------------------------------

function hitTestMark(mark: AnnotationMark, p: Point): boolean {
  switch (mark.kind) {
    case "pen":
      return polylineDistance(p, mark.points) <= Math.max(7, mark.width / 2 + 4);
    case "rectangle": {
      const r = standardized(mark.rect);
      const pad = Math.max(6, mark.width);
      return containsPadded(r, p, pad, pad);
    }
    case "line":
    case "arrow":
      return distanceToSegment(p, mark.start, mark.end) <= Math.max(7, mark.width / 2 + 4);
    case "text": {
      const r = standardized(mark.rect);
      return containsPadded(r, p, 7, 6);
    }
    case "mosaic":
      return polylineDistance(p, mark.points) <= mark.brushDiameter / 2 + 4;
  }
}

function containsPadded(r: Rect, p: Point, dx: number, dy: number): boolean {
  return p.x >= minX(r) - dx && p.x <= maxX(r) + dx && p.y >= minY(r) - dy && p.y <= maxY(r) + dy;
}

/** Reverse-order hit test: topmost mark wins. */
export function markIndexAt(marks: AnnotationMark[], p: Point): number | null {
  for (let i = marks.length - 1; i >= 0; i--) {
    if (hitTestMark(marks[i], p)) return i;
  }
  return null;
}

// ---------------------------------------------------------------------------
// Mark transforms (spec §6.3) — used as drag previews
// ---------------------------------------------------------------------------

export function translateMark(mark: AnnotationMark, by: Point, bounds: Rect): AnnotationMark {
  switch (mark.kind) {
    case "pen": {
      const b = pointBoundsPadded(mark.points, Math.max(1, mark.width / 2));
      const tx = clampTranslation(by.x, minX(b), maxX(b), minX(bounds), maxX(bounds));
      const ty = clampTranslation(by.y, minY(b), maxY(b), minY(bounds), maxY(bounds));
      return { ...mark, points: mark.points.map((p) => ({ x: p.x + tx, y: p.y + ty })) };
    }
    case "rectangle": {
      const b = standardized(mark.rect);
      const tx = clampTranslation(by.x, minX(b), maxX(b), minX(bounds), maxX(bounds));
      const ty = clampTranslation(by.y, minY(b), maxY(b), minY(bounds), maxY(bounds));
      return { ...mark, rect: { ...mark.rect, x: mark.rect.x + tx, y: mark.rect.y + ty } };
    }
    case "line":
    case "arrow": {
      const b = pointBoundsPadded([mark.start, mark.end], Math.max(1, mark.width / 2));
      const tx = clampTranslation(by.x, minX(b), maxX(b), minX(bounds), maxX(bounds));
      const ty = clampTranslation(by.y, minY(b), maxY(b), minY(bounds), maxY(bounds));
      return {
        ...mark,
        start: { x: mark.start.x + tx, y: mark.start.y + ty },
        end: { x: mark.end.x + tx, y: mark.end.y + ty },
      };
    }
    case "text": {
      const b = standardized(mark.rect);
      const tx = clampTranslation(by.x, minX(b), maxX(b), minX(bounds), maxX(bounds));
      const ty = clampTranslation(by.y, minY(b), maxY(b), minY(bounds), maxY(bounds));
      return { ...mark, rect: { ...mark.rect, x: mark.rect.x + tx, y: mark.rect.y + ty } };
    }
    case "mosaic": {
      const b = pointBoundsPadded(mark.points, mark.brushDiameter / 2);
      const tx = clampTranslation(by.x, minX(b), maxX(b), minX(bounds), maxX(bounds));
      const ty = clampTranslation(by.y, minY(b), maxY(b), minY(bounds), maxY(bounds));
      return { ...mark, points: mark.points.map((p) => ({ x: p.x + tx, y: p.y + ty })) };
    }
  }
}

function pointBoundsPadded(points: Point[], pad: number): Rect {
  const xs = points.map((p) => p.x);
  const ys = points.map((p) => p.y);
  const x = Math.min(...xs) - pad;
  const y = Math.min(...ys) - pad;
  return {
    x,
    y,
    width: Math.max(...xs) - x + pad,
    height: Math.max(...ys) - y + pad,
  };
}

function clampTranslation(
  delta: number,
  markMin: number,
  markMax: number,
  boundMin: number,
  boundMax: number,
): number {
  return Math.min(Math.max(delta, boundMin - markMin), boundMax - markMax);
}

export function resizeRectangleMark(
  mark: AnnotationMark,
  handle: string,
  point: Point,
  bounds: Rect,
): AnnotationMark {
  if (mark.kind !== "rectangle") return mark;
  return { ...mark, rect: resizeRect(mark.rect, handle, point, bounds) };
}

// Reuse the SelectionGeometry resize algorithm from geom.ts.
import { resized } from "./geom";

function resizeRect(r: Rect, handle: string, point: Point, bounds: Rect): Rect {
  return resized(r, handle as Parameters<typeof resized>[1], point, bounds, 8);
}

export function moveEndpointMark(
  mark: AnnotationMark,
  isStart: boolean,
  point: Point,
): AnnotationMark {
  if (mark.kind !== "line" && mark.kind !== "arrow") return mark;
  return isStart ? { ...mark, start: point } : { ...mark, end: point };
}

/** Selection bounds used for the outline (spec §6.4). */
export function selectionBounds(mark: AnnotationMark): Rect {
  switch (mark.kind) {
    case "pen": {
      const b = pointBounds(mark.points);
      const pad = Math.max(1, mark.width / 2);
      return { x: b.x - pad, y: b.y - pad, width: b.width + pad * 2, height: b.height + pad * 2 };
    }
    case "rectangle":
      return standardized(mark.rect);
    case "line":
    case "arrow": {
      const b = pointBounds([mark.start, mark.end]);
      const pad = Math.max(1, mark.width / 2);
      return { x: b.x - pad, y: b.y - pad, width: b.width + pad * 2, height: b.height + pad * 2 };
    }
    case "text":
      return standardized(mark.rect);
    case "mosaic": {
      const b = pointBounds(mark.points);
      const pad = mark.brushDiameter / 2;
      return { x: b.x - pad, y: b.y - pad, width: b.width + pad * 2, height: b.height + pad * 2 };
    }
  }
}

/** Arrow head geometry (spec §5.4). */
export function arrowHeadPoints(start: Point, end: Point, width: number): [Point, Point] {
  const angle = Math.atan2(end.y - start.y, end.x - start.x);
  const headLength = Math.max(12, width * 4);
  return [
    {
      x: end.x - headLength * Math.cos(angle - Math.PI / 6),
      y: end.y - headLength * Math.sin(angle - Math.PI / 6),
    },
    {
      x: end.x - headLength * Math.cos(angle + Math.PI / 6),
      y: end.y - headLength * Math.sin(angle + Math.PI / 6),
    },
  ];
}
