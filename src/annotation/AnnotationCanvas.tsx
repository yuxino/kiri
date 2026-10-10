// AnnotationCanvas — interactive canvas port of AnnotationCanvasView.swift.
// The parent owns tool/appearance; this component owns history, selection,
// drafts, inline text editing, and export.

import React, {
  forwardRef,
  useCallback,
  useEffect,
  useLayoutEffect,
  useImperativeHandle,
  useId,
  useMemo,
  useRef,
  useState,
} from "react";
import type { Point, Rect } from "./geom";
import { handleTextEditorKey, isTextComposition, setTextComposition } from "./text-composition.js";
import type { ColorPreset } from "./model";
import { clampPoint, hitTestHandle } from "./geom";
import {
  AnnotationHistory,
  COLOR_HEX,
  applyAnnotationAppearance,
  annotationTextForCommit,
  changeMosaicShape,
  calloutHandleAt,
  calloutPartAt,
  dragAnnotationHandle,
  markIndexAt,
  selectionBounds,
  labelGeometry,
  labelRectAtAnchor,
  translateMark,
  type AnnotationMark,
  type CalloutMark,
  type WatermarkMark,
  type AnnotationDocumentV1,
  type AppearanceSettings,
  type TextBackgroundStyle,
  type LabelDirection,
  type Tool,
  type MosaicShape,
} from "./model";
import { renderAll, textFont, type RenderContext } from "./render";
import {
  annotationSourceCrop,
  documentUnitsPerViewPixel,
  parseAnnotationDocument,
  viewPointToDocument,
} from "./project.js";
import { fitTextEditorFrame, layoutTextLines, textEditorInsets, TEXT_TAB_SIZE } from "./text-layout.js";
import { calloutLabelSize, repairCalloutLabelHeight } from "./callout-layout.js";
import { cropAnnotationDocument, isFullCrop, type CropPixels } from "./crop.js";
import { t } from "../i18n";
import { validateWatermarkDensity } from "./watermark-geometry.js";

export interface AnnotationCanvasHandle {
  undo(): void;
  redo(): void;
  clearAnnotations(): void;
  deleteSelection(): void;
  commitTextEditing(): void;
  cancelTextEditing(): boolean;
  editSelectedText(): void;
  editWatermark(): void;
  clearSelection(): void;
  cancelInteraction(): boolean;
  updateSelectionAppearance(patch: Partial<AppearanceSettings>, transient?: boolean): void;
  finishAppearanceAdjustment(): void;
  setMosaicShape(shape: MosaicShape): void;
  updateSelectedCallout(patch: Partial<Omit<CalloutMark, "kind" | "id">>, transient?: boolean): void;
  exportResult(cropSelection?: Rect): Promise<AnnotationExportResult | null>;
  /**
   * Live text font-size adjustment (spec §6.6): begin records the selected
   * text mark, set applies a preview (no history), end commits one history
   * entry. The slider value itself lives in the parent's AppearanceSettings.
   */
  beginTextFontSizeAdjustment(): void;
  setTextFontSizeLive(value: number): void;
  endTextFontSizeAdjustment(): void;
}

export interface AnnotationExportResult {
  png: Uint8Array;
  document: AnnotationDocumentV1;
  cropPixels: CropPixels | null;
}

interface Props {
  /** Full-resolution source image element. */
  image: HTMLImageElement | null;
  /** Region of the source in display-local points (top-left). */
  region: Rect;
  /**
   * Size of the coordinate space `region` lives in (the display in points).
   * Required when `region` is a sub-rect of the image (capture overlay);
   * omitted for the editor where region covers the whole image.
   */
  displaySize?: { width: number; height: number };
  /** Persisted baseline. It is loaded once, without creating undo history. */
  initialDocument?: AnnotationDocumentV1;
  /** Video editors can reload an externally owned history without remounting. */
  documentRevision?: number;
  selectedMarkId?: number | null;
  onSelectionChange?(markId: number | null): void;
  onSelectionInfo?(mark: AnnotationMark | null, editing: boolean): void;
  /** A recoverable text snapshot without committing history or interrupting IME. */
  onTextDraftChange?(mark: AnnotationMark | null, previousId: number | null, editing: boolean): void;
  onMarkCreated?(): void;
  mosaicShape?: MosaicShape;
  textEscapeCancelsEdit?: boolean;
  /** Video's toolbar commits explicitly; selecting a text track must not close its editor. */
  commitTextOnToolChange?: boolean;
  onDocumentChange?(marks: AnnotationMark[]): void;
  /** Let a video compositor present live drafts through the same effects as export. */
  onFrame?(canvas: HTMLCanvasElement): void;
  /** Video draws marks in its own layer stack; this canvas keeps hit targets and handles. */
  onLiveMarks?(marks: AnnotationMark[], draft: AnnotationMark | null, editingId: number | null): void;
  onUndo?(): void;
  onRedo?(): void;
  /** CSS viewport size; document coordinates remain fixed to canvas/region. */
  viewSize?: { width: number; height: number };
  /** Prevents edits while an immutable export snapshot is being committed. */
  interactionDisabled?: boolean;
  /** Synchronous companion to interactionDisabled for the pre-render event gap. */
  interactionLock?: { readonly locked: boolean };
  tool: Tool;
  appearance: AppearanceSettings;
  calloutNumber?: number;
  onHistoryChange(canUndo: boolean, canRedo: boolean, hasMarks: boolean): void;
  onCancel(): void;
  onError?(message: string): void;
  /**
   * Called after a text annotation is committed via Return (spec §6.6:
   * "Return commits the text and completes the capture"). The parent
   * overlay finishes the screenshot; the editor leaves this unset.
   */
  onFinishAfterTextCommit?(): void;
  /** Capture-only: Select tool double-clicks on unmarked canvas confirm the image. */
  onFinishOnBlankDoubleClick?(): void;
}

interface EditingState {
  id: number;
  index: number | null;
  text: string;
  rect: Rect;
  maxWidth: number;
  uiScale: number;
  color: ColorPreset;
  background: TextBackgroundStyle;
  fontSize: number;
  labelDirection?: LabelDirection;
  textOriginal?: Extract<AnnotationMark, {kind: "text"}>;
  callout?: CalloutMark;
  watermark?: WatermarkMark;
  watermarkOriginal?: WatermarkMark;
}

function editingCalloutMark(editing: EditingState): CalloutMark {
  return {...editing.callout!, text: editing.text, labelRect: editing.rect,
    color: editing.color, fontSize: editing.fontSize};
}

function editingWatermarkMark(editing: EditingState): WatermarkMark {
  const mark = {...editing.watermark!, text: editing.text, color: editing.color, fontSize: editing.fontSize};
  // Keep untouched legacy single marks byte-compatible. An actual content or
  // geometry edit uses the current tiled-only workflow, including its preview.
  return mark.mode === "single" && editing.watermarkOriginal &&
    JSON.stringify(mark) !== JSON.stringify(editing.watermarkOriginal) ? {...mark, mode: "tiled"} : mark;
}

function editingTextMark(editing: EditingState): Extract<AnnotationMark, {kind: "text"}> {
  const insets = textEditorInsets(editing.uiScale);
  return {kind: "text", id: editing.id, text: editing.text, color: editing.color,
    fontSize: editing.fontSize, background: editing.background,
    ...(editing.labelDirection ? {labelDirection: editing.labelDirection} : {}),
    rect: {x: editing.rect.x + insets.x, y: editing.rect.y + insets.y,
      width: Math.max(.1, editing.rect.width - 2 * insets.x), height: Math.max(.1, editing.rect.height - 2 * insets.y)}};
}

function editingWithLabel(editing: EditingState, mark: Extract<AnnotationMark, {kind: "text"}>): EditingState {
  const insets = textEditorInsets(editing.uiScale);
  return {...editing, color: mark.color, fontSize: mark.fontSize, labelDirection: mark.labelDirection,
    rect: {x: mark.rect.x - insets.x, y: mark.rect.y - insets.y,
      width: mark.rect.width + 2 * insets.x, height: mark.rect.height + 2 * insets.y}};
}

/** Fit the target side without moving the point or changing the saved text. */
function fitAnchoredLabel(mark: Extract<AnnotationMark, {kind: "text"}>, bounds: {width: number; height: number},
  measure: (text: string, fontSize: number) => number): Extract<AnnotationMark, {kind: "text"}> {
  if (!mark.labelDirection) return mark;
  const anchor = labelGeometry(mark.rect, mark.fontSize, mark.labelDirection).dot;
  if (anchor.x < 0 || anchor.x > bounds.width || anchor.y < 0 || anchor.y > bounds.height) return mark;
  const roomX = mark.labelDirection === "left" ? bounds.width - anchor.x : anchor.x;
  const roomY = 2 * Math.min(anchor.y, bounds.height - anchor.y);
  let fontSize = mark.fontSize, width = mark.rect.width, height = mark.rect.height;
  for (let attempt = 0; attempt < 64; attempt++) {
    width = Math.max(.1, Math.min(width, roomX - fontSize * 2.42));
    height = Math.max(.1, layoutTextLines(mark.text, width, text => measure(text, fontSize)).length * fontSize * 1.25);
    const radius = fontSize * .22;
    if (width + fontSize * 2.42 <= roomX + 1e-7 && height + fontSize * .8 <= roomY + 1e-7 &&
      radius <= Math.min(anchor.x, bounds.width - anchor.x) + 1e-7) break;
    fontSize *= .8;
  }
  return {...mark, fontSize, rect: labelRectAtAnchor({...mark.rect, width, height}, fontSize, mark.labelDirection, anchor)};
}

function separateCalloutLabel(mark: CalloutMark, bounds: {width: number; height: number}, gap: number): CalloutMark {
  const rect = mark.labelRect, radius = mark.size / 2;
  const nearestX = Math.max(rect.x, Math.min(rect.x + rect.width, mark.center.x));
  const nearestY = Math.max(rect.y, Math.min(rect.y + rect.height, mark.center.y));
  if (Math.hypot(mark.center.x - nearestX, mark.center.y - nearestY) >= radius + gap) return mark;
  const candidates = [
    {x: mark.center.x + radius + gap, y: rect.y},
    {x: mark.center.x - radius - gap - rect.width, y: rect.y},
    {x: rect.x, y: mark.center.y + radius + gap},
    {x: rect.x, y: mark.center.y - radius - gap - rect.height},
  ].filter(point => point.x >= 0 && point.y >= 0 && point.x + rect.width <= bounds.width && point.y + rect.height <= bounds.height)
    .sort((a, b) => Math.hypot(a.x - rect.x, a.y - rect.y) - Math.hypot(b.x - rect.x, b.y - rect.y));
  return candidates.length ? {...mark, labelRect: {...rect, ...candidates[0]}} : mark;
}

function resizeCalloutLabel(mark: CalloutMark, previous: CalloutMark,
  bounds: {width: number; height: number}, gap: number): CalloutMark {
  const rect = mark.labelRect, before = previous.labelRect;
  if (before.width === rect.width && before.height === rect.height && previous.size === mark.size) return mark;
  const radius = previous.size / 2;
  // A note on the left or above grows away from its badge. Clamping a wider
  // frame at a display edge must not push it back over the number.
  const x = before.x + before.width <= previous.center.x - radius
    ? before.x + before.width - rect.width : rect.x;
  const y = before.y + before.height <= previous.center.y - radius
    ? before.y + before.height - rect.height : rect.y;
  return separateCalloutLabel({...mark, labelRect: {...rect,
    x: Math.max(0, Math.min(x, bounds.width - rect.width)),
    y: Math.max(0, Math.min(y, bounds.height - rect.height)),
  }}, bounds, gap);
}

type Interaction =
  | { kind: "none" }
  | { kind: "draw"; tool: Tool; start: Point; points: Point[]; mosaic?: Extract<AnnotationMark, {kind: "mosaic"}> }
  | { kind: "move"; index: number; original: AnnotationMark; start: Point }
  | { kind: "resize"; index: number; original: AnnotationMark; handle: string; start: Point }
  | { kind: "endpoint"; index: number; original: AnnotationMark; isStart: boolean; start: Point };

type CanvasFrame = {left: number; top: number; width: number; height: number};

