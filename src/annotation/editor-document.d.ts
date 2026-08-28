import type { AnnotationDocumentV1 } from "./model";

export function resolveInitialEditorDocument(
  initialDocument: AnnotationDocumentV1 | null,
  sourcePixels: { width: number; height: number },
): AnnotationDocumentV1;
