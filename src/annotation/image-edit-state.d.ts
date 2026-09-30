import type { AnnotationMark } from "./model";
import type { Rect } from "./geom";
export interface ImageTextDraft { mark: AnnotationMark | null; previousId: number | null; editing: boolean; }
export interface ImageEditSnapshot { marks: AnnotationMark[]; crop: Rect | null; }
export function hasPendingImageTextChange(marks: AnnotationMark[], draft: ImageTextDraft | null): boolean;
export function hasUnsavedImageChanges(saved: ImageEditSnapshot, marks: AnnotationMark[], crop: Rect | null, draft: ImageTextDraft | null): boolean;
