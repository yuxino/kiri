const COLOR_PRESETS = new Set([
  "violet",
  "cherry",
  "orange",
  "yellow",
  "mint",
  "blue",
  "white",
  "black",
]);
const TEXT_BACKGROUNDS = new Set(["transparent", "dark"]);
const MOSAIC_INTENSITIES = new Set(["soft", "standard", "strong"]);
const MOSAIC_STYLES = new Set(["pixel", "blur"]);

export const MAX_ANNOTATION_DOCUMENT_BYTES = 4 * 1024 * 1024;

export const ANNOTATION_PROJECT_LIMITS = Object.freeze({
  maxDocumentBytes: MAX_ANNOTATION_DOCUMENT_BYTES,
  maxDimension: 65_536,
  maxMarks: 2_048,
  maxTotalPoints: 100_000,
  maxTotalText: 65_536,
  maxStyleSize: 4_096,
  maxCoordinateMagnitude: 262_144,
});

function invalid(path, reason) {
  throw new TypeError(`Invalid annotation document: ${path} ${reason}.`);
}

function objectAt(value, path) {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    invalid(path, "must be an object");
  }
  return value;
}

function exactKeys(value, expected, path) {
  const allowed = new Set(expected);
  for (const key of Object.keys(value)) {
    if (!allowed.has(key)) invalid(path, `has unexpected field ${JSON.stringify(key)}`);
  }
  for (const key of expected) {
    if (!Object.hasOwn(value, key)) invalid(`${path}.${key}`, "is required");
  }
}

function finiteNumber(value, path, options = {}) {
  const { min = -Infinity, max = Infinity, integer = false } = options;
  if (typeof value !== "number" || !Number.isFinite(value)) {
    invalid(path, "must be a finite number");
  }
  if (integer && !Number.isInteger(value)) invalid(path, "must be an integer");
  if (value < min || value > max) invalid(path, `must be between ${min} and ${max}`);
  return value;
}

function enumValue(value, choices, path) {
  if (typeof value !== "string" || !choices.has(value)) {
    invalid(path, "has an unknown value");
  }
  return value;
}

function parseSize(value, path, integer) {
  const size = objectAt(value, path);
  exactKeys(size, ["width", "height"], path);
  return {
    width: finiteNumber(size.width, `${path}.width`, {
      min: Number.MIN_VALUE,
      max: ANNOTATION_PROJECT_LIMITS.maxDimension,
      integer,
    }),
    height: finiteNumber(size.height, `${path}.height`, {
      min: Number.MIN_VALUE,
      max: ANNOTATION_PROJECT_LIMITS.maxDimension,
      integer,
    }),
  };
}

function parsePoint(value, path) {
  const point = objectAt(value, path);
  exactKeys(point, ["x", "y"], path);
  const limit = ANNOTATION_PROJECT_LIMITS.maxCoordinateMagnitude;
  return {
    x: finiteNumber(point.x, `${path}.x`, { min: -limit, max: limit }),
    y: finiteNumber(point.y, `${path}.y`, { min: -limit, max: limit }),
  };
}

function parseRect(value, path) {
  const rect = objectAt(value, path);
  exactKeys(rect, ["x", "y", "width", "height"], path);
  const limit = ANNOTATION_PROJECT_LIMITS.maxCoordinateMagnitude;
  return {
    x: finiteNumber(rect.x, `${path}.x`, { min: -limit, max: limit }),
    y: finiteNumber(rect.y, `${path}.y`, { min: -limit, max: limit }),
    width: finiteNumber(rect.width, `${path}.width`, { min: 0, max: limit }),
    height: finiteNumber(rect.height, `${path}.height`, { min: 0, max: limit }),
  };
}

function parseId(value, path, ids) {
  const id = finiteNumber(value, path, { min: 0, max: Number.MAX_SAFE_INTEGER });
  if (ids.has(id)) invalid(path, `contains duplicate id ${id}`);
  ids.add(id);
  return id;
}

function parseWidth(value, path) {
  return finiteNumber(value, path, {
    min: Number.MIN_VALUE,
    max: ANNOTATION_PROJECT_LIMITS.maxStyleSize,
  });
}

function parsePoints(value, path, totals) {
  if (!Array.isArray(value) || value.length === 0) {
    invalid(path, "must be a non-empty array");
  }
  if (totals.points + value.length > ANNOTATION_PROJECT_LIMITS.maxTotalPoints) {
    invalid(path, `exceeds the total points limit of ${ANNOTATION_PROJECT_LIMITS.maxTotalPoints}`);
  }
  totals.points += value.length;
  return value.map((point, index) => parsePoint(point, `${path}[${index}]`));
}

