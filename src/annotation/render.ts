// Shared canvas rendering for the live annotation view and exported bitmap.
// Document coordinates are top-left (y down); export scales into pixel space.

import type {
  AnnotationMark,
  ColorPreset,
  MosaicIntensity,
  TextBackgroundStyle,
} from "./model";
import { COLOR_HEX, MOSAIC_VIEW_BLOCK_SIZE, arrowHeadPoints, selectionBounds } from "./model";
import type { Point, Rect } from "./geom";
import { inset, intersection, maxX, maxY, minX, minY, standardized } from "./geom";
import { layoutTextLines } from "./text-layout.js";

const FONT_STACK =
  '-apple-system, BlinkMacSystemFont, "Segoe UI", "PingFang SC", "Microsoft YaHei", sans-serif';

export function textFont(size: number): string {
  return `600 ${size}px ${FONT_STACK}`;
}

function colorValue(color: ColorPreset): string {
  return COLOR_HEX[color];
}

function backgroundValue(style: TextBackgroundStyle): string | null {
  switch (style) {
    case "transparent":
      return null;
    case "dark":
      return "rgba(0, 0, 0, 0.72)";
  }
}

export interface RenderContext {
  ctx: CanvasRenderingContext2D;
  /** Full-resolution source image. */
  sourceImage: CanvasImageSource;
  sourceWidth: number;
  sourceHeight: number;
  /** Display-local offset of the region top-left (points). */
  sourceOffset: Point;
  /** Display size of the region (points). */
  regionSize: Rect;
  /** Image pixels per annotation-document point. */
  scaleX: number;
  scaleY: number;
  /** CSS viewport points per annotation-document point. */
  viewScaleX: number;
  viewScaleY: number;
  /** True when rendering the export bitmap in pixel space. */
  exporting: boolean;
}

export interface RenderGeometryScale {
  /** Directional document-to-output multipliers. */
  x: number;
  y: number;
  /**
   * Isotropic geometry uses the smaller axis so strokes, circular brush clips,
   * and blur kernels stay inside the mapped document-space bounds.
   */
  stroke: number;
}

export function renderGeometryScale(
  exporting: boolean,
  scaleX: number,
  scaleY: number,
): RenderGeometryScale {
  if (!exporting) return { x: 1, y: 1, stroke: 1 };
  return { x: scaleX, y: scaleY, stroke: Math.min(scaleX, scaleY) };
}

export function scaleRectForRender(rect: Rect, scale: RenderGeometryScale): Rect {
  return {
    x: rect.x * scale.x,
    y: rect.y * scale.y,
    width: rect.width * scale.x,
    height: rect.height * scale.y,
  };
}

export function mosaicBlurRadius(
  brushDiameter: number,
  intensity: MosaicIntensity,
  scale: RenderGeometryScale,
): number {
  const intensityFactor = intensity === "soft" ? 0.18 : intensity === "standard" ? 0.25 : 0.34;
  const documentRadius = Math.max(2, Math.round(brushDiameter * intensityFactor));
  return Math.max(1, Math.round(documentRadius * scale.stroke));
}

function geometryScale(r: RenderContext): RenderGeometryScale {
  return renderGeometryScale(r.exporting, r.scaleX, r.scaleY);
}

function exportPoint(p: Point, r: RenderContext): Point {
  const scale = geometryScale(r);
  // Canvas 2D and the view are both y-down, so scaling without a vertical flip
  // keeps sub-region annotations aligned with the background image.
  return {
    x: p.x * scale.x,
    y: p.y * scale.y,
  };
}

function exportStrokeSize(size: number, r: RenderContext): number {
  return size * geometryScale(r).stroke;
}

function strokePolyline(ctx: CanvasRenderingContext2D, points: Point[]) {
  if (points.length === 0) return;
  ctx.beginPath();
  ctx.moveTo(points[0].x, points[0].y);
  for (let i = 1; i < points.length; i++) {
    ctx.lineTo(points[i].x, points[i].y);
  }
  ctx.stroke();
}

