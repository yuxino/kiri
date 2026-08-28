import type { Point, Rect } from "./geom";
import type { AnnotationDocumentV1 } from "./model";

export const MAX_ANNOTATION_DOCUMENT_BYTES: number;

export const ANNOTATION_PROJECT_LIMITS: Readonly<{
  maxDocumentBytes: number;
  maxDimension: number;
  maxMarks: number;
  maxTotalPoints: number;
  maxTotalText: number;
  maxStyleSize: number;
  maxCoordinateMagnitude: number;
}>;

export function parseAnnotationDocument(value: unknown): AnnotationDocumentV1;

export function viewPointToDocument(
  point: Point,
  viewSize: { width: number; height: number },
  canvasSize: { width: number; height: number },
): Point;

export function documentUnitsPerViewPixel(
  viewSize: { width: number; height: number },
  canvasSize: { width: number; height: number },
): { x: number; y: number; radial: number };

export function annotationSourceCrop(
  sourceSize: { width: number; height: number },
  displaySize: { width: number; height: number },
  selection: Rect,
  outputSize: { width: number; height: number },
): { x: number; y: number; width: number; height: number };
