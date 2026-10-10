// EditorWindow — annotation editor for saved captures
// Dark screenshot editor with one compact toolbar and an aspect-fit canvas.

import { OcrDialog } from "../ocr/TextHistory";
import { QrOverlay } from "../qr/QrOverlay";
import type { AssetDto, QrScanDto } from "../lib/ipc";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api, isEditorRevisionMismatch, onEditorRecognizeQr } from "../lib/ipc";
import { t } from "../i18n";
import { isTextComposition } from "../annotation/text-composition.js";
import type { Rect } from "../annotation/geom";
import {
  type AnnotationDocumentV1,
  type AnnotationMark,
  type Tool,
} from "../annotation/model";
import { useAnnotationAppearance } from "../annotation/useAnnotationAppearance";
import {AnnotationStyleControls} from "../annotation/AnnotationStyleControls";
import {TextToolPicker} from "../annotation/TextToolPicker";
import {nextCalloutNumber} from "../annotation/model";
import AnnotationCanvas, { type AnnotationCanvasHandle } from "../annotation/AnnotationCanvas";
import { CropOverlay } from "../annotation/CropOverlay";
import {
  fullCropRect,
  isFullCrop,
} from "../annotation/crop.js";
import { resolveInitialEditorDocument } from "../annotation/editor-document.js";
import { AnnotationInteractionLock } from "../annotation/interaction-lock.js";
import { parseAnnotationDocument } from "../annotation/project.js";
import { KiriIcon, type IconName } from "../components/KiriIcons";
import { kiriResourceUrl } from "../lib/kiri-resource-url.js";
import { hasUnsavedImageChanges, type ImageEditSnapshot, type ImageTextDraft } from "../annotation/image-edit-state.js";
import { ImageCloseGuard, type ImageCloseGuardHandle } from "./ImageCloseGuard";

type EditorTool = Tool | "crop";

const TOOLS: { tool: EditorTool; icon: IconName; title: string }[] = [
  { tool: "select", icon: "cursorarrow", title: "Select (V)" },
  { tool: "crop", icon: "crop", title: "Crop (C)" },
  { tool: "pen", icon: "pencil.tip", title: "Pen (P)" },
  { tool: "rectangle", icon: "rectangle.dashed", title: "Rectangle (R)" },
  { tool: "line", icon: "line.diagonal", title: "Line (L)" },
  { tool: "arrow", icon: "arrow.up.right", title: "Arrow (A)" },
  { tool: "text", icon: "textformat", title: "Text (T)" },
  { tool: "mosaic", icon: "square.grid.3x3.fill", title: "Mosaic (M)" },
  { tool: "watermark", icon: "watermark", title: "Watermark (W)" },
];

