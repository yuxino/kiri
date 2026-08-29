import { parseAnnotationDocument } from "./project.js";

export const MIN_CROP_SOURCE_PIXELS = 8;

export function fullCropRect(document) {
  const parsed = parseAnnotationDocument(document);
  return { x: 0, y: 0, width: parsed.canvas.width, height: parsed.canvas.height };
}

export function cropPixelsFromDocumentRect(document, selection) {
  const parsed = parseAnnotationDocument(document);
  const rect = normalizedRect(selection);
  const scaleX = parsed.sourcePixels.width / parsed.canvas.width;
  const scaleY = parsed.sourcePixels.height / parsed.canvas.height;
  const left = clamp(Math.round(rect.x * scaleX), 0, parsed.sourcePixels.width - 1);
  const top = clamp(Math.round(rect.y * scaleY), 0, parsed.sourcePixels.height - 1);
  const right = clamp(
    Math.round((rect.x + rect.width) * scaleX),
    left + 1,
    parsed.sourcePixels.width,
  );
  const bottom = clamp(
    Math.round((rect.y + rect.height) * scaleY),
    top + 1,
    parsed.sourcePixels.height,
  );
  return { x: left, y: top, width: right - left, height: bottom - top };
}

export function isFullCrop(document, selection) {
  const parsed = parseAnnotationDocument(document);
  const crop = cropPixelsFromDocumentRect(parsed, selection);
  return crop.x === 0 && crop.y === 0 &&
    crop.width === parsed.sourcePixels.width &&
    crop.height === parsed.sourcePixels.height;
}

export function cropAnnotationDocument(document, selection) {
  const parsed = parseAnnotationDocument(document);
  const cropPixels = cropPixelsFromDocumentRect(parsed, selection);
  const scaleX = parsed.sourcePixels.width / parsed.canvas.width;
  const scaleY = parsed.sourcePixels.height / parsed.canvas.height;
  const cropRect = {
    x: cropPixels.x / scaleX,
    y: cropPixels.y / scaleY,
    width: cropPixels.width / scaleX,
    height: cropPixels.height / scaleY,
  };
  const translated = parsed.marks
    .filter((mark) => intersects(markBounds(mark), cropRect))
    .map((mark) => translateMark(mark, -cropRect.x, -cropRect.y));
  return {
    cropPixels,
    document: parseAnnotationDocument({
      schemaVersion: 1,
      canvas: { width: cropRect.width, height: cropRect.height },
      sourcePixels: { width: cropPixels.width, height: cropPixels.height },
      marks: translated,
    }),
  };
}

function markBounds(mark) {
  switch (mark.kind) {
    case "pen":
      return pointsBounds(mark.points, mark.width / 2);
    case "mosaic":
      return pointsBounds(mark.points, mark.brushDiameter / 2);
    case "rectangle":
      return paddedRect(mark.rect, mark.width / 2);
    case "line":
      return pointsBounds([mark.start, mark.end], mark.width / 2);
    case "arrow":
      return pointsBounds(
        [mark.start, mark.end],
        Math.max(mark.width * 4, 12) + mark.width / 2,
      );
    case "text":
      return normalizedRect(mark.rect);
  }
}

function translateMark(mark, dx, dy) {
  switch (mark.kind) {
    case "pen":
    case "mosaic":
      return { ...mark, points: mark.points.map((point) => translatePoint(point, dx, dy)) };
    case "rectangle":
    case "text":
      return { ...mark, rect: { ...mark.rect, x: mark.rect.x + dx, y: mark.rect.y + dy } };
    case "line":
    case "arrow":
      return {
        ...mark,
        start: translatePoint(mark.start, dx, dy),
        end: translatePoint(mark.end, dx, dy),
      };
  }
}

function translatePoint(point, dx, dy) {
  return { x: point.x + dx, y: point.y + dy };
}

function pointsBounds(points, padding) {
  const xs = points.map((point) => point.x);
  const ys = points.map((point) => point.y);
  const left = Math.min(...xs) - padding;
  const top = Math.min(...ys) - padding;
  const right = Math.max(...xs) + padding;
  const bottom = Math.max(...ys) + padding;
  return { x: left, y: top, width: right - left, height: bottom - top };
}

function paddedRect(value, padding) {
  const rect = normalizedRect(value);
  return {
    x: rect.x - padding,
    y: rect.y - padding,
    width: rect.width + padding * 2,
    height: rect.height + padding * 2,
  };
}

function normalizedRect(value) {
  if (!value || ![value.x, value.y, value.width, value.height].every(Number.isFinite)) {
    throw new TypeError("crop selection must be a finite rectangle");
  }
  return {
    x: value.width >= 0 ? value.x : value.x + value.width,
    y: value.height >= 0 ? value.y : value.y + value.height,
    width: Math.abs(value.width),
    height: Math.abs(value.height),
  };
}

function intersects(a, b) {
  return a.x + a.width >= b.x && b.x + b.width >= a.x &&
    a.y + a.height >= b.y && b.y + b.height >= a.y;
}

function clamp(value, minimum, maximum) {
  return Math.min(Math.max(value, minimum), maximum);
}