const AnnotationCanvas = forwardRef<AnnotationCanvasHandle, Props>(
  function AnnotationCanvas(
    {
      image,
      region,
      displaySize,
      initialDocument,
      documentRevision,
      selectedMarkId,
      onSelectionChange,
      onSelectionInfo,
      onTextDraftChange,
      onMarkCreated,
      mosaicShape = "brush",
      textEscapeCancelsEdit = true,
      commitTextOnToolChange = true,
      onDocumentChange,
      onFrame,
      onLiveMarks,
      onUndo,
      onRedo,
      viewSize,
      interactionDisabled = false,
      interactionLock,
      tool,
      appearance,
      calloutNumber = 1,
      onHistoryChange,
      onCancel,
      onError,
      onFinishAfterTextCommit,
      onFinishOnBlankDoubleClick,
    },
    ref,
  ) {
    const initialProjectRef = useRef<AnnotationDocumentV1 | null | undefined>(undefined);
    if (initialProjectRef.current === undefined) {
      initialProjectRef.current = initialDocument
        ? parseAnnotationDocument(initialDocument)
        : null;
    }
    const initialProject = initialProjectRef.current;
    const historyRef = useRef<AnnotationHistory | null>(null);
    if (historyRef.current === null) {
      historyRef.current = new AnnotationHistory(initialProject?.marks ?? []);
    }
    const history = historyRef.current;
    const canvasRef = useRef<HTMLCanvasElement>(null);
    const [marks, setMarks] = useState<AnnotationMark[]>(() =>
      history.elements.slice(),
    );
    const [selectedIndex, setSelectedIndex] = useState<number | null>(null);
    const selectionChangeRef = useRef(onSelectionChange);
    selectionChangeRef.current = onSelectionChange;
    // Publish only user selection changes. External revision reloads use the
    // raw state setter, so their temporary reset cannot clear the parent track.
    const selectMark = useCallback((index: number | null) => {
      selectedIndexRef.current = index;
      setSelectedIndex(index);
      selectionChangeRef.current?.(index === null ? null : history.elements[index]?.id ?? null);
    }, [history]);
    const [draft, setDraft] = useState<AnnotationMark | null>(null);
    const [brushCursor, setBrushCursor] = useState<Point | null>(null);
    const [selectCursor, setSelectCursor] = useState<string>("default");
    const [editing, setEditing] = useState<EditingState | null>(null);
    const interactionRef = useRef<Interaction>({ kind: "none" });
    const gestureRectRef = useRef<CanvasFrame | null>(null);
    const canvasClickRef = useRef({ start: { x: 0, y: 0 }, moved: false, wasEditing: false,
      editingCalloutId: null as number | null });
    const calloutClickRef = useRef<{id: number; frame: CanvasFrame} | null>(null);
    const blankDoubleClickRef = useRef(false);
    const appearanceRef = useRef(appearance);
    appearanceRef.current = appearance;
    const toolRef = useRef(tool);
    toolRef.current = tool;
    const calloutNumberRef = useRef(calloutNumber);
    calloutNumberRef.current = calloutNumber;
    const mosaicShapeRef=useRef(mosaicShape);mosaicShapeRef.current=mosaicShape;
    const markCreatedRef=useRef(onMarkCreated);markCreatedRef.current=onMarkCreated;
    const interactionDisabledRef = useRef(interactionDisabled);
    interactionDisabledRef.current = interactionDisabled;
    const interactionLockRef = useRef(interactionLock);
    interactionLockRef.current = interactionLock;
    const interactionsDisabled = useCallback(
      () => interactionDisabledRef.current || interactionLockRef.current?.locked === true,
      [],
    );
    const imageRef = useRef<HTMLImageElement | null>(null);
    imageRef.current = image;
    const editingRef = useRef<EditingState | null>(null);
    const brushCursorRef = useRef<Point | null>(null);
    useEffect(() => {
      brushCursorRef.current = brushCursor;
    }, [brushCursor]);

    useEffect(() => {
      if (!interactionDisabled) return;
      interactionRef.current = { kind: "none" };
      setDraft(null);
      setBrushCursor(null);
      setSelectCursor("default");
    }, [interactionDisabled]);

    // Canvas drawImage can crop directly from the decoded HTMLImageElement.
    // Keeping a second full-resolution source canvas would duplicate the
    // image's RGBA surface for the whole annotation session.
    const getSourceImage = useCallback((): HTMLImageElement | null => {
      const img = imageRef.current;
      if (!img || !img.complete || img.naturalWidth === 0) return null;
      return img;
    }, []);

    const documentSize = initialProject?.canvas ?? {
      width: region.width,
      height: region.height,
    };
    const errorRef = useRef(onError); errorRef.current = onError;
    const validateMarks = useCallback((candidate: AnnotationMark[]): boolean => {
      try {
        validateWatermarkDensity(candidate, {x: 0, y: 0, ...documentSize});
        const watermarks = candidate.filter(mark => mark.kind === "watermark");
        if (watermarks.length) parseAnnotationDocument({schemaVersion: 1, canvas: documentSize,
          sourcePixels: initialProject?.sourcePixels ?? {
            width: Math.max(1, Math.round(documentSize.width)), height: Math.max(1, Math.round(documentSize.height)),
          }, marks: watermarks});
        return true;
      } catch (error) {
        errorRef.current?.(t(error instanceof Error ? error.message : "Watermark is too dense. Increase its size or spacing."));
        return false;
      }
    }, [documentSize.width, documentSize.height, initialProject]);
    const validateEditingWatermark = useCallback((next: EditingState): boolean => {
      const mark = editingWatermarkMark(next);
      const candidate = history.elements.filter((_, index) => index !== next.index);
      if (annotationTextForCommit(next.text) !== null) candidate.push(mark);
      return validateMarks(candidate);
    }, [history, validateMarks]);
    const setValidatedDraft = useCallback((mark: AnnotationMark) => {
      if (mark.kind === "watermark" && !validateMarks([
        ...history.elements.filter(previous => previous.id !== mark.id), mark,
      ])) return;
      setDraft(mark);
    }, [history, validateMarks]);
    const view = useMemo(
      () => viewSize ?? { width: region.width, height: region.height },
      [region.height, region.width, viewSize],
    );
    const hitTestScale = useMemo(
      () => documentUnitsPerViewPixel(view, documentSize),
      [documentSize.height, documentSize.width, view.height, view.width],
    );
    const viewScaleX = 1 / hitTestScale.x;
    const viewScaleY = 1 / hitTestScale.y;

    const publishHistory = useCallback(() => {
      onHistoryChange(
        history.canUndo,
        history.canRedo,
        history.elements.length > 0 || editingRef.current !== null,
      );
    }, [history, onHistoryChange]);

    const documentChangeRef = useRef(onDocumentChange);
    documentChangeRef.current = onDocumentChange;
    const loadedRevisionRef = useRef(documentRevision);
    useEffect(() => {
      if (loadedRevisionRef.current === documentRevision) return;
      loadedRevisionRef.current = documentRevision;
      const document = initialDocument ? parseAnnotationDocument(initialDocument) : null;
      history.load(document?.marks ?? []);
      interactionRef.current = {kind: "none"};
      editingRef.current = null;
      setEditing(null); setDraft(null); setSelectedIndex(null);
      setMarks(history.elements.slice());
      publishHistory();
    }, [documentRevision, initialDocument, history, publishHistory]);
    useEffect(() => {
      if (selectedMarkId === undefined) return;
      const index = history.elements.findIndex(mark => mark.id === selectedMarkId);
      setSelectedIndex(index < 0 ? null : index);
    }, [selectedMarkId, documentRevision, history]);

    const syncMarks = useCallback(() => {
      setMarks(history.elements.slice());
      documentChangeRef.current?.(history.elements.slice());
      publishHistory();
    }, [history, publishHistory]);

    useEffect(()=>{
      const selected=selectedIndex===null?null:marks[selectedIndex]??null;
      const mark:AnnotationMark|null=editing?.watermark?editingWatermarkMark(editing):editing?.callout?editingCalloutMark(editing):editing?{kind:"text",id:editing.index===null?-1:marks[editing.index]?.id??-1,
        text:editing.text,rect:editing.rect,color:editing.color,background:editing.background,fontSize:editing.fontSize,
        ...(editing.labelDirection?{labelDirection:editing.labelDirection}:{})}:selected;
      onSelectionInfo?.(mark,!!editing);
    },[marks,selectedIndex,editing,onSelectionInfo]);

    useEffect(()=>{
      const text=editing?annotationTextForCommit(editing.text):null;
      const insets=textEditorInsets(editing?.uiScale);
      const mark:AnnotationMark|null=editing?.watermark&&text!==null?{...editingWatermarkMark(editing),text}:editing?.callout?editingCalloutMark(editing):editing&&!editing.watermark&&text!==null?{kind:"text",id:editing.id,text,
        rect:{x:editing.rect.x+insets.x,y:editing.rect.y+insets.y,
          width:Math.max(1,editing.rect.width-2*insets.x),height:Math.max(1,editing.rect.height-2*insets.y)},
        color:editing.color,background:editing.background,fontSize:editing.fontSize,
        ...(editing.labelDirection?{labelDirection:editing.labelDirection}:{})}:null;
      onTextDraftChange?.(mark,editing?.index!=null?marks[editing.index]?.id??null:null,!!editing);
    },[editing,marks,onTextDraftChange]);

    const appendMark=useCallback((mark:AnnotationMark)=>{
      history.append(mark);syncMarks();selectMark(history.elements.length-1);markCreatedRef.current?.();
    },[history,syncMarks,selectMark]);

    useEffect(() => {
      publishHistory();
    }, [publishHistory]);

    const updateEditingText = useCallback((text: string) => {
      const current = editingRef.current;
      if (!current) return;
      const next = { ...current, text };
      if (current.watermark && !validateEditingWatermark(next)) return;
      editingRef.current = next;
      setEditing(next);
    }, [validateEditingWatermark]);

    const updateEditingRect = useCallback((rect: Rect, fontSize?: number) => {
      const current = editingRef.current;
      if (!current) return;
      let nextRect = rect;
      if (current.callout) {
        const note = editingCalloutMark(current);
        const repairingSavedHeight = current.text === current.callout.text &&
          current.fontSize === current.callout.fontSize && rect.width === current.rect.width &&
          rect.x === current.rect.x && rect.height >= current.rect.height;
        if (!repairingSavedHeight) nextRect = resizeCalloutLabel({...note, labelRect: rect}, note,
          documentSize, 32 * current.uiScale).labelRect;
      }
      const nextFont = fontSize ?? current.fontSize;
      if (current.labelDirection) {
        const previous = editingTextMark(current), insets = textEditorInsets(current.uiScale);
        const rect = labelRectAtAnchor({x: nextRect.x + insets.x, y: nextRect.y + insets.y,
          width: Math.max(.1, nextRect.width - 2 * insets.x), height: Math.max(.1, nextRect.height - 2 * insets.y)},
          nextFont, current.labelDirection, labelGeometry(previous.rect, previous.fontSize, current.labelDirection).dot);
        nextRect = {x: rect.x - insets.x, y: rect.y - insets.y,
          width: rect.width + 2 * insets.x, height: rect.height + 2 * insets.y};
      }
      if (current.rect.x === nextRect.x && current.rect.y === nextRect.y &&
        current.rect.width === nextRect.width && current.rect.height === nextRect.height && current.fontSize === nextFont) return;
      const next = { ...current, rect: nextRect, fontSize: nextFont };
      if (current.watermark) {
        const insets = textEditorInsets(current.uiScale);
        const width = Math.max(1, nextRect.width - 2 * insets.x);
        const height = Math.max(1, nextRect.height - 2 * insets.y);
        const previous = current.watermark.rect;
        next.watermark = {...current.watermark, rect: {
          x: previous.x + (previous.width - width) / 2,
          y: previous.y + (previous.height - height) / 2, width, height,
        }};
        if (!validateEditingWatermark(next)) return;
      }
      editingRef.current = next;
      setEditing(next);
    }, [documentSize.width, documentSize.height, validateEditingWatermark]);

    const redraw = useCallback(() => {
      const canvas = canvasRef.current;
      if (!canvas) return;
      const ctx = canvas.getContext("2d");
      if (!ctx) return;
      const sourceImage = getSourceImage();
      if (!sourceImage) return;
      ctx.setTransform(
        devicePixelRatio * viewScaleX,
        0,
        0,
        devicePixelRatio * viewScaleY,
        0,
        0,
      );
      const context: RenderContext = {
        ctx,
        sourceImage,
        sourceWidth: sourceImage.naturalWidth,
        sourceHeight: sourceImage.naturalHeight,
        sourceOffset: { x: region.x, y: region.y },
        regionSize: { x: 0, y: 0, width: documentSize.width, height: documentSize.height },
        scaleX: sourceImage.naturalWidth / (displaySize?.width ?? documentSize.width),
        scaleY: sourceImage.naturalHeight / (displaySize?.height ?? documentSize.height),
        viewScaleX,
        viewScaleY,
        exporting: false,
      };
      // Moving/resizing an existing mark replaces it in place for this frame.
      // Preserve stacking and move the handles too, without touching history.
      const replacementIndex = draft ? marks.findIndex(mark => mark.id === draft.id) : -1;
      let previewMarks = replacementIndex < 0 ? marks : marks.map((mark, index) =>
        index === replacementIndex ? draft! : mark);
      if (editing?.callout) previewMarks = previewMarks.map((mark, index) => index === editing.index ? editingCalloutMark(editing) : mark);
      let drawingDraft = replacementIndex < 0 ? draft : null;
      if (editing?.watermark) {
        const watermark = editingWatermarkMark(editing);
        if (editing.index === null) drawingDraft = watermark;
        else previewMarks = previewMarks.map((mark, index) => index === editing.index ? watermark : mark);
      }
      renderAll(context, previewMarks, {
        draft: drawingDraft,
        brushCursor: (interactionRef.current.kind === "draw" && interactionRef.current.mosaic?.shape === "brush") ||
          (tool === "mosaic" && mosaicShape === "brush") ? brushCursor : null,
        brushDiameter: interactionRef.current.kind === "draw" && interactionRef.current.mosaic
          ? interactionRef.current.mosaic.brushDiameter : appearanceRef.current.mosaicBrushDiameter,
        selectedIndex: editing && !editing.callout ? null : selectedIndex,
        editingIndex: editing ? editing.index : null,
        editingId: editing?.id,
        chromeOnly: !!onLiveMarks,
      });
      onLiveMarks?.(previewMarks, drawingDraft, editing?.index != null ? marks[editing.index]?.id ?? null : null);
      onFrame?.(canvas);
    }, [
      marks,
      draft,
      brushCursor,
      appearance.mosaicBrushDiameter,
      tool,
      mosaicShape,
      selectedIndex,
      editing,
      region.x,
      region.y,
      documentSize.width,
      documentSize.height,
      view.width,
      view.height,
      viewScaleX,
      viewScaleY,
      displaySize,
      getSourceImage,
      onFrame,
      onLiveMarks,
    ]);

    useEffect(() => {
      redraw();
    }, [redraw, image]);

    const toPoint = useCallback((e: React.PointerEvent | MouseEvent): Point => {
      const canvas = canvasRef.current!;
      // Selecting a callout can open its inspector and resize the editor stage
      // before pointerup. Keep this gesture in the coordinate space it began in.
      const rect = interactionRef.current.kind !== "none" && gestureRectRef.current
        ? gestureRectRef.current : canvas.getBoundingClientRect();
      return viewPointToDocument(
        { x: e.clientX - rect.left, y: e.clientY - rect.top },
        { width: rect.width, height: rect.height },
        documentSize,
      );
    }, [documentSize.height, documentSize.width]);

    const commitText = useCallback(() => {
      const current = editingRef.current;
      if (!current) return;
      if (current.watermark) {
        const text = annotationTextForCommit(current.text);
        if (text !== null && !validateEditingWatermark(current)) return;
        editingRef.current = null; setEditing(null); publishHistory();
        if (text === null) {
          if (current.index !== null) { history.remove(current.index); selectMark(null); syncMarks(); }
          return;
        }
        const mark = {...editingWatermarkMark(current), text};
        if (current.index === null) appendMark(mark);
        else {
          if (JSON.stringify(history.elements[current.index]) !== JSON.stringify(mark)) {
            history.replace(current.index, mark); syncMarks();
          }
          selectMark(current.index);
        }
        return;
      }
      editingRef.current = null;
      setEditing(null);
      // Keep Clear enabled while an inline edit exists, then publish its
      // removal even when a new empty text box produces no history entry.
      publishHistory();
      if (current.callout && current.index !== null) {
        const mark = editingCalloutMark(current);
        const previous = history.elements[current.index];
        const context = canvasRef.current?.getContext("2d");
        const fittedRect = (rect: Rect) => {
          if (!mark.text || !context) return rect;
          context.save(); context.font = textFont(mark.fontSize);
          const size = calloutLabelSize({text: mark.text, fontSize: mark.fontSize, width: rect.width,
            boundsWidth: documentSize.width, measureText: value => context.measureText(value).width});
          context.restore();
          return repairCalloutLabelHeight(rect, size, documentSize.height);
        };
        if (previous?.kind === "callout" && previous.text === mark.text && previous.color === mark.color &&
          previous.fontSize === mark.fontSize && previous.number === mark.number && previous.size === mark.size && previous.style === mark.style) {
          mark.labelRect = fittedRect({...previous.labelRect, x: mark.labelRect.x, y: mark.labelRect.y});
        } else {
          mark.labelRect = fittedRect(mark.labelRect);
        }
        if (JSON.stringify(previous) !== JSON.stringify(mark)) {
          history.replace(current.index, mark); syncMarks();
        }
        selectMark(current.index);
        return;
      }
      const text = annotationTextForCommit(current.text);
      const frame = current.rect;
      const insets = textEditorInsets(current.uiScale);
      const textRect: Rect = {
        x: frame.x + insets.x,
        y: frame.y + insets.y,
        width: Math.max(current.labelDirection ? .1 : 1, frame.width - 2*insets.x),
        height: Math.max(current.labelDirection ? .1 : 1, frame.height - 2*insets.y),
      };
      if (text === null) {
        if (current.index !== null) {
          history.remove(current.index);
          selectMark(null);
          syncMarks();
        }
        return;
      }
      const previous =
        current.index !== null ? history.elements[current.index] : null;
      const newMark: AnnotationMark = {
        // Reuse the previous id so an unchanged edit compares equal and
        // does not create a no-op history entry (spec §6.6).
        id: previous && previous.kind === "text" ? previous.id : current.id,
        kind: "text",
        text,
        rect: textRect,
        color: current.color,
        background: current.background,
        fontSize: current.fontSize,
        ...(current.labelDirection?{labelDirection:current.labelDirection}:{}),
      };
      // Opening or flipping back an unchanged label retains its saved layout.
      // This also avoids an invisible history entry from padding round-off.
      if (previous?.kind === "text" && previous.labelDirection && previous.text === text &&
        previous.fontSize === current.fontSize && previous.labelDirection === current.labelDirection) newMark.rect = previous.rect;
      if (current.index !== null) {
        const unchanged =
          previous && previous.kind === "text"
            ? previous.text === newMark.text &&
              previous.rect.x === newMark.rect.x &&
              previous.rect.y === newMark.rect.y &&
              previous.rect.width === newMark.rect.width &&
              previous.rect.height === newMark.rect.height &&
              previous.color === newMark.color &&
              previous.background === newMark.background &&
              previous.fontSize === newMark.fontSize && previous.labelDirection === newMark.labelDirection
            : false;
        if (!unchanged) {
          history.replace(current.index, newMark);
          syncMarks();
        }
        selectMark(current.index);
      } else {
        appendMark(newMark);
      }
      // Spec §6.6: commit (unchanged edits do not write history). The
      // Return key additionally finishes the capture — handled in the
      // TextEditor's Enter branch so other commit triggers (tool switch,
      // undo, export) do not complete the capture.
    }, [history, publishHistory, syncMarks, appendMark, selectMark, validateEditingWatermark, documentSize.width, documentSize.height]);

    const editText=useCallback((index:number)=>{
      const mark=history.elements[index];if(!mark||mark.kind!=="text")return;
      const uiScale=hitTestScale.radial;
      const insets=textEditorInsets(uiScale);
      const width=mark.labelDirection ? mark.rect.width+2*insets.x : Math.max(1,Math.min(mark.rect.width+2*insets.x,documentSize.width));
      const height=mark.labelDirection ? mark.rect.height+2*insets.y : Math.max(1,Math.min(mark.rect.height+2*insets.y,documentSize.height));
      const next:EditingState={id:mark.id,index,text:mark.text,textOriginal:mark,uiScale,rect:{x:mark.labelDirection ? mark.rect.x-insets.x : Math.min(Math.max(0,mark.rect.x-insets.x),Math.max(0,documentSize.width-width)),
        y:mark.labelDirection ? mark.rect.y-insets.y : Math.min(Math.max(0,mark.rect.y-insets.y),Math.max(0,documentSize.height-height)),width,height},
        maxWidth:Math.max(width,documentSize.width-Math.max(0,mark.rect.x-insets.x)),color:mark.color,background:mark.background,fontSize:mark.fontSize,labelDirection:mark.labelDirection};
      editingRef.current=next;setEditing(next);selectMark(index);publishHistory();
    },[history,documentSize.width,documentSize.height,selectMark,publishHistory,hitTestScale.radial]);

    const startWatermark = useCallback((index: number | null, point?: Point) => {
      const current = editingRef.current;
      if (current?.watermark && (index === null || index === current.index)) {
        // A repeated toolbar/inspector action keeps the native input and undo
        // stack alive. Its mount-only focus effect does not run a second time.
        canvasRef.current?.parentElement?.querySelector<HTMLTextAreaElement>("textarea")?.focus();
        return;
      }
      if (index === null) {
        const selected = selectedIndexRef.current;
        if (selected !== null && history.elements[selected]?.kind === "watermark") index = selected;
        else for (let i = history.elements.length - 1; i >= 0; i--) {
          if (history.elements[i].kind === "watermark") { index = i; break; }
        }
      }
      const previous = index === null ? null : history.elements[index];
      if (previous && previous.kind !== "watermark") return;
      if (index !== null && !previous) return;
      const ap = appearanceRef.current, uiScale = hitTestScale.radial;
      const insets = textEditorInsets(uiScale);
      const width = Math.max(1, Math.min(180 * uiScale, documentSize.width - 2 * insets.x));
      const height = Math.max(1, Math.min(ap.watermarkFontSize * 1.25, documentSize.height - 2 * insets.y));
      const center = point ?? {x: documentSize.width / 2, y: documentSize.height / 2};
      const mark: WatermarkMark = previous ?? {kind: "watermark", id: Date.now() + Math.random(), text: "",
        rect: {x: center.x - width / 2, y: center.y - height / 2, width, height},
        color: ap.watermarkColor, fontSize: ap.watermarkFontSize, opacity: ap.watermarkOpacity / 100,
        rotation: ap.watermarkRotation, mode: "tiled", spacing: ap.watermarkSpacing};
      if (!validateMarks([...history.elements.filter((_, i) => i !== index), mark])) return;
      const frameWidth = Math.min(documentSize.width, mark.rect.width + 2 * insets.x);
      const frameHeight = Math.min(documentSize.height, mark.rect.height + 2 * insets.y);
      const next: EditingState = {id: mark.id, index, watermark: mark, watermarkOriginal: mark,
        text: mark.text, uiScale, maxWidth: documentSize.width, color: mark.color,
        background: "transparent", fontSize: mark.fontSize, rect: {
          x: Math.max(0, Math.min(documentSize.width - frameWidth, mark.rect.x - insets.x)),
          y: Math.max(0, Math.min(documentSize.height - frameHeight, mark.rect.y - insets.y)),
          width: frameWidth, height: frameHeight,
        }};
      editingRef.current = next; setEditing(next); selectMark(index); publishHistory();
      // Tool actions may commit and reopen the same id in one React batch.
      // That preserves this native textarea, so restore focus explicitly too.
      canvasRef.current?.parentElement?.querySelector<HTMLTextAreaElement>("textarea")?.focus();
    }, [history, documentSize.width, documentSize.height, hitTestScale.radial, validateMarks, selectMark, publishHistory]);

    const fitLabelRef = useRef<(mark:AnnotationMark)=>AnnotationMark>(mark=>mark);
    const styleAdjustment=useRef<{index:number;original:AnnotationMark}|null>(null);
    const fitCallout = useCallback((mark: CalloutMark): CalloutMark => {
      const context = canvasRef.current?.getContext("2d");
      const size = Math.min(mark.size, documentSize.width, documentSize.height);
      const center = {x: Math.max(size / 2, Math.min(documentSize.width - size / 2, mark.center.x)),
        y: Math.max(size / 2, Math.min(documentSize.height - size / 2, mark.center.y))};
      if (!mark.text.trim() || !context) return {...mark, size, center};
      let fontSize = mark.fontSize;
      let width = 1, height = 1;
      context.save();
      for (let attempt = 0; attempt < 32; attempt++) {
        context.font = textFont(fontSize);
        ({width, height} = calloutLabelSize({text: mark.text, fontSize, uiScale: hitTestScale.radial,
          boundsWidth: documentSize.width, measureText: value => context.measureText(value).width}));
        if (height <= documentSize.height || fontSize <= .1) break;
        fontSize *= .8;
      }
      context.restore();
      height = Math.min(height, documentSize.height);
      return {...mark, size, center, fontSize, labelRect: {
        x: Math.max(0, Math.min(documentSize.width - width, mark.labelRect.x)),
        y: Math.max(0, Math.min(documentSize.height - height, mark.labelRect.y)), width, height}};
    }, [documentSize.width, documentSize.height, hitTestScale.radial]);
    const createCallout = useCallback((start: Point, end: Point, id: number): CalloutMark => {
      const ap = appearanceRef.current;
      const size = Math.min(ap.calloutSize, documentSize.width, documentSize.height);
      const dragged = Math.hypot(end.x - start.x, end.y - start.y) >= 12 * hitTestScale.radial;
      const width = Math.min(160, documentSize.width), height = Math.min(ap.textFontSize * 2.25, documentSize.height);
      const gap = 32 * hitTestScale.radial;
      const right = dragged ? end.x >= start.x : start.x + size / 2 + gap + width <= documentSize.width;
      const anchor = dragged ? end : {x: start.x + (right ? 1 : -1) * (size / 2 + gap), y: start.y};
      return separateCalloutLabel(fitCallout({kind: "callout", id, number: calloutNumberRef.current, text: "", center: start,
        labelRect: {x: Math.max(0, Math.min(documentSize.width - width, right ? anchor.x : anchor.x - width)),
          y: Math.max(0, Math.min(documentSize.height - height, anchor.y - height / 2)), width, height},
        color: ap.colorPreset, size, fontSize: ap.textFontSize, style: ap.calloutStyle}), documentSize, gap);
    }, [documentSize.width, documentSize.height, hitTestScale.radial, fitCallout]);
    const editCallout = useCallback((index: number) => {
      const mark = history.elements[index]; if (!mark || mark.kind !== "callout") return;
      // Opening a saved note preserves the user's placement. Only creation
      // and automatic size changes need an initial spacing correction.
      const context = canvasRef.current?.getContext("2d");
      let rect = mark.labelRect;
      if (mark.text && context) {
        context.save(); context.font = textFont(mark.fontSize);
        const size = calloutLabelSize({text: mark.text, fontSize: mark.fontSize, width: rect.width,
          boundsWidth: documentSize.width, measureText: value => context.measureText(value).width});
        context.restore(); rect = repairCalloutLabelHeight(rect, size, documentSize.height);
      }
      const note = mark;
      const next: EditingState = {id: note.id, index, callout: note, text: note.text, rect,
        maxWidth: Math.min(280, documentSize.width), uiScale: hitTestScale.radial,
        color: note.color, background: "transparent", fontSize: note.fontSize};
      editingRef.current = next; setEditing(next); selectMark(index); publishHistory();
    }, [history, documentSize.width, documentSize.height, hitTestScale.radial, selectMark, publishHistory]);
    const finishAppearanceAdjustment=useCallback(()=>{
      const adjustment=styleAdjustment.current;styleAdjustment.current=null;if(!adjustment)return;
      if(JSON.stringify(history.elements[adjustment.index])!==JSON.stringify(adjustment.original)){
        history.commitOverwrite(adjustment.index,adjustment.original);syncMarks();
      }
    },[history,syncMarks]);
    const updateSelectedCallout = useCallback((patch: Partial<Omit<CalloutMark, "kind" | "id">>, transient = false) => {
      if (interactionsDisabled()) return;
      const editing = editingRef.current;
      if (editing?.callout) {
        const previous = editingCalloutMark(editing);
        const mark = resizeCalloutLabel(fitCallout({...previous, ...patch}), previous,
          documentSize, 32 * editing.uiScale);
        const next = {...editing, callout: mark, text: mark.text, rect: mark.labelRect, color: mark.color, fontSize: mark.fontSize};
        editingRef.current = next; setEditing(next); return;
      }
      const index = selectedIndexRef.current;
      const mark = index === null ? null : history.elements[index];
      if (index === null || !mark || mark.kind !== "callout") return;
      styleAdjustment.current ??= {index, original: mark};
      const next = resizeCalloutLabel(fitCallout({...mark, ...patch}), mark,
        documentSize, 32 * hitTestScale.radial);
      const elements = history.elements.slice(); elements[index] = next;
      history.overwrite(elements); setMarks(elements);
      documentChangeRef.current?.(elements);
      if (!transient) finishAppearanceAdjustment();
    }, [history, interactionsDisabled, fitCallout, finishAppearanceAdjustment,
      documentSize.width, documentSize.height, hitTestScale.radial]);

    const updateSelectionAppearance=useCallback((patch:Partial<AppearanceSettings>,transient=false)=>{
      if(interactionsDisabled())return;
      const editing=editingRef.current;
      if(editing){
        if (editing.watermark) {
          if (!Object.entries(patch).some(([key, value]) => key.startsWith("watermark") && value !== undefined)) return;
          const watermarkPatch = {...patch, watermarkMode: "tiled" as const};
          const mark = applyAnnotationAppearance(editingWatermarkMark(editing), watermarkPatch) as WatermarkMark;
          const next = {...editing, watermark: mark, color: mark.color, fontSize: mark.fontSize};
          if (!validateEditingWatermark(next)) return;
          editingRef.current = next; setEditing(next); return;
        }
        if (editing.callout) {
          const mark = applyAnnotationAppearance(editingCalloutMark(editing), patch) as CalloutMark;
          updateSelectedCallout({size: mark.size, style: mark.style, color: mark.color, fontSize: mark.fontSize}, transient);
          return;
        }
        if (editing.labelDirection) {
          const mark = fitLabelRef.current(applyAnnotationAppearance(editingTextMark(editing), patch)) as Extract<AnnotationMark, {kind: "text"}>;
          const next = {...editingWithLabel(editing, mark), background: mark.background};
          editingRef.current = next; setEditing(next); return;
        }
        const next={...editing,color:patch.colorPreset??editing.color,background:patch.textBackgroundStyle??editing.background,fontSize:patch.textFontSize??editing.fontSize,
          ...(editing.labelDirection?{labelDirection:patch.labelDirection??editing.labelDirection}: {})};
        editingRef.current=next;setEditing(next);return;
      }
      const index=selectedIndexRef.current;if(index===null)return;
      const mark=history.elements[index];if(!mark)return;
      if (mark.kind === "watermark" && !Object.entries(patch).some(([key, value]) => key.startsWith("watermark") && value !== undefined)) return;
      styleAdjustment.current??={index,original:mark};
      const watermarkPatch = mark.kind === "watermark" && Object.keys(patch).some(key => key.startsWith("watermark"))
        ? {...patch, watermarkMode: "tiled" as const} : patch;
      const updated=applyAnnotationAppearance(mark,watermarkPatch);
      const next=updated.kind === "callout" && mark.kind === "callout"
        ? resizeCalloutLabel(fitCallout(updated), mark, documentSize, 32 * hitTestScale.radial)
        : updated.kind === "text" && updated.labelDirection ? fitLabelRef.current(updated) : updated;
      const elements=history.elements.slice();elements[index]=next;
      if (!validateMarks(elements)) return;
      history.overwrite(elements);setMarks(elements);
      if(!transient)finishAppearanceAdjustment();
    },[history,interactionsDisabled,finishAppearanceAdjustment,fitCallout,documentSize.width,documentSize.height,hitTestScale.radial,validateMarks,validateEditingWatermark,updateSelectedCallout]);

    const toggleLabel = useCallback((id: number) => {
      if (interactionsDisabled() || interactionRef.current.kind !== "none") return;
      blankDoubleClickRef.current = false;
      const current = editingRef.current;
      if (current?.id === id && current.labelDirection) {
        const mark = fitLabelRef.current(applyAnnotationAppearance(editingTextMark(current), {
          labelDirection: current.labelDirection === "left" ? "right" : "left",
        })) as Extract<AnnotationMark, {kind: "text"}>;
        const next = editingWithLabel(current, mark);
        editingRef.current = next; setEditing(next); return;
      }
      commitText(); finishAppearanceAdjustment();
      const index = history.elements.findIndex(mark => mark.id === id);
      const mark = history.elements[index];
      if (!mark || mark.kind !== "text" || !mark.labelDirection) return;
      history.replace(index, fitLabelRef.current(applyAnnotationAppearance(mark, {
        labelDirection: mark.labelDirection === "left" ? "right" : "left",
      })));
      selectMark(index); syncMarks();
    }, [history, interactionsDisabled, commitText, finishAppearanceAdjustment, selectMark, syncMarks]);

    const cancelInteraction=useCallback(()=>{
      if(editingRef.current){editingRef.current=null;setEditing(null);publishHistory();return true;}
      if(interactionRef.current.kind!=="none"){
        interactionRef.current={kind:"none"};setDraft(null);setSelectCursor("default");return true;
      }
      return false;
    },[publishHistory]);

    // Switching tools while a text edit is open should commit it (the text
    // becomes a mark and the editor closes), matching the canvas click
    // behavior. Without this the textarea stays up after choosing another
    // tool.
    const prevTool = useRef(tool);
    useEffect(() => {
      if (commitTextOnToolChange && prevTool.current !== tool && editingRef.current &&
        !(tool === "watermark" && editingRef.current.watermark)) {
        commitText();
      }
      prevTool.current = tool;
    }, [tool, commitText, commitTextOnToolChange]);

    const onPointerDown = useCallback(
      (e: React.PointerEvent) => {
        if (interactionsDisabled()) return;
        if(e.button!==0)return;
        canvasClickRef.current = {
          start: { x: e.clientX, y: e.clientY }, moved: false, wasEditing: editingRef.current !== null,
          editingCalloutId: editingRef.current?.callout ? editingRef.current.id : null,
        };
        if (canvasClickRef.current.editingCalloutId !== calloutClickRef.current?.id) calloutClickRef.current = null;
        finishAppearanceAdjustment();
        const canvas = canvasRef.current!;
        gestureRectRef.current = canvas.getBoundingClientRect();
        canvas.setPointerCapture(e.pointerId);
        const p = clampPoint(toPoint(e), {
          x: 0,
          y: 0,
          width: documentSize.width,
          height: documentSize.height,
        });
        const t = toolRef.current;
        const ap = appearanceRef.current;

        if (t === "watermark") {
          const hit = markIndexAt(history.elements, p, hitTestScale);
          const watermarkIndex = hit !== null && history.elements[hit].kind === "watermark" ? hit : null;
          if (editingRef.current?.watermark && (watermarkIndex === null || watermarkIndex === editingRef.current.index)) {
            startWatermark(watermarkIndex, p);
            return;
          }
          const watermarkId = watermarkIndex === null ? null : history.elements[watermarkIndex].id;
          if (editingRef.current) commitText();
          if (editingRef.current) return; // A rejected edit must remain visible.
          const index = watermarkId === null ? null : history.elements.findIndex(mark => mark.id === watermarkId);
          startWatermark(index === -1 ? null : index, p);
          return;
        }

        if (editingRef.current) {
          commitText();
        }

        const current = history.elements;
        const selectedLine = selectedIndex === null ? null : current[selectedIndex];
        const textTool = t === "text" || t === "label";
        const textHit = textTool ? markIndexAt(current, p, hitTestScale) : null;
        const editingExistingText = textTool && (
          (textHit !== null && current[textHit].kind === "text") ||
          (selectedLine?.kind === "text" && hitTestHandle(p, selectionBounds(selectedLine), 9 * hitTestScale.radial) !== null)
        );
        if (textTool && !editingExistingText) {
          const width = Math.max(1, Math.min(180*hitTestScale.radial, documentSize.width));
          const height = Math.max(1, Math.min(34*hitTestScale.radial, documentSize.height));
          const frame: Rect = {
            x: Math.min(Math.max(0, p.x), Math.max(0, documentSize.width - width)),
            y: Math.min(Math.max(0, p.y), Math.max(0, documentSize.height - height)),
            width,
            height,
          };
          const nextEditing: EditingState = {
            id: Date.now() + Math.random(),
            // Blank text-tool presses create a new mark; a stale selection
            // must not replace an existing mark. Body presses use the same
            // move/resize path as Select, and double-click reopens the editor.
            index: null,
            uiScale:hitTestScale.radial,
            text: "",
            rect: frame,
            maxWidth: Math.max(width,documentSize.width-frame.x),
            color: ap.colorPreset,
            background: ap.textBackgroundStyle,
            fontSize: t === "label" ? Math.min(ap.textFontSize, documentSize.width / 8, documentSize.height / 3) : ap.textFontSize,
            ...(t === "label" ? {labelDirection: ap.labelDirection} : {}),
          };
          editingRef.current = nextEditing;
          setEditing(nextEditing);
          selectMark(null);
          publishHistory();
          return;
        }

        const editingSelectedLine =
          (t === "line" || t === "arrow") &&
          selectedLine?.kind === t &&
          (Math.hypot(p.x - selectedLine.start.x, p.y - selectedLine.start.y) <= 10 * hitTestScale.radial ||
            Math.hypot(p.x - selectedLine.end.x, p.y - selectedLine.end.y) <= 10 * hitTestScale.radial ||
            markIndexAt(current, p, hitTestScale) === selectedIndex);
        const calloutHit = t === "callout" ? markIndexAt(current, p, hitTestScale) : null;
        const editingCallout = (calloutHit !== null && current[calloutHit].kind === "callout") ||
          (t === "callout" && selectedLine?.kind === "callout" && calloutHandleAt(selectedLine, p, 9 * hitTestScale.radial) !== null);
        if (t === "select" || editingSelectedLine || editingCallout || editingExistingText) {
          let handleInteraction: string | null = null;
          const selectedMark = selectedIndex === null || (textTool && selectedLine?.kind !== "text") ? null : current[selectedIndex];
          if (selectedMark?.kind === "callout") {
            handleInteraction = calloutHandleAt(selectedMark, p, 9 * hitTestScale.radial);
          } else if (selectedMark && !["line","arrow"].includes(selectedMark.kind)) {
            handleInteraction = hitTestHandle(
              p,
              selectionBounds(selectedMark),
              9 * hitTestScale.radial,
            );
          } else if (selectedMark) {
            const mark = selectedMark;
            if (mark && (mark.kind === "line" || mark.kind === "arrow")) {
              if (
                Math.hypot(p.x - mark.start.x, p.y - mark.start.y) <=
                10 * hitTestScale.radial
              ) {
                handleInteraction = "start";
              } else if (
                Math.hypot(p.x - mark.end.x, p.y - mark.end.y) <=
                10 * hitTestScale.radial
              ) {
                handleInteraction = "end";
              }
            }
          }
          const index = handleInteraction
            ? selectedIndex
            : markIndexAt(current, p, hitTestScale);
          if (index === null || index === undefined) {
            selectMark(null);
            interactionRef.current = { kind: "none" };
            redraw();
            return;
          }
          const mark = current[index];
          if (t === "select" && mark.kind === "text" && e.detail >= 2) {editText(index);return;}
          if (!handleInteraction && mark.kind === "callout") handleInteraction = calloutPartAt(mark, p);
          selectMark(index);
          if (handleInteraction === "start" || handleInteraction === "end") {
            interactionRef.current = {
              kind: "endpoint",
              index,
              original: mark,
              isStart: handleInteraction === "start",
              start: p,
            };
          } else if (handleInteraction) {
            interactionRef.current = { kind: "resize", index, original: mark, handle: handleInteraction, start: p };
          } else {
            interactionRef.current = { kind: "move", index, original: mark, start: p };
            // Spec §6.3: closedHand while dragging.
            setSelectCursor("grabbing");
          }
          redraw();
          return;
        }

        const points = [p];
        interactionRef.current = { kind: "draw", tool: t, start: p, points };
        if (t === "callout") {
          setDraft(createCallout(p, p, -1));
        } else if (t === "pen") {
          setDraft({ kind: "pen", id: -1, points, color: ap.colorPreset, width: ap.penWidth });
        } else if (t === "mosaic") {
          const mosaic: Extract<AnnotationMark, {kind: "mosaic"}> = {
            kind: "mosaic",
            id: -1,
            points:mosaicShapeRef.current==="brush"?points:[p,p],
            shape:mosaicShapeRef.current,
            brushDiameter: ap.mosaicBrushDiameter,
            intensity: ap.mosaicIntensity,
            style: ap.mosaicStyle,
          };
          interactionRef.current = {...interactionRef.current as Extract<Interaction, {kind: "draw"}>, mosaic};
          setDraft(mosaic);
        } else if (t === "rectangle") {
          setDraft({
            kind: "rectangle",
            id: -1,
            rect: { x: p.x, y: p.y, width: 0, height: 0 },
            color: ap.colorPreset,
            width: ap.shapeWidth,
          });
        } else {
          setDraft({ kind: t==="arrow"?"arrow":"line", id: -1, start: p, end: p, color: ap.colorPreset, width: ap.shapeWidth });
        }
      },
      [
        toPoint,
        documentSize.height,
        documentSize.width,
        selectedIndex,
        redraw,
        commitText,
        history,
        hitTestScale,
        publishHistory,editText,finishAppearanceAdjustment,createCallout,startWatermark,
      ],
    );

    // Native font metrics can change wrapping after even a uniform scale.
    // Use the same layout as rendering for preview, hit bounds and persistence.
    const fitTextBounds = useCallback((mark: AnnotationMark, handle?: string,
      original: AnnotationMark = mark): AnnotationMark => {
      if (mark.kind !== "text") return mark;
      if (mark.labelDirection) {
        const context = canvasRef.current?.getContext("2d");
        if (!context) return mark;
        context.save();
        try {
          return fitAnchoredLabel(mark, documentSize, (text, fontSize) => {
            context.font = textFont(fontSize); return context.measureText(text).width;
          });
        } finally { context.restore(); }
      }
      const context = canvasRef.current?.getContext("2d");
      if (!context) return mark;
      const measureHeight = (width: number, fontSize: number) => {
        context.font = textFont(fontSize);
        const lines = layoutTextLines(mark.text, width, text => context.measureText(text).width);
        return Math.max(1, Math.ceil(lines.length * fontSize * 1.25));
      };
      context.save();
      let {width} = mark.rect;
      let {fontSize} = mark;
      let height = measureHeight(width, fontSize);
      if (!handle) {
        context.restore();
        return {...mark, rect: {...mark.rect, height}};
      }
      const left = handle.includes("Left") || handle === "left";
      const right = handle.includes("Right") || handle === "right";
      const top = handle.startsWith("top");
      const bottom = handle.startsWith("bottom");
      const anchorX = left ? mark.rect.x + width : right ? mark.rect.x : mark.rect.x + width / 2;
      const anchorY = top ? mark.rect.y + mark.rect.height : bottom ? mark.rect.y : mark.rect.y + mark.rect.height / 2;
      const room = Math.max(0, top ? anchorY : bottom ? documentSize.height - anchorY :
        2 * Math.min(anchorY, documentSize.height - anchorY));
      if (room < 1) { context.restore(); return original; }
      // A wrap threshold can add lines even when width/font scale together.
      // Reduce the proposed scale against measured height, preserving its fixed
      // edge/center. Never accept a frame that extends beyond that anchored room.
      for (let attempt = 0; height > room && attempt < 12; attempt++) {
        const factor = Math.min(.99, room / height);
        width *= factor;
        fontSize *= factor;
        height = measureHeight(width, fontSize);
      }
      context.restore();
      if (height > room) return original;
      return {...mark, fontSize, rect: {
        x: left ? anchorX - width : right ? anchorX : anchorX - width / 2,
        y: top ? anchorY - height : bottom ? anchorY : anchorY - height / 2,
        width, height,
      }};
    }, [documentSize.height, documentSize.width]);

    fitLabelRef.current = fitTextBounds;

    const onPointerMove = useCallback(
      (e: React.PointerEvent) => {
        if (Math.hypot(e.clientX - canvasClickRef.current.start.x,
          e.clientY - canvasClickRef.current.start.y) >= 3) canvasClickRef.current.moved = true;
        if (interactionsDisabled()) return;
        const p = clampPoint(toPoint(e), {
          x: 0,
          y: 0,
          width: documentSize.width,
          height: documentSize.height,
        });
        const interaction = interactionRef.current;
        if (interaction.kind === "none") {
          if (toolRef.current === "mosaic" && mosaicShapeRef.current==="brush") setBrushCursor(p);
          else if (brushCursorRef.current) setBrushCursor(null);
          const textTool = toolRef.current === "text" || toolRef.current === "label";
          if (toolRef.current === "select" || toolRef.current === "line" || toolRef.current === "arrow" || textTool) {
            // Spec §6.7: handle → crosshair, over a mark → open hand,
            // otherwise arrow.
            const current = history.elements;
            const selected = selectedIndexRef.current;
            let cursor = "default";
            const selectedMark = selected === null ? null : current[selected];
            const activeLine = selectedMark &&
              (selectedMark.kind === "line" || selectedMark.kind === "arrow") &&
              (toolRef.current === "select" || toolRef.current === selectedMark.kind);
            if (activeLine &&
              (Math.hypot(p.x - selectedMark.start.x, p.y - selectedMark.start.y) <= 10 * hitTestScale.radial ||
                Math.hypot(p.x - selectedMark.end.x, p.y - selectedMark.end.y) <= 10 * hitTestScale.radial)) {
              cursor = "crosshair";
            }
            if (toolRef.current === "select" && selectedMark?.kind === "callout") {
              if (calloutHandleAt(selectedMark, p, 9 * hitTestScale.radial)) cursor = "crosshair";
            } else if ((toolRef.current === "select" || (textTool && selectedMark?.kind === "text")) &&
              selected !== null && current[selected] && !["line","arrow"].includes(current[selected].kind)) {
              if (
                hitTestHandle(
                  p,
                  selectionBounds(current[selected]),
                  9 * hitTestScale.radial,
                )
              ) {
                cursor = "crosshair";
              }
            }
            if (
              cursor === "default" &&
              (toolRef.current === "select" || activeLine || textTool) &&
              (textTool
                ? current[markIndexAt(current, p, hitTestScale) ?? -1]?.kind === "text"
                : toolRef.current === "select"
                ? markIndexAt(current, p, hitTestScale) !== null
                : markIndexAt(current, p, hitTestScale) === selected)
            ) {
              cursor = "grab";
            }
            setSelectCursor(cursor);
          }
          return;
        }
        if (interaction.kind === "draw") {
          const t = interaction.tool;
          // Spec §7.4: the brush cursor tracks the drag point while drawing.
          if (t === "mosaic" && interaction.mosaic?.shape === "brush") setBrushCursor(p);
          if (t === "callout") {
            setDraft(createCallout(interaction.start, p, -1));
          } else if (t === "pen" || t === "mosaic") {
            const points = interaction.points;
            const last = points[points.length - 1];
            if (Math.hypot(p.x - last.x, p.y - last.y) >= 0.5) points.push(p);
            if (t === "pen") {
              setDraft({
                kind: "pen",
                id: -1,
                points: [...points],
                color: appearanceRef.current.colorPreset,
                width: appearanceRef.current.penWidth,
              });
            } else {
              const mosaic = interaction.mosaic!;
              setDraft({...mosaic, points: mosaic.shape === "brush" ? [...points] : [interaction.start, p]});
            }
          } else {
            const start = interaction.start;
            if (t === "rectangle") {
              setDraft({
                kind: "rectangle",
                id: -1,
                rect: {
                  x: Math.min(start.x, p.x),
                  y: Math.min(start.y, p.y),
                  width: Math.abs(p.x - start.x),
                  height: Math.abs(p.y - start.y),
                },
                color: appearanceRef.current.colorPreset,
                width: appearanceRef.current.shapeWidth,
              });
            } else {
              setDraft({
                kind: t==="arrow"?"arrow":"line",
                id: -1,
                start,
                end: p,
                color: appearanceRef.current.colorPreset,
                width: appearanceRef.current.shapeWidth,
              });
            }
          }
          return;
        }
        if (interaction.kind === "move") {
          if (interaction.original.kind === "callout" && !canvasClickRef.current.moved) return;
          const by = { x: p.x - interaction.start.x, y: p.y - interaction.start.y };
          setValidatedDraft(
            translateMark(interaction.original, by, {
              x: 0,
              y: 0,
              width: documentSize.width,
              height: documentSize.height,
            }),
          );
          return;
        }
        if (interaction.kind === "resize") {
          if (interaction.original.kind === "callout" && !canvasClickRef.current.moved) return;
          const resized = dragAnnotationHandle(interaction.original, interaction.handle,
            {x: p.x - interaction.start.x, y: p.y - interaction.start.y},
            {x: 0, y: 0, width: documentSize.width, height: documentSize.height});
          setValidatedDraft(resized === interaction.original ? resized :
            fitTextBounds(resized, interaction.handle, interaction.original));
          return;
        }
        if (interaction.kind === "endpoint") {
          setDraft(dragAnnotationHandle(interaction.original, interaction.isStart ? "start" : "end",
            { x: p.x-interaction.start.x, y: p.y-interaction.start.y }, {x:0,y:0,width:documentSize.width,height:documentSize.height}));
        }
      },
      [toPoint, documentSize.height, documentSize.width, history, hitTestScale, fitTextBounds, createCallout, setValidatedDraft],
    );

    const onPointerUp = useCallback(
      (e: React.PointerEvent) => {
        if (interactionsDisabled()) return;
        const p = clampPoint(toPoint(e), {
          x: 0,
          y: 0,
          width: documentSize.width,
          height: documentSize.height,
        });
        const interaction = interactionRef.current;
        interactionRef.current = { kind: "none" };
        if (interaction.kind === "move") setSelectCursor("default");

        if (interaction.kind === "draw") {
          const t = interaction.tool;
          const ap = appearanceRef.current;
          if (t === "callout") {
            appendMark(createCallout(interaction.start, p, Date.now() + Math.random()));
            editCallout(history.elements.length - 1);
          } else if (t === "pen") {
            const points = interaction.points;
            const last = points[points.length - 1];
            if (Math.hypot(p.x - last.x, p.y - last.y) >= 0.5) points.push(p);
            if (points.length > 0) {
              appendMark({
                kind: "pen",
                id: Date.now() + Math.random(),
                points: [...points],
                color: ap.colorPreset,
                width: ap.penWidth,
              });
            }
          } else if (t === "mosaic") {
            const mosaic = interaction.mosaic!;
            const points = interaction.points;
            const last = points[points.length - 1];
            if (Math.hypot(p.x - last.x, p.y - last.y) >= 0.5) points.push(p);
            if(mosaic.shape!=="brush"&&(Math.abs(p.x-interaction.start.x)<1||Math.abs(p.y-interaction.start.y)<1)){setDraft(null);return;}
            appendMark({
              ...mosaic,
              id: Date.now() + Math.random(),
              points: mosaic.shape === "brush" ? [...points] : [interaction.start, p],
            });
          } else if (t === "rectangle") {
            const start = interaction.start;
            if(Math.abs(p.x-start.x)<1||Math.abs(p.y-start.y)<1){setDraft(null);return;}
            appendMark({
              kind: "rectangle",
              id: Date.now() + Math.random(),
              rect: {
                x: Math.min(start.x, p.x),
                y: Math.min(start.y, p.y),
                width: Math.abs(p.x - start.x),
                height: Math.abs(p.y - start.y),
              },
              color: ap.colorPreset,
              width: ap.shapeWidth,
            });
          } else {
            const start = interaction.start;
            if (Math.hypot(p.x - start.x, p.y - start.y) >= 3) {
              appendMark({
                kind: t === "arrow" ? "arrow" : "line",
                id: Date.now() + Math.random(),
                start,
                end: p,
                color: ap.colorPreset,
                width: ap.shapeWidth,
              });
            }
          }
          setDraft(null);
          return;
        }

        if (
          interaction.kind === "move" ||
          interaction.kind === "resize" ||
          interaction.kind === "endpoint"
        ) {
          const bounds={x:0,y:0,width:documentSize.width,height:documentSize.height};
          let preview=interaction.kind==="move"?translateMark(interaction.original,{x:p.x-interaction.start.x,y:p.y-interaction.start.y},bounds):
            dragAnnotationHandle(interaction.original,interaction.kind==="resize"?interaction.handle:interaction.isStart?"start":"end",
              {x:p.x-interaction.start.x,y:p.y-interaction.start.y},bounds);
          if (interaction.kind === "resize" && preview !== interaction.original) {
            preview = fitTextBounds(preview, interaction.handle, interaction.original);
          }
          // Spec §6.3: only commit a drag when it actually changed the
          // mark (≥1pt of movement) — a click without movement must not
          // write a no-op history entry.
          const changed =
            (interaction.original.kind === "callout"
              ? canvasClickRef.current.moved || Math.hypot(e.clientX - canvasClickRef.current.start.x, e.clientY - canvasClickRef.current.start.y) >= 3
              : Math.hypot(p.x - interaction.start.x, p.y - interaction.start.y) >= 1) &&
            preview !== null && JSON.stringify(preview) !== JSON.stringify(interaction.original);
          if (preview && changed) {
            if (preview.kind === "watermark" && !validateMarks([
              ...history.elements.filter((_, index) => index !== interaction.index), preview,
            ])) { setDraft(null); return; }
            history.replace(interaction.index, preview);
            syncMarks();
          }
          setDraft(null);
          if (preview?.kind === "callout" && !changed) {
            if (toolRef.current === "select" && gestureRectRef.current) {
              const {left, top, width, height} = gestureRectRef.current;
              calloutClickRef.current = {id: preview.id, frame: {left, top, width, height}};
            }
            editCallout(interaction.index);
          }
        }
        redraw();
      },
      [
        toPoint,
        documentSize.height,
        documentSize.width,
        redraw,
        syncMarks,
        history,appendMark,fitTextBounds,createCallout,editCallout,validateMarks,
      ],
    );

    // Keyboard shortcuts (overlay-level keys are handled by the parent).
    useEffect(() => {
      const onKeyDown = (e: KeyboardEvent) => {
        const canvasWindow = window as unknown as { __kiriOverlay?: boolean };
        if (!canvasWindow.__kiriOverlay) return;
        if (interactionsDisabled()) return;
        if (editingRef.current) return;
        if (e.key === "Delete" || e.key === "Backspace") {
          if (toolRef.current === "select" && selectedIndexRef.current !== null) {
            history.remove(selectedIndexRef.current);
            selectMark(null);
            syncMarks();
          }
        }
      };
      window.addEventListener("keydown", onKeyDown);
      return () => window.removeEventListener("keydown", onKeyDown);
    }, [history, interactionsDisabled, syncMarks]);

    const selectedIndexRef = useRef<number | null>(null);
    useEffect(() => {
      selectedIndexRef.current = selectedIndex;
    }, [selectedIndex]);

    const undoRef = useRef<() => void>(() => {});
    const redoRef = useRef<() => void>(() => {});
    const deleteRef = useRef<() => void>(() => {});
    undoRef.current = () => {
      if (interactionsDisabled()) return;
      commitText();
      finishAppearanceAdjustment();
      if (onUndo) { onUndo(); return; }
      history.undo();
      selectMark(null);
      syncMarks();
    };
    redoRef.current = () => {
      if (interactionsDisabled()) return;
      commitText();
      finishAppearanceAdjustment();
      if (onRedo) { onRedo(); return; }
      history.redo();
      selectMark(null);
      syncMarks();
    };
    deleteRef.current = () => {
      if (interactionsDisabled()) return;
      if (selectedIndexRef.current === null) return;
      finishAppearanceAdjustment();
      history.remove(selectedIndexRef.current);
      selectMark(null);
      syncMarks();
    };

    // Live text font-size adjustment (spec §6.6): begin records the selected
    // text mark; set applies a preview without touching history; end commits
    // a single replace entry if the size actually changed.
    const fontAdjustRef = useRef<{
      index: number;
      original: Extract<AnnotationMark, { kind: "text" }>;
    } | null>(null);
    const beginFontAdjustRef = useRef<() => void>(() => {});
    const setFontLiveRef = useRef<(value: number) => void>(() => {});
    const endFontAdjustRef = useRef<() => void>(() => {});
    beginFontAdjustRef.current = () => {
      if (interactionsDisabled()) return;
      if (fontAdjustRef.current) return;
      commitText();
      const index = selectedIndexRef.current;
      const mark = index !== null ? history.elements[index] : undefined;
      if (index !== null && mark && mark.kind === "text") {
        fontAdjustRef.current = { index, original: mark };
      } else {
        fontAdjustRef.current = null;
      }
    };
    setFontLiveRef.current = (value: number) => {
      if (interactionsDisabled()) return;
      // Range inputs also change through keyboard and accessibility actions.
      if (!fontAdjustRef.current) beginFontAdjustRef.current();
      const adjust = fontAdjustRef.current;
      if (!adjust) return;
      const mark = history.elements[adjust.index];
      if (!mark || mark.kind !== "text") return;
      const updated = fitTextBounds(applyAnnotationAppearance(adjust.original, { textFontSize: value }));
      // Preview: swap the element without recording history.
      const before = history.elements.slice();
      before[adjust.index] = updated;
      history.overwrite(before);
      // Keep live slider frames local; external history gets one final commit.
      setMarks(history.elements.slice());
      publishHistory();
    };
    const finishFontAdjustment = useCallback(() => {
      const adjust = fontAdjustRef.current;
      fontAdjustRef.current = null;
      if (!adjust) return;
      const mark = history.elements[adjust.index];
      if (mark && mark.kind === "text" &&
          JSON.stringify(mark) !== JSON.stringify(adjust.original)) {
        history.commitOverwrite(adjust.index, adjust.original);
        syncMarks();
      }
    }, [history, syncMarks]);
    endFontAdjustRef.current = () => {
      if (interactionsDisabled()) return;
      finishFontAdjustment();
    };

    const exportResult = useCallback(async (cropSelection?: Rect): Promise<AnnotationExportResult | null> => {
      const img = imageRef.current;
      if (!img) return null;
      if (!img.complete) {
        try {
          await img.decode();
        } catch {
          return null;
        }
      }
      const sourceImage = getSourceImage();
      if (!sourceImage) return null;

      // This is intentionally synchronous: the PNG and sidecar below must be
      // derived from the exact same committed text/mark snapshot.
      finishFontAdjustment();
      finishAppearanceAdjustment();
      commitText();
      if (editingRef.current) return null;
      const scaleX =
        sourceImage.naturalWidth / (displaySize?.width ?? documentSize.width);
      const scaleY =
        sourceImage.naturalHeight / (displaySize?.height ?? documentSize.height);
      const derivedSourcePixels = {
        width: Math.max(1, Math.round(documentSize.width * scaleX)),
        height: Math.max(1, Math.round(documentSize.height * scaleY)),
      };
      if (
        initialProject &&
        (initialProject.sourcePixels.width !== derivedSourcePixels.width ||
          initialProject.sourcePixels.height !== derivedSourcePixels.height)
      ) {
        return null;
      }

      let project: AnnotationDocumentV1;
      try {
        project = parseAnnotationDocument({
          schemaVersion: 1,
          canvas: documentSize,
          sourcePixels: initialProject?.sourcePixels ?? derivedSourcePixels,
          marks: history.elements,
        });
      } catch (error) {
        errorRef.current?.(t(error instanceof Error ? error.message : "Watermark is too dense. Increase its size or spacing."));
        return null;
      }
      let sourceCrop: Rect;
      try {
        sourceCrop = annotationSourceCrop(
          { width: sourceImage.naturalWidth, height: sourceImage.naturalHeight },
          displaySize ?? documentSize,
          region,
          project.sourcePixels,
        );
      } catch {
        return null;
      }
      const exportScaleX = project.sourcePixels.width / documentSize.width;
      const exportScaleY = project.sourcePixels.height / documentSize.height;

      let cropPixels: CropPixels | null = null;
      let exportSource: CanvasImageSource = sourceImage;
      let croppedSource: HTMLCanvasElement | null = null;
      if (cropSelection && !isFullCrop(project, cropSelection)) {
        const cropped = cropAnnotationDocument(project, cropSelection);
        cropPixels = cropped.cropPixels;
        project = cropped.document;
        // Re-render from exactly the clean source that Rust will persist.
        // Mosaic sampling at the cropped edge must match a later reopen.
        croppedSource = document.createElement("canvas");
        croppedSource.width = cropPixels.width;
        croppedSource.height = cropPixels.height;
        const sourceContext = croppedSource.getContext("2d");
        if (!sourceContext) return null;
        sourceContext.drawImage(sourceImage, sourceCrop.x+cropPixels.x, sourceCrop.y+cropPixels.y,
          cropPixels.width, cropPixels.height, 0, 0, cropPixels.width, cropPixels.height);
        exportSource = croppedSource;
      }

      const out = document.createElement("canvas");
      out.width = project.sourcePixels.width;
      out.height = project.sourcePixels.height;
      const ctx = out.getContext("2d");
      if (!ctx) {
        out.width = 0;
        out.height = 0;
        return null;
      }
      const context: RenderContext = {
        ctx,
        sourceImage: exportSource,
        sourceWidth: croppedSource?.width ?? sourceImage.naturalWidth,
        sourceHeight: croppedSource?.height ?? sourceImage.naturalHeight,
        sourceOffset: croppedSource ? { x: 0, y: 0 } : {
          x: sourceCrop.x / exportScaleX,
          y: sourceCrop.y / exportScaleY,
        },
        regionSize: { x: 0, y: 0, ...project.canvas },
        scaleX: exportScaleX,
        scaleY: exportScaleY,
        viewScaleX: 1,
        viewScaleY: 1,
        exporting: true,
      };
      try { renderAll(context, project.marks, {}); }
      catch (error) {
        out.width = 0; out.height = 0;
        if (croppedSource) { croppedSource.width = 0; croppedSource.height = 0; }
        errorRef.current?.(t(error instanceof Error ? error.message : "Watermark is too dense. Increase its size or spacing."));
        return null;
      }
      if (croppedSource) { croppedSource.width = 0; croppedSource.height = 0; }
      const blob = await new Promise<Blob | null>((resolve) =>
        out.toBlob(resolve, "image/png"),
      );
      if (!blob) {
        out.width = 0;
        out.height = 0;
        return null;
      }
      const png = new Uint8Array(await blob.arrayBuffer());
      // Release the large export backing store immediately instead of
      // waiting for a later garbage-collection cycle.
      out.width = 0;
      out.height = 0;
      return { png, document: project, cropPixels };
    }, [
      commitText,
      displaySize,
      documentSize.height,
      documentSize.width,
      getSourceImage,
      history,
      initialProject,
      finishFontAdjustment,
      finishAppearanceAdjustment,
      region.x,
      region.y,
    ]);

    useImperativeHandle(
      ref,
      () => ({
        undo: () => undoRef.current(),
        redo: () => redoRef.current(),
        clearAnnotations: () => {
          if (interactionsDisabled()) return;
          if (history.elements.length === 0 && !editingRef.current) return;
          // Spec §10.1: clear also discards an in-flight text edit.
          editingRef.current = null;
          setEditing(null);
          history.clear();
          selectMark(null);
          syncMarks();
        },
        deleteSelection: () => deleteRef.current(),
        commitTextEditing: () => {
          if (!interactionsDisabled()) commitText();
        },
        cancelTextEditing: () => {
          if (interactionsDisabled() || !editingRef.current) return false;
          return cancelInteraction();
        },
        editSelectedText:()=>{if(!interactionsDisabled()&&selectedIndexRef.current!==null){
          const index = selectedIndexRef.current;
          if(history.elements[index]?.kind==="callout")editCallout(index);
          else if (history.elements[index]?.kind === "watermark") startWatermark(index);
          else editText(index);
        }},
        editWatermark: () => {
          if (interactionsDisabled()) return;
          if (editingRef.current?.watermark) { startWatermark(editingRef.current.index); return; }
          commitText(); finishAppearanceAdjustment();
          if (editingRef.current) return;
          startWatermark(null);
        },
        clearSelection:()=>{if(!interactionsDisabled()){finishAppearanceAdjustment();selectMark(null);}},
        cancelInteraction,
        updateSelectionAppearance,
        updateSelectedCallout,
        finishAppearanceAdjustment,
        setMosaicShape:(shape)=>{
          if(interactionsDisabled())return;finishAppearanceAdjustment();
          const index=selectedIndexRef.current;if(index===null)return;
          const mark=history.elements[index];if(!mark||mark.kind!=="mosaic")return;
          const next=changeMosaicShape(mark,shape);if(next!==mark){history.replace(index,next);syncMarks();}
        },
        exportResult: (cropSelection) => exportResult(cropSelection),
        beginTextFontSizeAdjustment: () => beginFontAdjustRef.current(),
        setTextFontSizeLive: (value: number) => setFontLiveRef.current(value),
        endTextFontSizeAdjustment: () => endFontAdjustRef.current(),
      }),
      [commitText, exportResult, history, interactionsDisabled, syncMarks,editText,editCallout,startWatermark,selectMark,cancelInteraction,updateSelectionAppearance,updateSelectedCallout,finishAppearanceAdjustment],
    );

    const dotMarks = draft && marks.some(mark => mark.id === draft.id)
      ? marks.map(mark => mark.id === draft.id ? draft : mark) : marks;
    return (
      <div
        className="annotation-canvas-root"
        aria-busy={interactionDisabled}
        data-interaction-disabled={interactionDisabled || undefined}
        style={{
          position: "relative",
          width: view.width,
          height: view.height,
          pointerEvents: interactionDisabled ? "none" : "auto",
        }}
      >
        <canvas
          ref={canvasRef}
          width={Math.round(view.width * devicePixelRatio)}
          height={Math.round(view.height * devicePixelRatio)}
          style={{
            display: "block",
            width: view.width,
            height: view.height,
            // Spec §6.7: mosaic uses a crosshair, select tracks hover
            // (arrow/hand/crosshair), all other tools use the arrow.
            cursor:
              interactionDisabled
                ? "progress"
                : tool === "mosaic"
                ? "crosshair"
                : tool === "select" || tool === "text" || tool === "label"
                  ? selectCursor
                  : "default",
          }}
          onPointerDown={onPointerDown}
          onPointerMove={onPointerMove}
          onPointerUp={onPointerUp}
          onClick={(event) => {
            const point = toPoint(event.nativeEvent);
            const selected = selectedIndexRef.current === null ? null : history.elements[selectedIndexRef.current];
            const eligible = !interactionsDisabled() && toolRef.current === "select" &&
              !editingRef.current && !canvasClickRef.current.wasEditing && !canvasClickRef.current.moved &&
              markIndexAt(history.elements, point, hitTestScale) === null &&
              !(selected && hitTestHandle(point, selectionBounds(selected), 10 * hitTestScale.radial));
            blankDoubleClickRef.current = event.detail === 1
              ? eligible : blankDoubleClickRef.current && eligible;
          }}
          onDoubleClick={(event) => {
            const activeTool = toolRef.current;
            if (interactionsDisabled() || editingRef.current ||
              !["select", "text", "label"].includes(activeTool) ||
              (activeTool !== "select" && canvasClickRef.current.moved)) return;
            // The inspector may shrink the stage on the first click and hide
            // again on the second. Resolve the second click in its own frame,
            // rather than the layout React rendered after that click ended.
            const frame = gestureRectRef.current ?? canvasRef.current!.getBoundingClientRect();
            const pointInFrame = (rect: CanvasFrame) => viewPointToDocument(
              {x: event.clientX - rect.left, y: event.clientY - rect.top}, rect, documentSize);
            const index = markIndexAt(history.elements, pointInFrame(frame),
              documentUnitsPerViewPixel(frame, documentSize));
            if (index !== null && history.elements[index].kind === "text") {
              editText(index);
            } else if (activeTool !== "select") {
              return;
            } else if (index !== null && history.elements[index].kind === "watermark") {
              startWatermark(index);
            } else if (index === null) {
              const first = calloutClickRef.current;
              calloutClickRef.current = null;
              const reflowed = first && (["left", "top", "width", "height"] as const).some(
                key => Math.abs(frame[key] - first.frame[key]) > .5);
              if (first && reflowed && !canvasClickRef.current.moved &&
                canvasClickRef.current.editingCalloutId === first.id) {
                const previousIndex = markIndexAt(history.elements, pointInFrame(first.frame),
                  documentUnitsPerViewPixel(first.frame, documentSize));
                if (previousIndex !== null && history.elements[previousIndex].kind === "callout" &&
                  history.elements[previousIndex].id === first.id) {
                  editCallout(previousIndex);
                  return;
                }
              }
              // Plain text/labels retain their existing live-layout editing
              // path when selection closes the inspector between clicks.
              // An actual second-click hit above always takes precedence.
              const liveIndex = markIndexAt(history.elements, toPoint(event.nativeEvent), hitTestScale);
              if (liveIndex !== null && ["text", "watermark"].includes(history.elements[liveIndex].kind)) {
                if (history.elements[liveIndex].kind === "watermark") startWatermark(liveIndex);
                else editText(liveIndex);
                return;
              }
              if (blankDoubleClickRef.current) onFinishOnBlankDoubleClick?.();
            }
          }}
          onPointerCancel={()=>{interactionRef.current={kind:"none"};calloutClickRef.current=null;setDraft(null);setSelectCursor("default");}}
          onPointerLeave={()=>setBrushCursor(null)}
        />
        {dotMarks.map((mark, index) => {
          if (mark.kind !== "text" || !mark.labelDirection || editing?.id === mark.id) return null;
          const {dot, radius} = labelGeometry(mark.rect, mark.fontSize, mark.labelDirection);
          if (markIndexAt(dotMarks.slice(index + 1), dot, hitTestScale) !== null) return null;
          return <LabelDot key={mark.id} x={dot.x * viewScaleX} y={dot.y * viewScaleY}
            radius={radius * Math.min(viewScaleX, viewScaleY)} color={COLOR_HEX[mark.color]}
            direction={mark.labelDirection} disabled={interactionDisabled}
            onToggle={() => toggleLabel(mark.id)}/>;
        })}
        {editing && (
          <div
            style={{
              position: "absolute",
              left: 0,
              top: 0,
              width: documentSize.width,
              height: documentSize.height,
              transformOrigin: "top left",
              transform: `scale(${view.width / documentSize.width}, ${view.height / documentSize.height})`,
              pointerEvents: "none",
            }}
          >
            <TextEditor
              key={editing.id}
              editing={editing}
              bounds={documentSize}
              disabled={interactionDisabled}
              onToggleDirection={() => toggleLabel(editing.id)}
              onTextChange={updateEditingText}
              onRectChange={updateEditingRect}
              onCommit={commitText}
              onFinish={editing.callout || editing.watermark ? undefined : onFinishAfterTextCommit}
              onMoveCallout={(event, firstPoint) => {
                if (interactionsDisabled() || !editingRef.current?.callout) return;
                event.preventDefault(); event.stopPropagation();
                const index = editingRef.current.index!;
                const canvas = canvasRef.current!;
                gestureRectRef.current = canvas.getBoundingClientRect();
                canvas.setPointerCapture(event.pointerId);
                const frame = gestureRectRef.current;
                const start = viewPointToDocument({x: firstPoint.x - frame.left, y: firstPoint.y - frame.top}, frame, documentSize);
                canvasClickRef.current = {start: firstPoint, moved: true, wasEditing: true, editingCalloutId: editingRef.current.id};
                commitText();
                interactionRef.current = {kind: "resize", index, original: history.elements[index], handle: "label", start};
                onPointerMove(event);
              }}
              onUndo={() => undoRef.current()}
              onRedo={() => redoRef.current()}
              onCancel={textEscapeCancelsEdit?()=>{cancelInteraction();}:onCancel}
              nativeUndo={textEscapeCancelsEdit}
            />
          </div>
        )}
      </div>
    );
  },
);