export function EditorWindow(props: { id: string }) {
  const [readOnly, setReadOnly] = useState(() => new URLSearchParams(window.location.search).get("readonly") === "1");
  const [image, setImage] = useState<HTMLImageElement | null>(null);
  const [imageSize, setImageSize] = useState<{ w: number; h: number } | null>(null);
  const [document, setDocument] = useState<AnnotationDocumentV1 | null>(null);
  const [containerSize, setContainerSize] = useState({ width: 800, height: 560 });
  const [tool, setTool] = useState<EditorTool>("select");
  const [selectedMark, setSelectedMark] = useState<AnnotationMark | null>(null);
  const [calloutNumber, setCalloutNumber] = useState(1);
  const onAnnotationSelection = useCallback((mark: AnnotationMark | null) => setSelectedMark(mark), []);
  const [cropSelection, setCropSelection] = useState<Rect | null>(null);
  const [cropUndo, setCropUndo] = useState<Rect[]>([]);
  const [cropRedo, setCropRedo] = useState<Rect[]>([]);
  const [appearance, setAppearance] = useAnnotationAppearance();
  const [canUndo, setCanUndo] = useState(false);
  const [canRedo, setCanRedo] = useState(false);
  const [hasMarks, setHasMarks] = useState(false);
  const [ocrAsset, setOcrAsset] = useState<AssetDto | null>(null);
  const [qrActive, setQrActive] = useState(false);
  const [qrScan, setQrScan] = useState<QrScanDto | null>(null);
  const [qrError, setQrError] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [completing, setCompleting] = useState(false);
  const [currentMarks, setCurrentMarks] = useState<AnnotationMark[]>([]);
  useEffect(() => setCalloutNumber(nextCalloutNumber(currentMarks)), [currentMarks]);
  const [textDraft, setTextDraft] = useState<ImageTextDraft | null>(null);
  const [savedSnapshot, setSavedSnapshot] = useState<ImageEditSnapshot>({ marks: [], crop: null });
  const closeGuardRef = useRef<ImageCloseGuardHandle>(null);
  const canvasRef = useRef<AnnotationCanvasHandle>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const revisionRef = useRef<string | null>(null);
  const qrRequestRef = useRef<string | null>(null);
  const requestedQrRef = useRef(new URLSearchParams(window.location.search).get("qr") === "1");
  const completionLock = useMemo(() => new AnnotationInteractionLock(), []);
  const readOnlyRef = useRef(readOnly);
  readOnlyRef.current = readOnly;
  const canvasLock = useMemo(() => ({
    get locked() { return readOnlyRef.current || completionLock.locked || qrRequestRef.current !== null; },
  }), [completionLock]);
  const closeQr = useCallback(() => {
    const requestId = qrRequestRef.current;
    qrRequestRef.current = null;
    setQrActive(false);
    setQrScan(null);
    setQrError(null);
    if (requestId) void api.cancelQr(requestId).catch(() => {});
  }, []);
  const runQr = useCallback(() => {
    const revision = revisionRef.current;
    if (!imageSize || !revision || ocrAsset || completionLock.locked || qrRequestRef.current) return;
    const requestId = crypto.randomUUID();
    qrRequestRef.current = requestId;
    setActionError(null);
    setQrActive(true);
    setQrScan(null);
    setQrError(null);
    void api.scanQr(requestId, null, props.id, revision).then(scan => {
      if (qrRequestRef.current !== requestId) return;
      // The arrows describe this displayed source, never a changed asset or
      // annotation document's logical coordinates (which can differ at 2x).
      if (scan.width !== imageSize.w || scan.height !== imageSize.h) {
        setQrError("The screenshot changed. Close and reopen the editor.");
        void api.cancelQr(requestId).catch(() => {});
        return;
      }
      setQrScan(scan);
    }).catch(error => {
      if (qrRequestRef.current !== requestId) return;
      setQrError(isEditorRevisionMismatch(error)
        ? "The screenshot changed. Close and reopen the editor."
        : "QR recognition failed.");
    });
  }, [completionLock, imageSize, ocrAsset, props.id]);
  const runQrRef = useRef(runQr);
  runQrRef.current = runQr;
  const onTextDraftChange = useCallback((mark: AnnotationMark | null, previousId: number | null, editing: boolean) => {
    setTextDraft({ mark, previousId, editing });
  }, []);
  const effectiveCrop = document && cropSelection && !isFullCrop(document, cropSelection) ? cropSelection : null;
  const dirty = hasUnsavedImageChanges(savedSnapshot, currentMarks, effectiveCrop, textDraft);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    const consumePending = async () => {
      if (disposed) return;
      try {
        const requested = await api.takeEditorQrRequest();
        if (disposed || !requested) return;
        requestedQrRef.current = true;
        runQrRef.current();
        if (qrRequestRef.current) requestedQrRef.current = false;
      } catch { /* The toolbar remains available if a window event fails. */ }
    };
    void onEditorRecognizeQr(() => { void consumePending(); }).then(stop => {
      if (disposed) stop();
      else {
        unlisten = stop;
        // The backend queues an existing editor's request until this listener
        // is ready, including a request emitted while the WebView was loading.
        void consumePending();
      }
    }).catch(() => {});
    return () => {
      disposed = true;
      unlisten?.();
      // StrictMode replays this cleanup before the first image finishes loading.
      // Preserve an unconsumed query/event intent for the next setup.
      closeQr();
    };
  }, [closeQr, props.id]);

  useEffect(() => {
    if (!imageSize || !requestedQrRef.current) return;
    runQr();
    if (qrRequestRef.current) requestedQrRef.current = false;
  }, [completing, imageSize, runQr]);

  useEffect(() => {
    const controller = new AbortController();
    let disposed = false;
    let objectUrl: string | null = null;
    let pendingImage: HTMLImageElement | null = null;

    setImage(null);
    setImageSize(null);
    setDocument(null);
    setHasMarks(false);
    setCurrentMarks([]);
    setTextDraft(null);
    setSavedSnapshot({ marks: [], crop: null });
    setCropSelection(null);
    setCropUndo([]);
    setCropRedo([]);
    revisionRef.current = null;
    setActionError(null);

    async function loadImage(url: string): Promise<HTMLImageElement> {
      const response = await fetch(url, { signal: controller.signal });
      if (!response.ok) throw new Error("asset unavailable");
      const blob = await response.blob();
      if (disposed) throw new Error("editor disposed");
      if (objectUrl) URL.revokeObjectURL(objectUrl);
      objectUrl = URL.createObjectURL(blob);
      const img = new Image();
      pendingImage = img;
      img.src = objectUrl;
      await img.decode();
      return img;
    }

    void (async () => {
      let initialDocument: AnnotationDocumentV1 | null = null;
      let revisionSha256: string;
      try {
        const snapshot = await api.getAssetAnnotationProject(props.id);
        if (!disposed) setReadOnly(snapshot.readOnly === true);
        revisionSha256 = snapshot.revisionSha256;
        if (snapshot.state === "valid") {
          if (!snapshot.documentJson) throw new Error("valid project has no document");
          initialDocument = parseAnnotationDocument(JSON.parse(snapshot.documentJson));
        } else if (snapshot.state === "invalid" && !disposed) {
          setActionError("Editable data couldn't be loaded. The current image is still available.");
        }
        if (disposed) return;
      } catch {
        if (!disposed) setActionError("The screenshot changed. Close and reopen the editor.");
        return;
      }

      let img: HTMLImageElement;
      try {
        img = await loadImage(
          kiriResourceUrl("annotation-source", [props.id], {
            revision: revisionSha256,
          }),
        );
        if (
          initialDocument &&
          (img.naturalWidth !== initialDocument.sourcePixels.width ||
            img.naturalHeight !== initialDocument.sourcePixels.height)
        ) {
          throw new Error("annotation source dimensions changed");
        }
      } catch {
        if (!disposed) setActionError("The screenshot changed. Close and reopen the editor.");
        return;
      }
      if (disposed) return;
      const nextDocument = resolveInitialEditorDocument(initialDocument, {
        width: img.naturalWidth,
        height: img.naturalHeight,
      });
      setImage(img);
      setImageSize({ w: img.naturalWidth, h: img.naturalHeight });
      setDocument(nextDocument);
      setCurrentMarks(nextDocument.marks);
      setSavedSnapshot({ marks: nextDocument.marks, crop: null });
      setHasMarks(nextDocument.marks.length > 0);
      revisionRef.current = revisionSha256;
    })();

    return () => {
      disposed = true;
      controller.abort();
      if (pendingImage) pendingImage.onload = null;
      if (objectUrl) URL.revokeObjectURL(objectUrl);
    };
  }, [props.id]);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    const publish = () => {
      setContainerSize({
        width: Math.max(1, container.clientWidth),
        height: Math.max(1, container.clientHeight),
      });
    };
    publish();
    const observer = new ResizeObserver(publish);
    observer.observe(container);
    return () => observer.disconnect();
  }, []);

  // Aspect-fit CSS size; annotation geometry remains in document.canvas.
  const viewSize = useMemo(() => {
    if (!imageSize) return { width: 1, height: 1 };
    const scale = Math.min(
      containerSize.width / imageSize.w,
      containerSize.height / imageSize.h,
    );
    return { width: imageSize.w * scale, height: imageSize.h * scale };
  }, [containerSize, imageSize]);

  const documentRegion = useMemo<Rect>(() => {
    const canvas = document?.canvas ?? { width: 1, height: 1 };
    return { x: 0, y: 0, width: canvas.width, height: canvas.height };
  }, [document]);

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (isTextComposition(e)) return;
      if (e.target instanceof Element && e.target.closest("input,textarea,select,[contenteditable]")) return;
      if (qrRequestRef.current) {
        if (e.key === "Escape") {
          e.preventDefault();
          e.stopPropagation();
          closeQr();
        }
        // QR controls and selected text retain their normal keyboard behavior.
        // Editor save, undo, crop and annotation shortcuts stay paused.
        return;
      }
      if (completionLock.locked) {
        e.preventDefault();
        e.stopPropagation();
        return;
      }
      if (readOnlyRef.current) {
        if (e.key === "Escape") void closeWindow();
        return;
      }
      if (e.key === "Escape") {
        if (tool === "crop") {
          cancelCrop();
          return;
        }
        void closeWindow();
        return;
      }
      if (e.key === "Enter" && !e.isComposing) {
        // Focused controls own Enter (notably export and crop cancellation).
        // Saving here before their native click could persist unintended edits.
        if (e.target instanceof Element && e.target.closest("button,input,select,textarea,[contenteditable]")) return;
        void complete("save");
        return;
      }
      const mod = e.metaKey || e.ctrlKey;
      if (mod && e.key.toLowerCase() === "z") {
        e.preventDefault();
        if (tool === "crop") {
          if (e.shiftKey) redoCrop();
          else undoCrop();
        } else if (e.shiftKey) canvasRef.current?.redo();
        else canvasRef.current?.undo();
        return;
      }
      if (e.key === "Delete" || e.key === "Backspace") {
        if (tool !== "crop") canvasRef.current?.deleteSelection();
        return;
      }
      if (!mod && !e.altKey) {
        const keyMap: Record<string, EditorTool> = {
          v: "select",
          c: "crop",
          p: "pen",
          r: "rectangle",
          l: "line",
          a: "arrow",
          t: "text",
          b: "label",
          n: "callout",
          m: "mosaic",
          w: "watermark",
        };
        const next = keyMap[e.key.toLowerCase()];
        if (next) selectTool(next);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [closeQr, completionLock, cropRedo, cropSelection, cropUndo, document, tool]);

  function selectTool(next: EditorTool) {
    if (readOnlyRef.current || qrRequestRef.current) return;
    canvasRef.current?.finishAppearanceAdjustment();
    if (next === "watermark") {
      setTool(next);
      canvasRef.current?.editWatermark();
      return;
    }
    canvasRef.current?.commitTextEditing();
    if (next !== "select") canvasRef.current?.clearSelection();
    setTool(next);
    if (next === "crop" && document) {
      setCropSelection((current) => current ?? fullCropRect(document));
    }
  }

  function cancelCrop() {
    setCropSelection(null);
    setCropUndo([]);
    setCropRedo([]);
    if (tool === "crop") setTool("select");
  }

  function commitCrop(previous: Rect, next: Rect) {
    if (sameRect(previous, next)) return;
    setCropUndo((history) => [...history.slice(-99), previous]);
    setCropRedo([]);
  }

  function undoCrop() {
    if (!cropSelection || cropUndo.length === 0) return;
    const previous = cropUndo[cropUndo.length - 1];
    setCropUndo(cropUndo.slice(0, -1));
    setCropRedo([...cropRedo.slice(-99), cropSelection]);
    setCropSelection(previous);
  }

  function redoCrop() {
    if (!cropSelection || cropRedo.length === 0) return;
    const next = cropRedo[cropRedo.length - 1];
    setCropRedo(cropRedo.slice(0, -1));
    setCropUndo([...cropUndo.slice(-99), cropSelection]);
    setCropSelection(next);
  }

  async function closeWindow() {
    if (qrRequestRef.current) { closeQr(); return; }
    await closeGuardRef.current?.requestClose();
  }

  async function complete(action: "save" | "saveAs") {
    if (readOnlyRef.current || qrRequestRef.current) return;
    if (!completionLock.acquire()) return;
    setCompleting(true);
    const failureMessage = "Couldn't save the edited image. Try again.";
    try {
      setActionError(null);
      const revisionSha256 = revisionRef.current;
      if (!revisionSha256) {
        setActionError("The screenshot changed. Close and reopen the editor.");
        return;
      }
      const result = await canvasRef.current?.exportResult(cropSelection ?? undefined);
      if (!result) {
        setActionError(failureMessage);
        return;
      }
      const saveToken = action === "saveAs"
        ? await api.saveFileDialog(`kiri-${props.id}.png`)
        : null;
      // Cancelling Save As must be a true no-op: do not replace the library
      // asset when the system file picker returns no one-time authorization.
      if (action === "saveAs" && saveToken === null) return;
      const update = await api.updateAsset(props.id, result.png, result.document, {
        action,
        cropPixels: result.cropPixels,
        saveToken,
        revisionSha256,
      });
      revisionRef.current = update.revisionSha256;
      if (!update.actionSucceeded) {
        setActionError(failureMessage);
        return;
      }
      // Save As exports a copy without updating the editable library asset.
      // Only Save advances the library baseline; exported edits remain dirty.
      if (action === "save") {
        setSavedSnapshot({ marks: result.document.marks, crop: effectiveCrop });
        await closeGuardRef.current?.closeSaved();
      }
    } catch (error) {
      if (isEditorRevisionMismatch(error)) {
        revisionRef.current = null;
        setActionError("The screenshot changed. Close and reopen the editor.");
      } else {
        setActionError(failureMessage);
      }
    } finally {
      completionLock.release();
      setCompleting(false);
    }
  }

  return (
    <div
      className="kiri-dark"
      aria-busy={completing}
      data-interaction-disabled={completing || undefined}
      inert={completing}
      style={{ height: "100%", display: "flex", flexDirection: "column", background: "#080808", position: "relative" }}
    >
      <div className="kiri-image-editor-toolbar" style={{opacity: completing ? 0.62 : 1}}>
        {!readOnly && <div className="kiri-image-editor-tools" inert={qrActive}>
          <div className="kiri-annotation-tool-group">
            {TOOLS.slice(0, 2).map(({tool: value, icon, title}) => <EditorToolButton key={value} icon={icon} title={t(title)} active={tool === value} onClick={() => selectTool(value)}/>)}
          </div>
          <div className="kiri-annotation-tool-group" role="group" aria-label={t("Annotations")}>
            {TOOLS.slice(2, 6).map(({tool: value, icon, title}) => <EditorToolButton key={value} icon={icon} title={t(title)} active={tool === value} onClick={() => selectTool(value)}/>)}
          </div>
          <div className="kiri-annotation-tool-group"><TextToolPicker tool={tool} onSelect={selectTool}/></div>
          <div className="kiri-annotation-tool-group">
            {TOOLS.slice(7).map(({tool: value, icon, title}) => <EditorToolButton key={value} icon={icon} title={t(title)} active={tool === value} onClick={() => selectTool(value)}/>)}
          </div>
          <div className="kiri-annotation-tool-group">
            <EditorToolButton icon="arrow.uturn.backward" title={t("Undo (⌘Z)")} disabled={tool === "crop" ? cropUndo.length === 0 : !canUndo}
              onClick={() => tool === "crop" ? undoCrop() : canvasRef.current?.undo()}/>
            <EditorToolButton icon="arrow.uturn.forward" title={t("Redo (⇧⌘Z)")} disabled={tool === "crop" ? cropRedo.length === 0 : !canRedo}
              onClick={() => tool === "crop" ? redoCrop() : canvasRef.current?.redo()}/>
            <EditorToolButton icon="xmark" title={t("Clear Annotations")} disabled={tool === "crop" || !hasMarks} onClick={() => canvasRef.current?.clearAnnotations()}/>
          </div>
        </div>}
        <div className="kiri-image-editor-actions">
          <EditorToolButton icon="qrcode" title={t("Recognize QR Codes")} active={qrActive} disabled={!image || completing || !!ocrAsset}
            onClick={() => qrRequestRef.current ? closeQr() : runQr()}/>
          {!readOnly && <EditorToolButton icon="text.viewfinder" title={t("Recognize Saved Image Locally")} disabled={!image || completing || qrActive}
            onClick={() => { void api.getAsset(props.id).then(setOcrAsset).catch(() => setActionError(t("Can't read this file."))); }}/>}
          {!readOnly && <EditorToolButton icon="doc.on.doc" title={t("Save As…")} disabled={qrActive} onClick={() => void complete("saveAs")}/>}
          <EditorToolButton icon="xmark" title={t(readOnly ? "Close" : "Cancel (Esc)")} onClick={() => qrRequestRef.current ? closeQr() : closeWindow()}/>
          {!readOnly && <button type="button" className="kiri-primary-button" disabled={qrActive} style={{minHeight: 32, borderRadius: 10}}
            onClick={() => void complete("save")}>{t("Save")}</button>}
        </div>
      </div>
      {!readOnly && <div className="kiri-image-editor-properties" inert={qrActive}>
        {(tool === "crop" || cropSelection) && <div className="kiri-image-editor-crop-options"><strong>{t("Crop")}</strong>
          {cropSelection && <button type="button" className="kiri-annotation-action" disabled={qrActive || completing}
            onKeyDown={event => {if (event.key === "Enter" || event.key === " ") event.stopPropagation();}}
            onClick={cancelCrop}>{t("Cancel crop")}</button>}</div>}
        {tool !== "crop" &&
          <AnnotationStyleControls tool={tool} selected={selectedMark} appearance={appearance} disabled={qrActive}
            nextNumber={calloutNumber} onNextNumber={setCalloutNumber}
            onCalloutEdit={(patch, transient) => canvasRef.current?.updateSelectedCallout(patch, transient)}
            onChange={(patch, transient) => {setActionError(null); setAppearance({...appearance, ...patch}); canvasRef.current?.updateSelectionAppearance(patch, transient);}}
            onFinish={() => canvasRef.current?.finishAppearanceAdjustment()}
            onEditText={() => canvasRef.current?.editSelectedText()} onEditWatermark={() => canvasRef.current?.editWatermark()}/>}
      </div>}

      {ocrAsset && <OcrDialog asset={ocrAsset} onClose={() => setOcrAsset(null)} />}

      {actionError && (
        <div
          role="alert"
          style={{
            position: "absolute",
            top: 66,
            left: "50%",
            transform: "translateX(-50%)",
            zIndex: 20,
            maxWidth: "min(420px, calc(100% - 24px))",
            minHeight: 30,
            display: "flex",
            alignItems: "center",
            gap: 8,
            padding: "5px 7px 5px 11px",
            boxSizing: "border-box",
            border: "1px solid var(--kiri-surface-border)",
            borderRadius: 10,
            background: "var(--kiri-elevated)",
            color: "var(--kiri-coral)",
            font: "500 11.5px/16px var(--kiri-font-ui)",
          }}
        >
          <span style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
            {t(actionError)}
          </span>
          <button
            type="button"
            className="kiri-icon-button kiri-inline-dismiss"
            aria-label={t("Close")}
            title={t("Close")}
            onClick={() => setActionError(null)}
            style={{
              width: 20,
              height: 20,
              flexBasis: 20,
            }}
          >
            <KiriIcon name="xmark" size={9} />
          </button>
        </div>
      )}

      {/* Canvas area */}
      <div ref={containerRef} style={{ flex: 1, minHeight: 0, minWidth: 0, overflow: "hidden", display: "flex", alignItems: "center", justifyContent: "center", background: "#141414", position: "relative" }}>
        {imageSize && document && (
          <div style={{ position: "relative", width: viewSize.width, height: viewSize.height,
            backgroundImage: qrActive && image ? `url("${image.src}")` : undefined,
            backgroundSize: "100% 100%", backgroundRepeat: "no-repeat" }}>
            {/* Keep the canvas and native text editor mounted: QR reads the
                same clean source, then restores all unsaved edits on exit. */}
            <div style={{ visibility: qrActive ? "hidden" : undefined }}>
            <AnnotationCanvas
              ref={canvasRef}
              image={image}
              region={documentRegion}
              viewSize={viewSize}
              initialDocument={document}
              interactionDisabled={readOnly || completing || tool === "crop" || qrActive}
              interactionLock={canvasLock}
              tool={tool === "crop" ? "select" : tool}
              appearance={appearance}
              mosaicShape={appearance.mosaicShape}
              onError={setActionError}
              calloutNumber={calloutNumber}
              onSelectionInfo={onAnnotationSelection}
              onHistoryChange={(u, r, populated) => {
                setCanUndo(u);
                setCanRedo(r);
                setHasMarks(populated);
              }}
              onCancel={closeWindow}
              onDocumentChange={setCurrentMarks}
              onTextDraftChange={onTextDraftChange}
            />
            {cropSelection && (
              <CropOverlay
                document={document}
                viewSize={viewSize}
                selection={cropSelection}
                active={tool === "crop" && !completing && !qrActive}
                onChange={setCropSelection}
                onCommit={commitCrop}
              />
            )}
            </div>
          </div>
        )}
        {qrActive && <QrOverlay scan={qrScan} failed={!!qrError} error={qrError}
          sourceRect={{ x: (containerSize.width - viewSize.width) / 2,
            y: (containerSize.height - viewSize.height) / 2, ...viewSize }}
          selection={documentRegion} bounds={{ x: 0, y: 0, ...containerSize }} scale={1}
          onRetry={() => { closeQr(); runQr(); }} onClose={closeQr} onOpened={closeQr} />}
      </div>
      <ImageCloseGuard ref={closeGuardRef} dirty={!readOnly && dirty} busy={completing} lock={completionLock}
        error={actionError} onSave={() => { closeQr(); return complete("save"); }} />
    </div>
  );
}

function sameRect(left: Rect, right: Rect): boolean {
  return left.x === right.x && left.y === right.y &&
    left.width === right.width && left.height === right.height;
}

function EditorToolButton(props: {
  icon: IconName;
  title: string;
  active?: boolean;
  disabled?: boolean;
  onClick(): void;
}) {
  return (
    <button
      type="button"
      className="kiri-toolbar-button"
      data-active={props.active || undefined}
      title={props.title}
      aria-label={props.title}
      aria-pressed={props.active}
      onClick={props.onClick}
      disabled={props.disabled}
    >
      <KiriIcon name={props.icon} size={15} />
    </button>
  );
}