function parseColor(value, path) {
  return enumValue(value, COLOR_PRESETS, path);
}

function parseMark(value, index, ids, totals) {
  const path = `marks[${index}]`;
  const mark = objectAt(value, path);
  if (typeof mark.kind !== "string") invalid(`${path}.kind`, "must be a string");

  switch (mark.kind) {
    case "pen":
      exactKeys(mark, ["kind", "id", "points", "color", "width"], path);
      return {
        kind: "pen",
        id: parseId(mark.id, `${path}.id`, ids),
        points: parsePoints(mark.points, `${path}.points`, totals),
        color: parseColor(mark.color, `${path}.color`),
        width: parseWidth(mark.width, `${path}.width`),
      };
    case "rectangle":
      exactKeys(mark, ["kind", "id", "rect", "color", "width"], path);
      return {
        kind: "rectangle",
        id: parseId(mark.id, `${path}.id`, ids),
        rect: parseRect(mark.rect, `${path}.rect`),
        color: parseColor(mark.color, `${path}.color`),
        width: parseWidth(mark.width, `${path}.width`),
      };
    case "line":
    case "arrow":
      exactKeys(mark, ["kind", "id", "start", "end", "color", "width"], path);
      return {
        kind: mark.kind,
        id: parseId(mark.id, `${path}.id`, ids),
        start: parsePoint(mark.start, `${path}.start`),
        end: parsePoint(mark.end, `${path}.end`),
        color: parseColor(mark.color, `${path}.color`),
        width: parseWidth(mark.width, `${path}.width`),
      };
    case "text": {
      exactKeys(
        mark,
        ["kind", "id", "text", "rect", "color", "background", "fontSize"],
        path,
      );
      if (typeof mark.text !== "string") invalid(`${path}.text`, "must be a string");
      if (totals.text + mark.text.length > ANNOTATION_PROJECT_LIMITS.maxTotalText) {
        invalid(
          `${path}.text`,
          `exceeds the total text limit of ${ANNOTATION_PROJECT_LIMITS.maxTotalText}`,
        );
      }
      totals.text += mark.text.length;
      return {
        kind: "text",
        id: parseId(mark.id, `${path}.id`, ids),
        text: mark.text,
        rect: parseRect(mark.rect, `${path}.rect`),
        color: parseColor(mark.color, `${path}.color`),
        background: enumValue(mark.background, TEXT_BACKGROUNDS, `${path}.background`),
        fontSize: parseWidth(mark.fontSize, `${path}.fontSize`),
      };
    }
    case "mosaic":
      exactKeys(
        mark,
        ["kind", "id", "points", "brushDiameter", "intensity", "style"],
        path,
      );
      return {
        kind: "mosaic",
        id: parseId(mark.id, `${path}.id`, ids),
        points: parsePoints(mark.points, `${path}.points`, totals),
        brushDiameter: parseWidth(mark.brushDiameter, `${path}.brushDiameter`),
        intensity: enumValue(mark.intensity, MOSAIC_INTENSITIES, `${path}.intensity`),
        style: enumValue(mark.style, MOSAIC_STYLES, `${path}.style`),
      };
    default:
      invalid(`${path}.kind`, "has an unknown value");
  }
}

/** Validates untrusted sidecar data and returns a detached V1 document. */
export function parseAnnotationDocument(value) {
  const document = objectAt(value, "document");
  exactKeys(document, ["schemaVersion", "canvas", "sourcePixels", "marks"], "document");
  if (document.schemaVersion !== 1) invalid("schemaVersion", "must equal 1");
  if (!Array.isArray(document.marks)) invalid("marks", "must be an array");
  if (document.marks.length > ANNOTATION_PROJECT_LIMITS.maxMarks) {
    invalid("marks", `exceeds the limit of ${ANNOTATION_PROJECT_LIMITS.maxMarks}`);
  }

  const ids = new Set();
  const totals = { points: 0, text: 0 };
  const parsed = {
    schemaVersion: 1,
    canvas: parseSize(document.canvas, "canvas", false),
    sourcePixels: parseSize(document.sourcePixels, "sourcePixels", true),
    marks: document.marks.map((mark, index) => parseMark(mark, index, ids, totals)),
  };
  const json = JSON.stringify(parsed);
  if (new TextEncoder().encode(json).byteLength > MAX_ANNOTATION_DOCUMENT_BYTES) {
    invalid(
      "document",
      `exceeds the UTF-8 size limit of ${MAX_ANNOTATION_DOCUMENT_BYTES} bytes`,
    );
  }
  return parsed;
}

