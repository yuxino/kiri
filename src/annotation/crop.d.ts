import type { Rect } from "./geom";
import type { AnnotationDocumentV1 } from "./model";

export interface CropPixels extends Rect {}

export const MIN_CROP_SOURCE_PIXELS: number;
export function fullCropRect(document: AnnotationDocumentV1): Rect;
export function cropPixelsFromDocumentRect(
  document: AnnotationDocumentV1,
  selection: Rect,
): CropPixels;
export function isFullCrop(document: AnnotationDocumentV1, selection: Rect): boolean;
export function cropAnnotationDocument(
  document: AnnotationDocumentV1,
  selection: Rect,
): { cropPixels: CropPixels; document: AnnotationDocumentV1 };
