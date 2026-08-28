import { parseAnnotationDocument } from "./project.js";

/**
 * Resolves the annotation coordinate space used when an editor opens.
 * Existing projects keep their persisted logical canvas. Legacy flattened
 * images have no reliable display-scale metadata, so they use source pixels
 * as document units instead of guessing from the current window's DPR.
 */
export function resolveInitialEditorDocument(initialDocument, sourcePixels) {
  if (initialDocument !== null) return initialDocument;

  const width = positivePixelDimension(sourcePixels?.width, "sourcePixels.width");
  const height = positivePixelDimension(sourcePixels?.height, "sourcePixels.height");
  return parseAnnotationDocument({
    schemaVersion: 1,
    canvas: { width, height },
    sourcePixels: { width, height },
    marks: [],
  });
}

function positivePixelDimension(value, path) {
  if (!Number.isSafeInteger(value) || value <= 0) {
    throw new TypeError(`${path} must be a positive integer`);
  }
  return value;
}