/** Maps a CSS/view point into the immutable annotation-document coordinates. */
export function viewPointToDocument(point, viewSize, canvasSize) {
  const view = {
    width: finiteNumber(viewSize?.width, "viewSize.width", {
      min: Number.MIN_VALUE,
      max: ANNOTATION_PROJECT_LIMITS.maxDimension,
    }),
    height: finiteNumber(viewSize?.height, "viewSize.height", {
      min: Number.MIN_VALUE,
      max: ANNOTATION_PROJECT_LIMITS.maxDimension,
    }),
  };
  const canvas = {
    width: finiteNumber(canvasSize?.width, "canvasSize.width", {
      min: Number.MIN_VALUE,
      max: ANNOTATION_PROJECT_LIMITS.maxDimension,
    }),
    height: finiteNumber(canvasSize?.height, "canvasSize.height", {
      min: Number.MIN_VALUE,
      max: ANNOTATION_PROJECT_LIMITS.maxDimension,
    }),
  };
  const limit = ANNOTATION_PROJECT_LIMITS.maxCoordinateMagnitude;
  const p = {
    x: finiteNumber(point?.x, "point.x", { min: -limit, max: limit }),
    y: finiteNumber(point?.y, "point.y", { min: -limit, max: limit }),
  };
  return {
    x: (p.x * canvas.width) / view.width,
    y: (p.y * canvas.height) / view.height,
  };
}

/** Converts fixed CSS-pixel interaction sizes into document-space units. */
export function documentUnitsPerViewPixel(viewSize, canvasSize) {
  const view = {
    width: finiteNumber(viewSize?.width, "viewSize.width", {
      min: Number.MIN_VALUE,
      max: ANNOTATION_PROJECT_LIMITS.maxDimension,
    }),
    height: finiteNumber(viewSize?.height, "viewSize.height", {
      min: Number.MIN_VALUE,
      max: ANNOTATION_PROJECT_LIMITS.maxDimension,
    }),
  };
  const canvas = {
    width: finiteNumber(canvasSize?.width, "canvasSize.width", {
      min: Number.MIN_VALUE,
      max: ANNOTATION_PROJECT_LIMITS.maxDimension,
    }),
    height: finiteNumber(canvasSize?.height, "canvasSize.height", {
      min: Number.MIN_VALUE,
      max: ANNOTATION_PROJECT_LIMITS.maxDimension,
    }),
  };
  const x = canvas.width / view.width;
  const y = canvas.height / view.height;
  return { x, y, radial: Math.max(x, y) };
}

/**
 * Defines the integer source-pixel crop shared by capture export and Rust
 * persistence. Rendering annotations from this crop keeps the first flattened
 * PNG and every later re-edit pixel-aligned even when a selection starts on a
 * fractional display point.
 */
export function annotationSourceCrop(sourceSize, displaySize, selectionValue, outputSize) {
  const source = parseSize(sourceSize, "sourceSize", true);
  const display = parseSize(displaySize, "displaySize", false);
  const output = parseSize(outputSize, "outputSize", true);
  const selection = objectAt(selectionValue, "selection");
  exactKeys(selection, ["x", "y", "width", "height"], "selection");
  const x = finiteNumber(selection.x, "selection.x", { min: 0, max: display.width });
  const y = finiteNumber(selection.y, "selection.y", { min: 0, max: display.height });
  const width = finiteNumber(selection.width, "selection.width", {
    min: Number.MIN_VALUE,
    max: display.width,
  });
  const height = finiteNumber(selection.height, "selection.height", {
    min: Number.MIN_VALUE,
    max: display.height,
  });
  if (x + width > display.width + 0.01 || y + height > display.height + 0.01) {
    invalid("selection", "must stay within the display");
  }
  if (
    Math.round((width * source.width) / display.width) !== output.width ||
    Math.round((height * source.height) / display.height) !== output.height ||
    output.width > source.width ||
    output.height > source.height
  ) {
    invalid("outputSize", "must match the selected source pixels");
  }

  return {
    x: Math.min(
      Math.max(0, Math.round((x * source.width) / display.width)),
      source.width - output.width,
    ),
    y: Math.min(
      Math.max(0, Math.round((y * source.height) / display.height)),
      source.height - output.height,
    ),
    width: output.width,
    height: output.height,
  };
}
