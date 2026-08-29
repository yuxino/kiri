// EditorWindow — annotation editor for saved captures
// Dark screenshot editor with one compact toolbar and an aspect-fit canvas.

import { useEffect, useMemo, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { api, isEditorRevisionMismatch } from "../lib/ipc";
import { t } from "../i18n";
import type { Rect } from "../annotation/geom";
import {
  COLOR_HEX,
  COLOR_PRESETS,
  DEFAULT_APPEARANCE,
  type AnnotationDocumentV1,
  type AppearanceSettings,
  type MosaicIntensity,
  type MosaicStyle,
  type TextBackgroundStyle,
  type Tool,
} from "../annotation/model";
import AnnotationCanvas, { type AnnotationCanvasHandle } from "../annotation/AnnotationCanvas";
import { CropOverlay } from "../annotation/CropOverlay";
import {
  cropAnnotationDocument,
  fullCropRect,
  isFullCrop,
  type CropPixels,
} from "../annotation/crop.js";
import { resolveInitialEditorDocument } from "../annotation/editor-document.js";
import { AnnotationInteractionLock } from "../annotation/interaction-lock.js";
import { parseAnnotationDocument } from "../annotation/project.js";
import { KiriIcon, type IconName } from "../components/KiriIcons";

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
];

export function EditorWindow(props: { id: string }) {
  const [image, setImage] = useState<HTMLImageElement | null>(null);
  const [imageSize, setImageSize] = useState<{ w: number; h: number } | null>(null);
  const [document, setDocument] = useState<AnnotationDocumentV1 | null>(null);
  const [containerSize, setContainerSize] = useState({ width: 800, height: 560 });
  const [tool, setTool] = useState<EditorTool>("select");
  const [cropSelection, setCropSelection] = useState<Rect | null>(null);
  const [cropUndo, setCropUndo] = useState<Rect[]>([]);
  const [cropRedo, setCropRedo] = useState<Rect[]>([]);
  const [appearance, setAppearance] = useState<AppearanceSettings>(DEFAULT_APPEARANCE);
  const [canUndo, setCanUndo] = useState(false);
  const [canRedo, setCanRedo] = useState(false);
  const [hasMarks, setHasMarks] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);
  const [completing, setCompleting] = useState(false);
  const canvasRef = useRef<AnnotationCanvasHandle>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const revisionRef = useRef<string | null>(null);
  const completionLock = useMemo(() => new AnnotationInteractionLock(), []);

  useEffect(() => {
    const controller = new AbortController();
    let disposed = false;
    let objectUrl: string | null = null;
    let pendingImage: HTMLImageElement | null = null;

    setImage(null);
    setImageSize(null);
    setDocument(null);
    setHasMarks(false);
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
          `kiri://annotation-source/${props.id}?revision=${revisionSha256}`,
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
      if (completionLock.locked) {
        e.preventDefault();
        e.stopPropagation();
        return;
      }
      if (e.key === "Escape") {
        if (tool === "crop") {
          setCropSelection(null);
          setCropUndo([]);
          setCropRedo([]);
          setTool("select");
          return;
        }
        void closeWindow();
        return;
      }
      if (e.key === "Enter" && !e.isComposing) {
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
          m: "mosaic",
        };
        const next = keyMap[e.key.toLowerCase()];
        if (next) selectTool(next);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [completionLock, cropRedo, cropSelection, cropUndo, document, tool]);

  function selectTool(next: EditorTool) {
    if (tool === "crop" && next !== "crop") return;
    setTool(next);
    if (next === "crop" && document) {
      setCropSelection((current) => current ?? fullCropRect(document));
    }
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
    await getCurrentWindow().close();
  }

  async function complete(action: "save" | "saveAs") {
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
      const result = await canvasRef.current?.exportResult();
      if (!result) {
        setActionError(failureMessage);
        return;
      }
      let outputPng = result.png;
      let outputDocument = result.document;
      let cropPixels: CropPixels | null = null;
      if (cropSelection && !isFullCrop(result.document, cropSelection)) {
        const cropped = cropAnnotationDocument(result.document, cropSelection);
        outputPng = await cropPng(result.png, cropped.cropPixels);
        outputDocument = cropped.document;
        cropPixels = cropped.cropPixels;
      }
      const saveToken = action === "saveAs"
        ? await api.saveFileDialog(`kiri-${props.id}.png`)
        : null;
      // Cancelling Save As must be a true no-op: do not replace the library
      // asset when the system file picker returns no one-time authorization.
      if (action === "saveAs" && saveToken === null) return;
      const update = await api.updateAsset(props.id, outputPng, outputDocument, {
        action,
        cropPixels,
        saveToken,
        revisionSha256,
      });
      revisionRef.current = update.revisionSha256;
      if (!update.actionSucceeded) {
        setActionError(failureMessage);
        return;
      }
      if (action === "save") await closeWindow().catch(() => {});
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

  const slider =
    tool === "pen"
      ? { min: 1, max: 24, value: appearance.penWidth, onChange: (v: number) => setAppearance({ ...appearance, penWidth: v }) }
      : tool === "rectangle" || tool === "line" || tool === "arrow"
        ? { min: 1, max: 16, value: appearance.shapeWidth, onChange: (v: number) => setAppearance({ ...appearance, shapeWidth: v }) }
        : tool === "text"
          ? { min: 12, max: 64, value: appearance.textFontSize, onChange: (v: number) => setAppearance({ ...appearance, textFontSize: v }) }
          : tool === "mosaic"
            ? { min: 12, max: 120, value: appearance.mosaicBrushDiameter, onChange: (v: number) => setAppearance({ ...appearance, mosaicBrushDiameter: v }) }
            : null;

  return (
    <div
      className="kiri-dark"
      aria-busy={completing}
      data-interaction-disabled={completing || undefined}
      inert={completing}
      style={{ height: "100%", display: "flex", flexDirection: "column", background: "#080808", position: "relative" }}
    >
      {/* 58pt toolbar */}
      <div
        style={{
          height: 58,
          display: "flex",
          alignItems: "center",
          gap: 4,
          padding: "0 10px",
          borderBottom: "1px solid #383838",
          background: "#101010",
          opacity: completing ? 0.62 : 1,
          transition: "opacity 0.12s ease-out",
        }}
      >
        <div
          style={{
            minWidth: 0,
            flex: 1,
            display: "flex",
            alignItems: "center",
            gap: 4,
            overflowX: "auto",
            scrollbarWidth: "none",
          }}
        >
        {TOOLS.map(({ tool: t2, icon, title }) => (
          <EditorToolButton
            key={t2}
            icon={icon}
            title={t(title)}
            active={tool === t2}
            disabled={tool === "crop" && t2 !== "crop"}
            onClick={() => selectTool(t2)}
          />
        ))}
        {tool !== "crop" && tool !== "select" && <>
          <div style={{ width: 1, height: 26, background: "#383838", margin: "0 4px" }} />
          {tool === "text" ? (
          <EditorSegments
            segments={[
              { icon: "square.dashed", label: t("Transparent"), title: t("No background") },
              { icon: "moon.fill", label: t("Dark"), title: t("Dark background") },
            ]}
            value={appearance.textBackgroundStyle === "transparent" ? 0 : 1}
            onChange={(i) =>
              setAppearance({ ...appearance, textBackgroundStyle: (["transparent", "dark"] as TextBackgroundStyle[])[i] })
            }
          />
        ) : tool === "mosaic" ? (
          <>
            <EditorSegments
              segments={[
                { label: t("Pixel"), title: t("Pixel mosaic") },
                { label: t("Blur"), title: t("Gaussian blur") },
              ]}
              value={appearance.mosaicStyle === "pixel" ? 0 : 1}
              onChange={(i) =>
                setAppearance({ ...appearance, mosaicStyle: (["pixel", "blur"] as MosaicStyle[])[i] })
              }
            />
            <EditorSegments
              segments={[{ label: "1", title: t("Soft") }, { label: "2", title: t("Standard") }, { label: "3", title: t("Strong") }]}
              value={appearance.mosaicIntensity === "soft" ? 0 : appearance.mosaicIntensity === "standard" ? 1 : 2}
              onChange={(i) =>
                setAppearance({ ...appearance, mosaicIntensity: (["soft", "standard", "strong"] as MosaicIntensity[])[i] })
              }
            />
          </>
          ) : null}
          {slider && (
          <div style={{ display: "flex", alignItems: "center", gap: 6, marginLeft: 4 }}>
            <input
              type="range"
              min={slider.min}
              max={slider.max}
              value={slider.value}
              onChange={(e) => {
                const value = Math.round(Number(e.target.value));
                slider.onChange(value);
                if (tool === "text") canvasRef.current?.setTextFontSizeLive(value);
              }}
              onPointerDown={() => {
                if (tool === "text") canvasRef.current?.beginTextFontSizeAdjustment();
              }}
              onPointerUp={() => {
                if (tool === "text") canvasRef.current?.endTextFontSizeAdjustment();
              }}
              onPointerLeave={() => {
                if (tool === "text") canvasRef.current?.endTextFontSizeAdjustment();
              }}
              style={{ width: 90, accentColor: "#fff" }}
            />
            <span style={{ width: 28, textAlign: "right", fontSize: 9, fontVariantNumeric: "tabular-nums" }}>
              {slider.value}
            </span>
          </div>
          )}
          <div style={{ width: 1, height: 26, background: "#383838", margin: "0 4px" }} />
          {COLOR_PRESETS.map((preset) => (
          <EditorSwatch
            key={preset}
            color={COLOR_HEX[preset]}
            selected={appearance.colorPreset === preset}
            onClick={() => setAppearance({ ...appearance, colorPreset: preset })}
          />
          ))}
        </>}
        <div style={{ width: 1, height: 26, background: "#383838", margin: "0 4px" }} />
        <EditorToolButton
          icon="arrow.uturn.backward"
          title={t("Undo (⌘Z)")}
          disabled={tool === "crop" ? cropUndo.length === 0 : !canUndo}
          onClick={() => tool === "crop" ? undoCrop() : canvasRef.current?.undo()}
        />
        <EditorToolButton
          icon="arrow.uturn.forward"
          title={t("Redo (⇧⌘Z)")}
          disabled={tool === "crop" ? cropRedo.length === 0 : !canRedo}
          onClick={() => tool === "crop" ? redoCrop() : canvasRef.current?.redo()}
        />
        <EditorToolButton
          icon="xmark"
          title={t("Clear Annotations")}
          disabled={tool === "crop" || !hasMarks}
          onClick={() => canvasRef.current?.clearAnnotations()}
        />
        </div>
        <div style={{ display: "flex", alignItems: "center", gap: 4, flexShrink: 0 }}>
        <button
          className="editor-secondary-button"
          style={{
            height: 32,
            padding: "0 12px",
            borderRadius: 10,
            border: "1px solid rgba(255,255,255,0.16)",
            background: "transparent",
            color: "#fff",
            fontSize: 12,
            fontWeight: 500,
            cursor: "pointer",
          }}
          onClick={() => void complete("saveAs")}
        >
          {t("Save As…")}
        </button>
        <button
          className="editor-secondary-button"
          title={t("Cancel (Esc)")}
          style={{
            height: 32,
            padding: "0 12px",
            borderRadius: 10,
            border: "1px solid transparent",
            background: "transparent",
            color: "#fff",
            fontSize: 12,
            fontWeight: 500,
            cursor: "pointer",
          }}
          onClick={closeWindow}
        >
          {t("Cancel")}
        </button>
        <button
          className="kiri-primary-button"
          style={{ minHeight: 32, borderRadius: 10 }}
          onClick={() => void complete("save")}
        >
          {t("Save")}
        </button>
        </div>
      </div>

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
            aria-label={t("Close")}
            title={t("Close")}
            onClick={() => setActionError(null)}
            style={{
              width: 20,
              height: 20,
              flexShrink: 0,
              display: "grid",
              placeItems: "center",
              padding: 0,
              border: "none",
              borderRadius: 6,
              background: "transparent",
              color: "var(--kiri-secondary-label)",
              cursor: "pointer",
            }}
          >
            <KiriIcon name="xmark" size={9} />
          </button>
        </div>
      )}

      {/* Canvas area */}
      <div ref={containerRef} style={{ flex: 1, display: "flex", alignItems: "center", justifyContent: "center", background: "#141414", position: "relative" }}>
        {imageSize && document && (
          <div style={{ position: "relative", width: viewSize.width, height: viewSize.height }}>
            <AnnotationCanvas
              ref={canvasRef}
              image={image}
              region={documentRegion}
              viewSize={viewSize}
              initialDocument={document}
              interactionDisabled={completing || tool === "crop"}
              interactionLock={completionLock}
              tool={tool === "crop" ? "select" : tool}
              appearance={appearance}
              onHistoryChange={(u, r, populated) => {
                setCanUndo(u);
                setCanRedo(r);
                setHasMarks(populated);
              }}
              onCancel={closeWindow}
            />
            {cropSelection && (
              <CropOverlay
                document={document}
                viewSize={viewSize}
                selection={cropSelection}
                active={tool === "crop" && !completing}
                onChange={setCropSelection}
                onCommit={commitCrop}
              />
            )}
          </div>
        )}
      </div>
    </div>
  );
}