/** Draws one mark into the given context (already in the right space). */
function drawMark(mark: AnnotationMark, r: RenderContext, ctx: CanvasRenderingContext2D) {
  switch (mark.kind) {
    case "pen": {
      const points = mark.points.map((p) => exportPoint(p, r));
      ctx.strokeStyle = colorValue(mark.color);
      ctx.lineWidth = exportStrokeSize(mark.width, r);
      ctx.lineCap = "round";
      ctx.lineJoin = "round";
      strokePolyline(ctx, points);
      break;
    }
    case "rectangle": {
      const rect = standardized(mark.rect);
      const scale = geometryScale(r);
      const outputRect = scaleRectForRender(rect, scale);
      const p = { x: minX(outputRect), y: minY(outputRect) };
      const w = outputRect.width;
      const h = outputRect.height;
      const radius = 2 * scale.stroke;
      ctx.strokeStyle = colorValue(mark.color);
      ctx.lineWidth = mark.width * scale.stroke;
      if (w < 1 && h < 1) {
        // Keep a click-only rectangle visible as a small dot.
        const dot = Math.max(mark.width * scale.stroke * 0.7, 2 * scale.stroke);
        ctx.fillStyle = colorValue(mark.color);
        ctx.beginPath();
        ctx.arc(p.x, p.y, dot / 2, 0, Math.PI * 2);
        ctx.fill();
        break;
      }
      roundRectPath(ctx, p.x, p.y, w, h, radius);
      ctx.stroke();
      break;
    }
    case "line": {
      const start = exportPoint(mark.start, r);
      const end = exportPoint(mark.end, r);
      ctx.strokeStyle = colorValue(mark.color);
      ctx.lineWidth = exportStrokeSize(mark.width, r);
      ctx.lineCap = "round";
      ctx.beginPath();
      ctx.moveTo(start.x, start.y);
      ctx.lineTo(end.x, end.y);
      ctx.stroke();
      break;
    }
    case "arrow": {
      const start = exportPoint(mark.start, r);
      const end = exportPoint(mark.end, r);
      const width = exportStrokeSize(mark.width, r);
      ctx.strokeStyle = colorValue(mark.color);
      ctx.lineWidth = width;
      ctx.lineCap = "round";
      ctx.lineJoin = "round";
      ctx.beginPath();
      ctx.moveTo(start.x, start.y);
      ctx.lineTo(end.x, end.y);
      ctx.stroke();
      const [left, right] = arrowHeadPoints(start, end, width);
      ctx.beginPath();
      ctx.moveTo(left.x, left.y);
      ctx.lineTo(end.x, end.y);
      ctx.lineTo(right.x, right.y);
      ctx.stroke();
      break;
    }
    case "text": {
      const rect = standardized(mark.rect);
      const scale = geometryScale(r);
      const outputRect = scaleRectForRender(rect, scale);
      const p = { x: minX(outputRect), y: minY(outputRect) };
      const background = backgroundValue(mark.background);
      if (background) {
        const padX = 5 * scale.x;
        const padY = 3 * scale.y;
        ctx.fillStyle = background;
        roundRectPath(
          ctx,
          p.x - padX,
          p.y - padY,
          outputRect.width + padX * 2,
          outputRect.height + padY * 2,
          5 * scale.stroke,
        );
        ctx.fill();
      }

      // Lay text out once in document coordinates, then map glyphs through the
      // directional output transform. This preserves wrapping while scaling
      // font height by Y and glyph width by X.
      ctx.save();
      ctx.scale(scale.x, scale.y);
      ctx.fillStyle = colorValue(mark.color);
      ctx.font = textFont(mark.fontSize);
      ctx.textBaseline = "top";
      wrapText(ctx, mark.text, minX(rect), minY(rect), rect.width, mark.fontSize);
      ctx.restore();
      break;
    }
    case "mosaic": {
      drawMosaicMark(mark, r, ctx);
      break;
    }
  }
}