function TextEditor(props: {
  editing: EditingState;
  bounds: { width: number; height: number };
  disabled: boolean;
  onToggleDirection(): void;
  onTextChange(text: string): void;
  onRectChange(rect: Rect, fontSize?: number): void;
  onCommit(): void;
  onFinish?(): void;
  onUndo(): void;
  onRedo(): void;
  onCancel(): void;
  nativeUndo?: boolean;
  onMoveCallout?(event: React.PointerEvent, firstPoint: Point): void;
}) {
  const {
    editing,
    bounds,
    disabled,
    onTextChange,
    onToggleDirection,
    onRectChange,
    onCommit,
    onFinish,
    onUndo,
    onRedo,
    onCancel,
    nativeUndo,
    onMoveCallout,
  } = props;
  const ref = useRef<HTMLTextAreaElement>(null);
  const frameDrag = useRef<{x: number; y: number; pointerId: number} | null>(null);
  const inCalloutFrame = (event: React.PointerEvent<HTMLTextAreaElement>) => {
    if (!editing.callout) return false;
    const rect = event.currentTarget.getBoundingClientRect();
    const pad = Math.max(4, editing.fontSize * .5);
    const x = event.clientX - rect.left, y = event.clientY - rect.top;
    const padX = pad * rect.width / editing.rect.width, padY = pad * rect.height / editing.rect.height;
    return x <= padX || x >= rect.width - padX || y <= padY || y >= rect.height - padY;
  };
  const initialText = useRef(editing.text);
  const attachTextarea = useCallback((element: HTMLTextAreaElement | null) => {
    if (!element && ref.current) setTextComposition(ref.current, false);
    // Let the native editor own the live value/undo stack. A controlled React
    // textarea also rewrites defaultValue (light-DOM children) on each input;
    // WebKit treats those script mutations as non-user edits. Initialize once
    // per annotation, then observe input without writing it back into the DOM.
    if (element && element !== ref.current) element.value = initialText.current;
    ref.current = element;
  }, []);
  const hintId = useId();
  const hintHeight = 32 * editing.uiScale;
  const hintTop = editing.rect.y + editing.rect.height + 4 * editing.uiScale;

  // Callouts open on pointerup and saved text on double-click, after the
  // canvas mouse focus action. Own the next key before the first frame;
  // reopened text keeps the same initial selection as the RAF fallback.
  useLayoutEffect(() => {
    if (editing.callout || editing.watermark) ref.current?.focus();
    else if (editing.index !== null) {
      ref.current?.focus();
      ref.current?.select();
    }
  }, []);

  // Spec §6.6 resizeTextEditor: min 120×34, grows with text/font, clamped
  // to the right/bottom edges of the region.
  useEffect(() => {
    // Creation happens on pointerdown. Focus after its native mouse default
    // action, which otherwise returns WebKit focus to the underlying canvas.
    const frame = requestAnimationFrame(() => {
      const textarea = ref.current;
      // Typing or clicking may already focus this editor before the frame.
      // Preserve that input and selection instead of selecting its new text.
      if (!textarea || textarea.ownerDocument.activeElement === textarea) return;
      textarea.focus();
      textarea.select();
    });
    return () => cancelAnimationFrame(frame);
  }, []);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const font = textFont(editing.fontSize);
    const canvas = document.createElement("canvas");
    const ctx = canvas.getContext("2d")!;
    ctx.font = font;
    const text = editing.text || t("Type something…");
    if (editing.watermarkOriginal && editing.text === editing.watermarkOriginal.text &&
      editing.fontSize === editing.watermarkOriginal.fontSize) return;
    if (editing.callout) {
      const unchanged = editing.text === editing.callout.text && editing.fontSize === editing.callout.fontSize;
      const size = calloutLabelSize({text: editing.text, fontSize: editing.fontSize, uiScale: editing.uiScale,
        boundsWidth: bounds.width, measureText: value => ctx.measureText(value).width,
        width: unchanged || !editing.text ? editing.rect.width : undefined});
      if (unchanged) {
        const rect = repairCalloutLabelHeight(editing.rect, size, bounds.height);
        if (rect !== editing.rect) onRectChange(rect);
        return;
      }
      const {width} = size;
      const height = Math.min(bounds.height, Math.max(editing.fontSize * 2.25, size.height));
      onRectChange({...editing.rect, width, height,
        x: Math.max(0, Math.min(editing.rect.x, bounds.width - width)),
        y: Math.max(0, Math.min(editing.rect.y, bounds.height - height))});
      return;
    }
    // Width follows the longest line (measureText on the whole string with
    // newlines yields a wrong width).
    if (editing.labelDirection) {
      if (editing.textOriginal && editing.text === editing.textOriginal.text &&
        editing.fontSize === editing.textOriginal.fontSize) return;
      const current = editingTextMark(editing);
      const width = Math.max(.1, Math.max(...text.split("\n").map(line => ctx.measureText(line).width)));
      const fitted = fitAnchoredLabel({...current, rect: {...current.rect, width}}, bounds, (value, fontSize) => {
        ctx.font = textFont(fontSize); return ctx.measureText(value).width;
      });
      // Only size changes are applied by the owner, which retains the point.
      onRectChange(editingWithLabel(editing, fitted).rect, fitted.fontSize);
      return;
    }
    const frame = fitTextEditorFrame({
        text,
        fontSize: editing.fontSize,
        x: editing.rect.x,
        y: editing.rect.y,
        maxWidth: Math.min(editing.maxWidth, bounds.width),
        uiScale: editing.uiScale,
        boundsWidth: bounds.width,
        boundsHeight: bounds.height,
        measureText: (value) => ctx.measureText(value).width,
      });
    onRectChange(frame);
  }, [
    editing.labelDirection,
    bounds.height,
    bounds.width,
    editing.fontSize,
    editing.maxWidth,
    editing.rect.x,
    editing.rect.y,
    editing.text,
    onRectChange,
  ]);

  const insets = textEditorInsets(editing.uiScale);
  const label = editing.labelDirection ? labelGeometry({x:editing.rect.x+insets.x, y:editing.rect.y+insets.y,
    width:Math.max(.1,editing.rect.width-insets.x*2),height:Math.max(.1,editing.rect.height-insets.y*2)}, editing.fontSize, editing.labelDirection) : null;
  const edge = label ? editing.labelDirection === "left" ? label.body.x : label.body.x+label.body.width : 0;
  const sign = editing.labelDirection === "left" ? -1 : 1;
  return (
    <>
    {label && <>
      <svg aria-hidden="true" width={bounds.width} height={bounds.height} style={{position:"absolute",inset:0,pointerEvents:"none"}}>
        <rect {...label.body} rx={editing.fontSize*.45} fill="#303136"/>
        <path d={`M${edge-sign} ${label.dot.y-label.tail} L${edge+sign*label.tail} ${label.dot.y} L${edge-sign} ${label.dot.y+label.tail} Z`} fill="#303136"/>
      </svg>
      <LabelDot x={label.dot.x} y={label.dot.y} radius={label.radius} color={COLOR_HEX[editing.color]}
        direction={editing.labelDirection!} disabled={disabled} onToggle={onToggleDirection}/>
    </>}
    <textarea
      ref={attachTextarea}
      className={editing.callout ? "kiri-callout-editor" : label ? "kiri-label-text-editor" : undefined}
      aria-label={t(editing.callout ? "Description (optional)" : "Text content")}
      aria-describedby={editing.callout ? undefined : hintId}
      maxLength={editing.watermark ? 512 : editing.callout ? 1000 : undefined}
      disabled={disabled}
      placeholder={t(editing.callout ? "Add a description…" : "Type something…")}
      spellCheck={false}
      autoCorrect="off"
      autoCapitalize="off"
      onCompositionStart={(e) => setTextComposition(e.currentTarget, true)}
      onCompositionEnd={(e) => setTextComposition(e.currentTarget, false)}
      onBlur={(e) => setTextComposition(e.currentTarget, false)}
      onChange={(e) => onTextChange(e.target.value)}
      onPointerDown={event => {
        if (disabled || event.button !== 0 || !inCalloutFrame(event)) return;
        event.preventDefault(); event.stopPropagation();
        if (isTextComposition({target: event.currentTarget})) return;
        frameDrag.current = {x: event.clientX, y: event.clientY, pointerId: event.pointerId};
        event.currentTarget.focus();
        event.currentTarget.setPointerCapture(event.pointerId);
      }}
      onPointerMove={event => {
        const first = frameDrag.current;
        event.currentTarget.style.cursor = inCalloutFrame(event) ? "move" : "text";
        if (!first || first.pointerId !== event.pointerId) return;
        event.preventDefault(); event.stopPropagation();
        if (isTextComposition({target: event.currentTarget})) {frameDrag.current = null; return;}
        if (Math.hypot(event.clientX - first.x, event.clientY - first.y) < 3) return;
        frameDrag.current = null;
        onMoveCallout?.(event, {x: first.x, y: first.y});
      }}
      onPointerUp={() => {frameDrag.current = null;}}
      onPointerCancel={() => {frameDrag.current = null;}}
      onLostPointerCapture={() => {frameDrag.current = null;}}
      onKeyDown={(e) => {
        handleTextEditorKey(e, { cancel: onCancel, commit: onCommit,
          undo: onUndo, redo: onRedo, finish: onFinish,
          nativeHistory: (command) => e.currentTarget.ownerDocument.execCommand(command), multiline: !!editing.callout,
        }, nativeUndo);
      }}
      style={{
        position: "absolute",
        left: editing.rect.x,
        top: editing.rect.y,
        width: editing.rect.width,
        height: editing.rect.height,
        boxSizing: "border-box",
        padding: editing.callout ? Math.max(0, Math.max(4, editing.fontSize * .5) - editing.uiScale) : `${5*editing.uiScale}px ${8*editing.uiScale}px`,
        font: textFont(editing.fontSize),
        color: label ? "#fafafa" : COLOR_HEX[editing.color],
        background: "transparent",
        outline: "none",
        boxShadow: "none",
        border: `${editing.uiScale}px solid ${label ? "#ffffff55" : COLOR_HEX[editing.color]+"cc"}`,
        borderRadius: 7,
        resize: "none",
        overflow: "hidden",
        whiteSpace: "pre-wrap",
        tabSize: TEXT_TAB_SIZE,
        wordBreak: "break-word",
        lineHeight: 1.25,
        pointerEvents: "auto",
      }}
    />
    {!editing.callout && <div id={hintId} style={{
      position: "absolute",
      left: Math.min(editing.rect.x, Math.max(0, bounds.width - 280 * editing.uiScale)),
      top: hintTop + hintHeight <= bounds.height ? hintTop : Math.max(0, editing.rect.y - hintHeight - 4 * editing.uiScale),
      maxWidth: Math.min(280 * editing.uiScale, bounds.width),
      boxSizing: "border-box",
      padding: `${3 * editing.uiScale}px ${6 * editing.uiScale}px`,
      borderRadius: 5 * editing.uiScale,
      background: "rgba(0,0,0,.8)",
      color: "#eee",
      font: `${10 * editing.uiScale}px/${13 * editing.uiScale}px var(--kiri-font-ui)`,
      pointerEvents: "none",
    }}>{t("Shift + Enter: new line · Enter: done · Esc: cancel edit")}</div>}
    </>
  );
}