function sameRect(left: Rect, right: Rect): boolean {
  return left.x === right.x && left.y === right.y &&
    left.width === right.width && left.height === right.height;
}

async function cropPng(png: Uint8Array, crop: CropPixels): Promise<Uint8Array> {
  const url = URL.createObjectURL(new Blob([png.slice().buffer], { type: "image/png" }));
  const image = new Image();
  try {
    image.src = url;
    await image.decode();
    const canvas = document.createElement("canvas");
    canvas.width = crop.width;
    canvas.height = crop.height;
    const context = canvas.getContext("2d");
    if (!context) throw new Error("crop canvas unavailable");
    context.drawImage(
      image,
      crop.x,
      crop.y,
      crop.width,
      crop.height,
      0,
      0,
      crop.width,
      crop.height,
    );
    const blob = await new Promise<Blob | null>((resolve) => canvas.toBlob(resolve, "image/png"));
    canvas.width = 0;
    canvas.height = 0;
    if (!blob) throw new Error("crop export failed");
    return new Uint8Array(await blob.arrayBuffer());
  } finally {
    URL.revokeObjectURL(url);
  }
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
      title={props.title}
      onClick={props.onClick}
      disabled={props.disabled}
      style={{
        width: 32,
        height: 32,
        borderRadius: 10,
        border: "1px solid transparent",
        background: props.active ? "#fff" : "transparent",
        color: props.active ? "#000" : "#fff",
        fontSize: 12,
        fontWeight: 600,
        cursor: props.disabled ? "default" : "pointer",
        opacity: props.disabled ? 0.35 : 1,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        flexShrink: 0,
      }}
    >
      <KiriIcon name={props.icon} size={15} />
    </button>
  );
}