function roundRectPath(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  w: number,
  h: number,
  radius: number,
) {
  const r = Math.min(Math.max(radius, 0), Math.abs(w) / 2, Math.abs(h) / 2);
  ctx.beginPath();
  ctx.moveTo(x + r, y);
  ctx.arcTo(x + w, y, x + w, y + h, r);
  ctx.arcTo(x + w, y + h, x, y + h, r);
  ctx.arcTo(x, y + h, x, y, r);
  ctx.arcTo(x, y, x + w, y, r);
  ctx.closePath();
}

function wrapText(
  ctx: CanvasRenderingContext2D,
  text: string,
  x: number,
  y: number,
  maxWidth: number,
  fontSize: number,
) {
  const lineHeight = fontSize * 1.25;
  const lines = layoutTextLines(text, maxWidth, (value) => ctx.measureText(value).width);
  for (const [index, line] of lines.entries()) {
    if (line) ctx.fillText(line, x, y + index * lineHeight);
  }
}

// ---------------------------------------------------------------------------
// Mosaic (spec §7)
// ---------------------------------------------------------------------------

function mosaicStrokeBounds(points: Point[], diameter: number, region: Rect): Rect | null {
  const radius = diameter / 2;
  const xs = points.map((p) => p.x);
  const ys = points.map((p) => p.y);
  const bounds = {
    x: Math.min(...xs) - radius,
    y: Math.min(...ys) - radius,
    width: Math.max(...xs) - Math.min(...xs) + diameter,
    height: Math.max(...ys) - Math.min(...ys) + diameter,
  };
  const clipped = intersection(bounds, region);
  return clipped.width >= 1 && clipped.height >= 1 ? clipped : null;
}

function clipToMosaicStroke(ctx: CanvasRenderingContext2D, points: Point[], diameter: number) {
  ctx.save();
  ctx.beginPath();
  // Canvas 2D clip() uses the path's *fill* region, so an open polyline
  // would clip to ~nothing. Build the stroke band as the union of one disk
  // per sample point: sampling distance (≥0.5pt) is far smaller than the
  // brush diameter (≥12pt), so the disks overlap into a continuous band,
  // equivalent to clipping against the brush's stroked outline.
  const radius = diameter / 2;
  if (points.length === 1) {
    ctx.arc(points[0].x, points[0].y, radius, 0, Math.PI * 2);
  } else {
    for (const p of points) {
      ctx.moveTo(p.x + radius, p.y);
      ctx.arc(p.x, p.y, radius, 0, Math.PI * 2);
    }
  }
  ctx.clip();
}