export default AnnotationCanvas;


// Native-button gesture state survives a parent render without changing the
// canvas or inline editor's hook identity during development refreshes.
const labelDotPresses = new WeakMap<EventTarget, {pointerId:number; x:number; y:number; moved:boolean; pressed:boolean}>();

function LabelDot({x, y, radius, color, direction, disabled, onToggle}: {
  x:number; y:number; radius:number; color:string; direction:LabelDirection; disabled:boolean; onToggle():void;
}) {
  const size = Math.max(radius * 2 + 10, 22);
  const title = t(direction === "left" ? "Point label right" : "Point label left");
  const trackPointer = (event: React.PointerEvent<HTMLButtonElement>) => {
    event.stopPropagation();
    const press = labelDotPresses.get(event.currentTarget);
    if (press?.pointerId === event.pointerId && Math.hypot(event.clientX - press.x, event.clientY - press.y) >= 3) press.moved = true;
  };
  return <button type="button" className="kiri-label-dot" title={title} aria-label={title} disabled={disabled}
    onPointerDown={event => {
      event.stopPropagation(); event.preventDefault();
      if (disabled || event.button !== 0) return;
      labelDotPresses.set(event.currentTarget, {pointerId:event.pointerId, x:event.clientX, y:event.clientY, moved:false, pressed:true});
      event.currentTarget.setPointerCapture(event.pointerId);
    }}
    onPointerMove={trackPointer}
    onPointerUp={event => {
      trackPointer(event);
      const press = labelDotPresses.get(event.currentTarget);
      if (press?.pointerId === event.pointerId) press.pressed = false;
    }}
    onPointerCancel={event => {
      event.stopPropagation();
      const press = labelDotPresses.get(event.currentTarget);
      if (press) {press.moved = true; press.pressed = false;}
    }}
    onLostPointerCapture={event => {
      const press = labelDotPresses.get(event.currentTarget);
      if (press?.pressed) {press.moved = true; press.pressed = false;}
    }}
    onClick={event => {
      event.stopPropagation();
      const press = labelDotPresses.get(event.currentTarget);
      labelDotPresses.delete(event.currentTarget);
      if (disabled || (event.detail !== 0 && press?.moved)) return;
      onToggle();
    }}
    onDoubleClick={event => {event.stopPropagation(); event.preventDefault();}}
    onKeyDown={event => {if(event.key==="Enter"||event.key===" ")event.stopPropagation();}}
    onKeyUp={event => {if(event.key==="Enter"||event.key===" ")event.stopPropagation();}}
    style={{position:"absolute",left:x-size/2,top:y-size/2,width:size,height:size,pointerEvents:"auto"}}>
    <span style={{width:radius*2,height:radius*2,background:color}}/>
  </button>;
}