function EditorSwatch(props: { color: string; selected: boolean; onClick(): void }) {
  return (
    <button
      onClick={props.onClick}
      style={{
        width: 24,
        height: 28,
        borderRadius: 8,
        border: "none",
        background: props.selected ? `${props.color}33` : "transparent",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        cursor: "pointer",
        position: "relative",
        flexShrink: 0,
      }}
    >
      {props.selected && (
        <div style={{ position: "absolute", inset: 0, borderRadius: 8, border: `1.5px solid ${props.color}` }} />
      )}
      <div
        style={{
          width: props.selected ? 12 : 10,
          height: props.selected ? 12 : 10,
          borderRadius: "50%",
          background: props.color,
          boxShadow: props.color === "#FFFFFF" ? "0 0 0 0.75px rgba(0,0,0,0.18)" : "none",
        }}
      />
    </button>
  );
}

function EditorSegments(props: {
  segments: { label?: string; icon?: IconName; title?: string }[];
  value: number;
  onChange(i: number): void;
}) {
  return (
    <div style={{ display: "flex", flexShrink: 0, background: "rgba(255,255,255,0.06)", borderRadius: 8, padding: 2, gap: 2 }}>
      {props.segments.map((segment, index) => (
        <button
          key={index}
          title={segment.title}
          onClick={() => props.onChange(index)}
          style={{
            minWidth: 28,
            height: 24,
            padding: "0 7px",
            borderRadius: 6,
            border: "none",
            background: props.value === index ? "#fff" : "transparent",
            color: props.value === index ? "#000" : "#fff",
            fontSize: 10,
            cursor: "pointer",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            gap: 4,
            whiteSpace: "nowrap",
          }}
        >
          {segment.icon ? <KiriIcon name={segment.icon} size={12} style={{ opacity: 0.85 }} /> : null}
          {segment.label}
        </button>
      ))}
    </div>
  );
}