function drawMosaicMark(
  mark: AnnotationMark & { kind: "mosaic" },
  r: RenderContext,
  ctx: CanvasRenderingContext2D,
) {
  const region = { x: 0, y: 0, width: r.regionSize.width, height: r.regionSize.height };
  const viewRect = mosaicStrokeBounds(mark.points, mark.brushDiameter, region);
  if (!viewRect) return;

  // Source-pixel crop: mark coordinates are region-local; add the source
  // offset of the region within the full-resolution image.
  const crop = {
    x: Math.floor((r.sourceOffset.x + minX(viewRect)) * r.scaleX),
    y: Math.floor((r.sourceOffset.y + minY(viewRect)) * r.scaleY),
    width: Math.ceil(viewRect.width * r.scaleX),
    height: Math.ceil(viewRect.height * r.scaleY),
  };
  const sourceW = r.sourceWidth;
  const sourceH = r.sourceHeight;
  const cx = Math.max(0, Math.min(crop.x, sourceW));
  const cy = Math.max(0, Math.min(crop.y, sourceH));
  const cw = Math.max(1, Math.min(crop.width, sourceW - cx));
  const ch = Math.max(1, Math.min(crop.height, sourceH - cy));

  const scale = geometryScale(r);
  const clipDiameter = mark.brushDiameter * scale.stroke;
  const points = r.exporting ? mark.points.map((p) => exportPoint(p, r)) : mark.points;
  const drawW = r.exporting ? viewRect.width * r.scaleX : viewRect.width;
  const drawH = r.exporting ? viewRect.height * r.scaleY : viewRect.height;
  const drawX = r.exporting ? minX(viewRect) * r.scaleX : minX(viewRect);
  const drawY = r.exporting ? minY(viewRect) * r.scaleY : minY(viewRect);

  if (mark.style === "blur") {
    // Gaussian-blur mosaic: draw the source crop into an offscreen canvas,
    // blur it, and stamp it through the brush-stroke clip. The blur radius
    // scales with the brush diameter and the intensity preset.
    const off = document.createElement("canvas");
    off.width = cw;
    off.height = ch;
    const offCtx = off.getContext("2d")!;
    offCtx.drawImage(r.sourceImage, cx, cy, cw, ch, 0, 0, cw, ch);
    const blurPx = mosaicBlurRadius(mark.brushDiameter, mark.intensity, scale);
    clipToMosaicStroke(ctx, points, clipDiameter);
    ctx.filter = `blur(${blurPx}px)`;
    ctx.drawImage(off, 0, 0, cw, ch, drawX, drawY, drawW, drawH);
    ctx.filter = "none";
    ctx.restore();
    return;
  }

  const blockSizeX = MOSAIC_VIEW_BLOCK_SIZE[mark.intensity] * r.scaleX;
  const blockSizeY = MOSAIC_VIEW_BLOCK_SIZE[mark.intensity] * r.scaleY;

  const smallW = Math.max(1, Math.ceil(cw / blockSizeX));
  const smallH = Math.max(1, Math.ceil(ch / blockSizeY));
  const small = document.createElement("canvas");
  small.width = smallW;
  small.height = smallH;
  const smallCtx = small.getContext("2d")!;
  smallCtx.imageSmoothingEnabled = false;
  smallCtx.drawImage(r.sourceImage, cx, cy, cw, ch, 0, 0, smallW, smallH);

  clipToMosaicStroke(ctx, points, clipDiameter);
  ctx.imageSmoothingEnabled = false;
  ctx.drawImage(small, 0, 0, smallW, smallH, drawX, drawY, drawW, drawH);
  ctx.restore();
}

/** Full render: background image → mosaics → other marks → draft → cursor → selection. */
export function renderAll(
  r: RenderContext,
  marks: AnnotationMark[],
  options: {
    draft?: AnnotationMark | null;
    brushCursor?: Point | null;
    brushDiameter?: number;
    selectedIndex?: number | null;
    editingIndex?: number | null;
  } = {},
) {
  const { ctx } = r;
  const region = { x: 0, y: 0, width: r.regionSize.width, height: r.regionSize.height };

  ctx.fillStyle = "#141414";
  if (r.exporting) {
    ctx.fillRect(0, 0, ctx.canvas.width, ctx.canvas.height);
  } else {
    ctx.fillRect(0, 0, region.width, region.height);
  }

  if (r.exporting) {
    ctx.imageSmoothingEnabled = true;
    ctx.imageSmoothingQuality = "high";
    ctx.drawImage(
      r.sourceImage,
      r.sourceOffset.x * r.scaleX,
      r.sourceOffset.y * r.scaleY,
      region.width * r.scaleX,
      region.height * r.scaleY,
      0,
      0,
      region.width * r.scaleX,
      region.height * r.scaleY,
    );
  } else {
    ctx.imageSmoothingEnabled = true;
    ctx.drawImage(
      r.sourceImage,
      r.sourceOffset.x * r.scaleX,
      r.sourceOffset.y * r.scaleY,
      region.width * r.scaleX,
      region.height * r.scaleY,
      0,
      0,
      region.width,
      region.height,
    );
  }

  const mosaics = marks.filter((m) => m.kind === "mosaic");
  const others = marks.filter((m) => m.kind !== "mosaic");
  for (const mark of mosaics) drawMark(mark, r, ctx);
  for (const mark of others) {
    if (options.editingIndex !== null && options.editingIndex !== undefined) {
      const editingMark = marks[options.editingIndex];
      if (editingMark && mark.id === editingMark.id) continue;
    }
    drawMark(mark, r, ctx);
  }
  if (options.draft) drawMark(options.draft, r, ctx);

  if (!r.exporting && options.brushCursor && options.brushDiameter) {
    drawBrushCursor(
      ctx,
      options.brushCursor,
      options.brushDiameter,
      r.viewScaleX,
      r.viewScaleY,
    );
  }
  if (!r.exporting && options.selectedIndex !== null && options.selectedIndex !== undefined) {
    const selected = marks[options.selectedIndex];
    if (selected) drawSelectionOutline(ctx, selected, r.viewScaleX, r.viewScaleY);
  }
}

function drawBrushCursor(
  ctx: CanvasRenderingContext2D,
  p: Point,
  diameter: number,
  viewScaleX: number,
  viewScaleY: number,
) {
  const center = { x: p.x * viewScaleX, y: p.y * viewScaleY };
  const radiusX = (diameter * viewScaleX) / 2;
  const radiusY = (diameter * viewScaleY) / 2;
  // Interaction chrome is drawn in CSS viewport coordinates so its outline
  // remains legible while the document itself is zoomed to fit the editor.
  ctx.save();
  ctx.scale(1 / viewScaleX, 1 / viewScaleY);
  ctx.beginPath();
  ctx.ellipse(center.x, center.y, radiusX, radiusY, 0, 0, Math.PI * 2);
  ctx.strokeStyle = "rgba(0, 0, 0, 0.72)";
  ctx.lineWidth = 3;
  ctx.stroke();
  ctx.beginPath();
  ctx.ellipse(center.x, center.y, radiusX, radiusY, 0, 0, Math.PI * 2);
  ctx.strokeStyle = "rgba(255, 255, 255, 0.95)";
  ctx.lineWidth = 1.5;
  ctx.stroke();
  ctx.restore();
}

function drawSelectionOutline(
  ctx: CanvasRenderingContext2D,
  mark: AnnotationMark,
  viewScaleX: number,
  viewScaleY: number,
) {
  const documentBounds = selectionBounds(mark);
  const bounds = {
    x: documentBounds.x * viewScaleX,
    y: documentBounds.y * viewScaleY,
    width: documentBounds.width * viewScaleX,
    height: documentBounds.height * viewScaleY,
  };
  const toViewPoint = (point: Point): Point => ({
    x: point.x * viewScaleX,
    y: point.y * viewScaleY,
  });
  ctx.save();
  ctx.scale(1 / viewScaleX, 1 / viewScaleY);
  if (mark.kind === "line" || mark.kind === "arrow") {
    drawSelectionHandle(ctx, toViewPoint(mark.start));
    drawSelectionHandle(ctx, toViewPoint(mark.end));
    ctx.restore();
    return;
  }
  const outline = inset(standardized(bounds), 5, 5);
  roundRectPath(ctx, outline.x, outline.y, outline.width, outline.height, 6);
  ctx.setLineDash([4, 3]);
  ctx.strokeStyle = "rgba(255, 255, 255, 0.96)";
  ctx.lineWidth = 1.5;
  ctx.stroke();
  ctx.strokeStyle = "#050505";
  ctx.lineWidth = 1;
  ctx.stroke();
  ctx.setLineDash([]);
  if (mark.kind === "rectangle") {
    const handles = [
      { x: minX(bounds), y: minY(bounds) },
      { x: (minX(bounds) + maxX(bounds)) / 2, y: minY(bounds) },
      { x: maxX(bounds), y: minY(bounds) },
      { x: maxX(bounds), y: (minY(bounds) + maxY(bounds)) / 2 },
      { x: maxX(bounds), y: maxY(bounds) },
      { x: (minX(bounds) + maxX(bounds)) / 2, y: maxY(bounds) },
      { x: minX(bounds), y: maxY(bounds) },
      { x: minX(bounds), y: (minY(bounds) + maxY(bounds)) / 2 },
    ];
    for (const handle of handles) drawSelectionHandle(ctx, handle);
  }
  ctx.restore();
}

function drawSelectionHandle(ctx: CanvasRenderingContext2D, p: Point) {
  ctx.fillStyle = "#FFFFFF";
  ctx.beginPath();
  ctx.arc(p.x, p.y, 5, 0, Math.PI * 2);
  ctx.fill();
  ctx.fillStyle = "#050505";
  ctx.beginPath();
  ctx.arc(p.x, p.y, 3, 0, Math.PI * 2);
  ctx.fill();
}
