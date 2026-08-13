// OverlayWindow — capture overlay: mode selector, window hover, region
// selection, annotation toolbar, OCR, and recording options. Port of
// SelectionOverlayController.swift.

import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  api,
  frozenImageUrl,
  onNotice,
  type CaptureContextDto,
  type RecordingOptions,
} from "../lib/ipc";
import { t } from "../i18n";
import type { Point, Rect } from "../annotation/geom";
import {
  ALL_HANDLES,
  clampPoint,
  contains,
  handlePoint,
  hitTestHandle,
  intersection,
  isValidSelection,
  maxX,
  maxY,
  minX,
  minY,
  normalized,
  resized,
  standardized,
} from "../annotation/geom";
import {
  COLOR_HEX,
  COLOR_PRESETS,
  DEFAULT_APPEARANCE,
  type AppearanceSettings,
  type MosaicIntensity,
  type TextBackgroundStyle,
  type Tool,
} from "../annotation/model";
import AnnotationCanvas, { type AnnotationCanvasHandle } from "../annotation/AnnotationCanvas";

type Phase =
  | "mode-select"
  | "selecting"
  | "annotating"
  | "ocr-drag"
  | "ocr-result"
  | "record-options";

type Mode = "screenshot" | "record" | "ocr";

const ACCENT = "#7D69F5";

// --- window hover candidate (WindowSelectionGeometry.candidate port) ---
function windowCandidate(
  p: Point,
  windowsFrontToBack: Rect[],
  bounds: Rect,
): Rect | null {
  const minimum = 8;
  for (const window of windowsFrontToBack) {
    const visible = intersection(standardized(window), bounds);
    if (
      visible.width >= minimum &&
      visible.height >= minimum &&
      contains(visible, p)
    ) {
      return visible;
    }
  }
  return null;
}

export function OverlayWindow() {
  const [context, setContext] = useState<CaptureContextDto | null>(null);
  const [phase, setPhase] = useState<Phase>("mode-select");
  const [mode, setMode] = useState<Mode>("screenshot");
  const [selection, setSelection] = useState<Rect | null>(null);
  const [hoverWindow, setHoverWindow] = useState<Rect | null>(null);
  const [drag, setDrag] = useState<{ start: Point; current: Point; moved: boolean } | null>(null);
  const [resizeHandle, setResizeHandle] = useState<string | null>(null);
  const [moveDrag, setMoveDrag] = useState<{ start: Point; original: Rect } | null>(null);
  const [tool, setTool] = useState<Tool>("select");
  const [appearance, setAppearance] = useState<AppearanceSettings>(DEFAULT_APPEARANCE);
  const [canUndo, setCanUndo] = useState(false);
  const [canRedo, setCanRedo] = useState(false);
  const [ocrText, setOcrText] = useState("");
  const [ocrBusy, setOcrBusy] = useState(false);
  const [recordOptions, setRecordOptions] = useState<RecordingOptions>({
    usesCountdown: true,
    capturesSystemAudio: false,
    capturesMicrophone: false,
    showsCursor: true,
    highlightsClicks: false,
  });
  const [moreMenuOpen, setMoreMenuOpen] = useState(false);
  const canvasRef = useRef<AnnotationCanvasHandle>(null);
  const imageRef = useRef<HTMLImageElement>(null);
  const modeRef = useRef<Mode>("screenshot");
  modeRef.current = mode;
  const toolRef = useRef<Tool>("select");
  toolRef.current = tool;

  // Load context on mount.
  useEffect(() => {
    (window as unknown as { __kiriOverlay: boolean }).__kiriOverlay = true;
    let unlistenNotice: (() => void) | undefined;
    onNotice(() => {}).then((unlisten) => {
      unlistenNotice = unlisten;
    });
    api.startCapture().then((ctx) => setContext(ctx)).catch(() => {});
    api.getRecordingOptions().then((options) => setRecordOptions(options));
    return () => {
      unlistenNotice?.();
    };
  }, []);

  const bounds: Rect = context
    ? { x: 0, y: 0, width: context.displayWidth, height: context.displayHeight }
    : { x: 0, y: 0, width: window.innerWidth, height: window.innerHeight };

  const cancel = useCallback(() => {
    void api.cancelCapture();
  }, []);

  const complete = useCallback(
    async (action: "copy" | "save" | "pin" | "edit") => {
      const canvas = canvasRef.current;
      if (canvas) {
        const png = await canvas.exportPng();
        if (png) {
          const bytes = Array.from(png);
          void api.confirmCapture(bytes, action);
          return;
        }
      }
      cancel();
    },
    [cancel],
  );

  // --- keyboard ---
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        cancel();
        return;
      }
      const mod = e.metaKey || e.ctrlKey;
      if (mod && e.key.toLowerCase() === "z") {
        e.preventDefault();
        if (e.shiftKey) canvasRef.current?.redo();
        else canvasRef.current?.undo();
        return;
      }
      if (mod && e.key.toLowerCase() === "c") {
        e.preventDefault();
        if (phaseRef.current === "annotating") void complete("copy");
        return;
      }
      if (mod && e.key.toLowerCase() === "s") {
        e.preventDefault();
        if (phaseRef.current === "annotating") void complete("save");
        return;
      }
      if (e.key === "Enter" || e.key === "Return") {
        if (phaseRef.current === "annotating") void complete("copy");
        return;
      }
      if (!mod && !e.altKey) {
        const key = e.key.toLowerCase();
        const map: Record<string, Tool> = {
          v: "select",
          p: "pen",
          r: "rectangle",
          l: "line",
          a: "arrow",
          t: "text",
          m: "mosaic",
        };
        if (key in map && phaseRef.current !== "mode-select") {
          const next = map[key];
          if (phaseRef.current === "selecting" && selectionRef.current) {
            // Selecting a tool locks the region into annotation mode.
            setPhase("annotating");
            setTool(next);
          } else {
            setTool(next);
          }
        }
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [cancel, complete]);

  const phaseRef = useRef<Phase>("mode-select");
  phaseRef.current = phase;
  const selectionRef = useRef<Rect | null>(null);
  selectionRef.current = selection;

  // --- pointer interactions ---
  const toPoint = useCallback(
    (e: React.PointerEvent): Point => ({ x: e.clientX, y: e.clientY }),
    [],
  );

  const onPointerDown = useCallback(
    (e: React.PointerEvent) => {
      if (phaseRef.current !== "mode-select" && phaseRef.current !== "selecting") return;
      const p = clampPoint(toPoint(e), bounds);
      if (phaseRef.current === "selecting" && selectionRef.current) {
        // Handle or move the existing selection.
        const handle = hitTestHandle(p, selectionRef.current, 10);
        if (handle) {
          setResizeHandle(handle);
          setDrag({ start: p, current: p, moved: false });
          return;
        }
        if (contains(selectionRef.current, p)) {
          setMoveDrag({ start: p, original: { ...selectionRef.current } });
          return;
        }
      }
      setDrag({ start: p, current: p, moved: false });
    },
    [toPoint, bounds],
  );

  const onPointerMove = useCallback(
    (e: React.PointerEvent) => {
      const p = clampPoint(toPoint(e), bounds);
      if (phaseRef.current === "mode-select") {
        setHoverWindow(context ? windowCandidate(p, context.windowRects, bounds) : null);
        return;
      }
      if (phaseRef.current === "selecting") {
        if (!drag && !resizeHandle && !moveDrag) {
          setHoverWindow(context ? windowCandidate(p, context.windowRects, bounds) : null);
          return;
        }
        setHoverWindow(null);
        if (resizeHandle && drag && selectionRef.current) {
          const resizedRect = resized(selectionRef.current, resizeHandle as never, p, bounds, 16);
          setSelection(resizedRect);
          setDrag({ ...drag, current: p, moved: true });
          return;
        }
        if (moveDrag) {
          const by = { x: p.x - moveDrag.start.x, y: p.y - moveDrag.start.y };
          const movedRect = {
            x: Math.min(Math.max(moveDrag.original.x + by.x, minX(bounds)), maxX(bounds) - moveDrag.original.width),
            y: Math.min(Math.max(moveDrag.original.y + by.y, minY(bounds)), maxY(bounds) - moveDrag.original.height),
            width: moveDrag.original.width,
            height: moveDrag.original.height,
          };
          setSelection(movedRect);
          return;
        }
        if (drag) {
          const moved = Math.hypot(p.x - drag.start.x, p.y - drag.start.y) >= 3;
          if (moved) {
            setSelection(normalized(drag.start, p));
            setDrag({ ...drag, current: p, moved: true });
          } else {
            setDrag({ ...drag, current: p });
          }
        }
      }
    },
    [toPoint, bounds, context, drag, resizeHandle, moveDrag],
  );

  const onPointerUp = useCallback(
    (e: React.PointerEvent) => {
      const p = clampPoint(toPoint(e), bounds);
      if (resizeHandle || moveDrag) {
        setResizeHandle(null);
        setMoveDrag(null);
        setDrag(null);
        return;
      }
      if (drag) {
        const moved = Math.hypot(p.x - drag.start.x, p.y - drag.start.y) >= 3;
        if (!moved && context) {
          const candidate = windowCandidate(p, context.windowRects, bounds);
          if (candidate) {
            setSelection(candidate);
            afterSelection(candidate);
          }
        } else if (selectionRef.current && isValidSelection(selectionRef.current, 3)) {
          afterSelection(selectionRef.current);
        }
        setDrag(null);
      }
    },
    [toPoint, bounds, context, drag, resizeHandle, moveDrag],
  );

  function afterSelection(sel: Rect) {
    if (modeRef.current === "screenshot") {
      setPhase("annotating");
      setTool("select");
    } else if (modeRef.current === "ocr") {
      setPhase("ocr-drag");
      void runOcr(sel);
    } else {
      setPhase("record-options");
    }
  }

  async function runOcr(sel: Rect) {
    setOcrBusy(true);
    const image = imageRef.current;
    if (!image) return;
    const scale = image.naturalWidth / bounds.width;
    const crop = document.createElement("canvas");
    crop.width = Math.round(sel.width * scale);
    crop.height = Math.round(sel.height * scale);
    const ctx = crop.getContext("2d")!;
    ctx.drawImage(
      image,
      sel.x * scale,
      sel.y * scale,
      sel.width * scale,
      sel.height * scale,
      0,
      0,
      crop.width,
      crop.height,
    );
    const blob = await new Promise<Blob | null>((resolve) => crop.toBlob(resolve, "image/png"));
    if (!blob) return;
    const bytes = Array.from(new Uint8Array(await blob.arrayBuffer()));
    try {
      const text = await api.recognizeText(bytes);
      setOcrText(text);
      setPhase("ocr-result");
    } catch {
      setOcrText("");
      setPhase("ocr-result");
    } finally {
      setOcrBusy(false);
    }
  }

  // --- toolbar placement ---
  const toolbarAnchor = useMemo(() => {
    if (!selection) return { x: 0, y: 0, above: true };
    return {
      x: selection.x + selection.width / 2,
      y: selection.y,
      above: selection.y >= 96,
    };
  }, [selection]);

  // --- render ---
  const selectingRect = drag && drag.moved && !resizeHandle ? normalized(drag.start, drag.current) : null;
  const displayRect = selection ?? selectingRect;
  const annotating = phase === "annotating";

  return (
    <div
      className="overlay-root kiri-dark"
      style={{
        position: "fixed",
        inset: 0,
        background: "#141414",
        overflow: "hidden",
        cursor: phase === "selecting" ? "crosshair" : "default",
      }}
      onPointerDown={phase === "annotating" ? undefined : onPointerDown}
      onPointerMove={phase === "annotating" ? undefined : onPointerMove}
      onPointerUp={phase === "annotating" ? undefined : onPointerUp}
    >
      {context && (
        <img
          ref={imageRef}
          src={frozenImageUrl()}
          alt=""
          draggable={false}
          style={{ position: "absolute", inset: 0, width: "100%", height: "100%", pointerEvents: "none" }}
          onLoad={() => setContext((c) => c)}
        />
      )}

      {/* Dim overlay */}
      {phase !== "annotating" && (
        <div
          style={{
            position: "absolute",
            inset: 0,
            background: "rgba(0,0,0,0.25)",
            pointerEvents: "none",
          }}
        />
      )}
      {/* Hover dim (spec: hover dims to 0.34, selection dims to 0.48) */}
      {(hoverWindow || displayRect) && phase !== "annotating" && (
        <div style={{ position: "absolute", inset: 0, pointerEvents: "none" }}>
          {(hoverWindow ?? displayRect) && (
            <div
              style={{
                position: "absolute",
                left: (hoverWindow ?? displayRect)!.x,
                top: (hoverWindow ?? displayRect)!.y,
                width: (hoverWindow ?? displayRect)!.width,
                height: (hoverWindow ?? displayRect)!.height,
                boxShadow: "0 0 0 9999px rgba(0,0,0," + (displayRect ? "0.48" : "0.34") + ")",
              }}
            />
          )}
        </div>
      )}

      {/* Window hover outline (violet 2pt α0.92) */}
      {hoverWindow && !displayRect && (
        <div
          style={{
            position: "absolute",
            left: hoverWindow.x,
            top: hoverWindow.y,
            width: hoverWindow.width,
            height: hoverWindow.height,
            border: `2px solid ${ACCENT}e6`,
            pointerEvents: "none",
          }}
        />
      )}

      {/* Selection outline: white 3 + violet 1.5 (4 + 2 while annotating) */}
      {displayRect && phase !== "annotating" && (
        <>
          <div
            style={{
              position: "absolute",
              left: displayRect.x,
              top: displayRect.y,
              width: displayRect.width,
              height: displayRect.height,
              border: "3px solid rgba(255,255,255,0.92)",
              boxSizing: "border-box",
              pointerEvents: "none",
            }}
          />
          <div
            style={{
              position: "absolute",
              left: displayRect.x,
              top: displayRect.y,
              width: displayRect.width,
              height: displayRect.height,
              border: `1.5px solid ${ACCENT}`,
              boxSizing: "border-box",
              pointerEvents: "none",
            }}
          />
          <SelectionHandles rect={displayRect} />
          <SizeBadge rect={displayRect} />
        </>
      )}

      {/* Annotation canvas */}
      {annotating && selection && (
        <div
          style={{
            position: "absolute",
            left: selection.x,
            top: selection.y,
            width: selection.width,
            height: selection.height,
          }}
        >
          <AnnotationCanvas
            ref={canvasRef}
            image={imageRef.current}
            region={{ x: selection.x, y: selection.y, width: selection.width, height: selection.height }}
            tool={tool}
            appearance={appearance}
            onHistoryChange={(u, r) => {
              setCanUndo(u);
              setCanRedo(r);
            }}
            onCancel={cancel}
            onToolChange={setTool}
          />
        </div>
      )}

      {/* Mode selector */}
      {phase === "mode-select" && (
        <>
          <div
            className="kiri-hud"
            style={{
              position: "absolute",
              left: "50%",
              bottom: 88,
              transform: "translateX(-50%)",
              display: "flex",
              gap: 4,
              padding: 6,
              alignItems: "center",
            }}
          >
            <ModeButton
              active={mode === "screenshot"}
              glyph={<Glyph name="camera" />}
              label={t("Screenshot")}
              onClick={() => setMode("screenshot")}
            />
            <ModeButton
              active={mode === "record"}
              glyph={<Glyph name="record" />}
              label={t("Record")}
              onClick={() => setMode("record")}
            />
            <ModeButton
              active={mode === "ocr"}
              glyph={<Glyph name="text" />}
              label={t("OCR")}
              onClick={() => setMode("ocr")}
            />
          </div>
          <HintLabel
            text={
              mode === "screenshot"
                ? t("Drag to choose a capture area   ·   Click a window   ·   Esc to cancel")
                : mode === "record"
                  ? t("Drag to choose a recording area   ·   Click a window   ·   Esc to cancel")
                  : t("Drag to choose text to recognize   ·   Esc to cancel")
            }
            y={128}
          />
        </>
      )}

      {/* OCR states */}
      {phase === "ocr-drag" && <HintLabel text={t("Recognizing Text…")} y={128} />}
      {phase === "ocr-result" && (
        <OcrPanel
          text={ocrText}
          onCopy={() => {
            void api.copyText(ocrText);
          }}
          onClose={cancel}
        />
      )}

      {/* Recording options */}
      {phase === "record-options" && selection && (
        <RecordOptionsPanel
          anchor={selection}
          options={recordOptions}
          onChange={setRecordOptions}
          onStart={() => {
            void api.startRecordingFlow(selection, recordOptions);
          }}
          onCancel={cancel}
        />
      )}

      {/* Toolbar */}
      {annotating && (
        <Toolbar
          anchor={toolbarAnchor}
          selection={selection!}
          tool={tool}
          setTool={(next) => {
            if (next === "text" || next === "select") {
              canvasRef.current?.commitTextEditing();
            }
            setTool(next);
          }}
          appearance={appearance}
          setAppearance={setAppearance}
          canUndo={canUndo}
          canRedo={canRedo}
          onUndo={() => canvasRef.current?.undo()}
          onRedo={() => canvasRef.current?.redo()}
          onDone={() => void complete("copy")}
          onCancel={cancel}
          moreMenuOpen={moreMenuOpen}
          setMoreMenuOpen={setMoreMenuOpen}
          onReselect={() => {
            setPhase("selecting");
            setSelection(null);
          }}
          onSaveAs={() => void complete("save")}
          onPin={() => void complete("pin")}
          onOpenEditor={() => void complete("edit")}
          onClear={() => canvasRef.current?.clearAnnotations()}
        />
      )}

      {/* Busy veil while OCR runs */}
      {ocrBusy && <div style={{ position: "absolute", inset: 0, pointerEvents: "none" }} />}
    </div>
  );
}

// ---------------------------------------------------------------------------

function Glyph(props: { name: string }) {
  return (
    <svg width="13" height="13" viewBox="0 0 16 16" style={{ display: "block" }}>
      {props.name === "camera" && (
        <rect x="1.5" y="4" width="13" height="9.5" rx="2.5" fill="none" stroke="currentColor" strokeWidth="1.6" />
      )}
      {props.name === "record" && (
        <circle cx="8" cy="8" r="5.5" fill="none" stroke="currentColor" strokeWidth="1.6" />
      )}
      {props.name === "text" && (
        <>
          <rect x="1.5" y="3" width="13" height="10" rx="2.5" fill="none" stroke="currentColor" strokeWidth="1.6" />
          <line x1="5" y1="6.5" x2="11" y2="6.5" stroke="currentColor" strokeWidth="1.4" />
          <line x1="5" y1="9.5" x2="9.5" y2="9.5" stroke="currentColor" strokeWidth="1.4" />
        </>
      )}
    </svg>
  );
}

function ModeButton(props: {
  active: boolean;
  glyph: React.ReactNode;
  label: string;
  onClick(): void;
}) {
  return (
    <button
      onClick={props.onClick}
      style={{
        display: "flex",
        alignItems: "center",
        gap: 6,
        height: 32,
        minWidth: 92,
        padding: "0 14px",
        borderRadius: 10,
        border: "none",
        background: props.active ? "#634FDB" : "transparent",
        color: "#fff",
        font: "600 12px " + "var(--kiri-font-ui)",
        cursor: "default",
      }}
    >
      {props.glyph}
      {props.label}
    </button>
  );
}

function HintLabel(props: { text: string; y: number }) {
  return (
    <div
      style={{
        position: "absolute",
        left: "50%",
        top: props.y,
        transform: "translateX(-50%)",
        background: "rgba(0,0,0,0.76)",
        border: "1px solid rgba(255,255,255,0.16)",
        borderRadius: 9,
        color: "#fff",
        padding: "5px 10px",
        font: "500 12px var(--kiri-font-ui)",
        whiteSpace: "pre",
        pointerEvents: "none",
      }}
    >
      {props.text}
    </div>
  );
}

function SelectionHandles(props: { rect: Rect }) {
  const { rect } = props;
  return (
    <>
      {ALL_HANDLES.map((handle) => {
        const p = handlePoint(handle, rect);
        return (
          <div
            key={handle}
            style={{
              position: "absolute",
              left: p.x - 5,
              top: p.y - 5,
              width: 10,
              height: 10,
              borderRadius: "50%",
              background: "#fff",
              boxShadow: `0 0 0 2px #fff inset, 0 0 0 5px ${ACCENT} inset`,
              pointerEvents: "none",
            }}
          />
        );
      })}
    </>
  );
}

function SizeBadge(props: { rect: Rect }) {
  const { rect } = props;
  const label = `${Math.round(rect.width)} × ${Math.round(rect.height)}`;
  const top = Math.max(0, rect.y - 24);
  const left = Math.min(Math.max(0, rect.x), 6);
  return (
    <div
      style={{
        position: "absolute",
        left,
        top,
        background: "rgba(0,0,0,0.76)",
        border: "1px solid rgba(255,255,255,0.16)",
        borderRadius: 9,
        color: "#fff",
        padding: "3px 8px",
        font: "500 11px ui-monospace, SFMono-Regular, Menlo, monospace",
        pointerEvents: "none",
      }}
    >
      {label}
    </div>
  );
}

function OcrPanel(props: { text: string; onCopy(): void; onClose(): void }) {
  const { text, onCopy, onClose } = props;
  return (
    <div
      className="kiri-hud"
      style={{
        position: "absolute",
        left: "50%",
        top: "50%",
        transform: "translate(-50%, -50%)",
        width: 336,
        padding: 14,
        display: "flex",
        flexDirection: "column",
        gap: 10,
      }}
    >
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
        <span style={{ font: "600 12.5px var(--kiri-font-ui)" }}>{t("Recognized Text")}</span>
        <button onClick={onClose} style={iconButtonStyle}>
          ✕
        </button>
      </div>
      <div
        style={{
          background: "#fff",
          color: "#1c1a24",
          borderRadius: 8,
          padding: "8px 10px",
          height: Math.min(160, Math.max(56, text.split("\n").length * 22 + 16)),
          overflow: "auto",
          font: "400 13px var(--kiri-font-ui)",
          userSelect: "text",
          whiteSpace: "pre-wrap",
        }}
      >
        {text || t("No Text Found")}
      </div>
      <button className="kiri-primary-button" onClick={onCopy}>
        {t("Copy")}
      </button>
    </div>
  );
}

const iconButtonStyle: React.CSSProperties = {
  width: 24,
  height: 24,
  borderRadius: 8,
  border: "none",
  background: "transparent",
  color: "#fff",
  fontSize: 11,
  cursor: "default",
};

function RecordOptionsPanel(props: {
  anchor: Rect;
  options: RecordingOptions;
  onChange(options: RecordingOptions): void;
  onStart(): void;
  onCancel(): void;
}) {
  const { anchor, options, onChange, onStart, onCancel } = props;
  const toggle = (key: keyof RecordingOptions) => {
    const next = { ...options, [key]: !options[key] };
    if (key === "showsCursor" && !next.showsCursor) next.highlightsClicks = false;
    onChange(next);
  };
  return (
    <div
      className="kiri-hud"
      style={{
        position: "absolute",
        left: Math.max(8, anchor.x),
        top: Math.max(8, anchor.y + anchor.height + 10),
        padding: 12,
        width: 260,
        display: "flex",
        flexDirection: "column",
        gap: 4,
      }}
    >
      <div style={{ font: "600 12.5px var(--kiri-font-ui)", marginBottom: 6 }}>{t("Record Region")}</div>
      <div style={{ color: "rgba(255,255,255,0.7)", font: "400 11px var(--kiri-font-ui)", marginBottom: 6 }}>
        {t("MP4 · 30 fps · Saved locally")}
      </div>
      <ToggleRow
        label={t("3-second countdown")}
        checked={options.usesCountdown}
        onToggle={() => toggle("usesCountdown")}
      />
      <ToggleRow
        label={t("System audio")}
        checked={options.capturesSystemAudio}
        onToggle={() => toggle("capturesSystemAudio")}
      />
      <ToggleRow
        label={t("Microphone")}
        checked={options.capturesMicrophone}
        onToggle={() => toggle("capturesMicrophone")}
      />
      <ToggleRow
        label={t("Show pointer")}
        checked={options.showsCursor}
        onToggle={() => toggle("showsCursor")}
      />
      <ToggleRow
        label={t("Highlight clicks")}
        checked={options.highlightsClicks}
        onToggle={() => toggle("highlightsClicks")}
        disabled={!options.showsCursor}
      />
      <div style={{ display: "flex", gap: 8, marginTop: 10 }}>
        <button className="kiri-primary-button" style={{ flex: 1 }} onClick={onStart}>
          {t("Start Recording")}
        </button>
        <button style={{ ...iconButtonStyle, height: 36 }} onClick={onCancel}>
          ✕
        </button>
      </div>
    </div>
  );
}

function ToggleRow(props: {
  label: string;
  checked: boolean;
  onToggle(): void;
  disabled?: boolean;
}) {
  return (
    <div
      onClick={props.disabled ? undefined : props.onToggle}
      style={{
        display: "flex",
        justifyContent: "space-between",
        alignItems: "center",
        padding: "5px 2px",
        opacity: props.disabled ? 0.4 : 1,
        cursor: "default",
      }}
    >
      <span style={{ font: "400 12.5px var(--kiri-font-ui)" }}>{props.label}</span>
      <div
        style={{
          width: 34,
          height: 20,
          borderRadius: 10,
          background: props.checked ? "#634FDB" : "rgba(255,255,255,0.2)",
          position: "relative",
          transition: "background 0.14s ease-out",
        }}
      >
        <div
          style={{
            position: "absolute",
            top: 2,
            left: props.checked ? 16 : 2,
            width: 16,
            height: 16,
            borderRadius: "50%",
            background: "#fff",
            transition: "left 0.14s ease-out",
          }}
        />
      </div>
    </div>
  );
}

interface ToolbarProps {
  anchor: { x: number; y: number; above: boolean };
  selection: Rect;
  tool: Tool;
  setTool(tool: Tool): void;
  appearance: AppearanceSettings;
  setAppearance(a: AppearanceSettings): void;
  canUndo: boolean;
  canRedo: boolean;
  onUndo(): void;
  onRedo(): void;
  onDone(): void;
  onCancel(): void;
  moreMenuOpen: boolean;
  setMoreMenuOpen(open: boolean): void;
  onReselect(): void;
  onSaveAs(): void;
  onPin(): void;
  onOpenEditor(): void;
  onClear(): void;
}

const TOOLS: { tool: Tool; label: string }[] = [
  { tool: "select", label: "V" },
  { tool: "pen", label: "P" },
  { tool: "rectangle", label: "R" },
  { tool: "line", label: "L" },
  { tool: "arrow", label: "A" },
  { tool: "text", label: "T" },
  { tool: "mosaic", label: "M" },
];

function Toolbar(props: ToolbarProps) {
  const {
    anchor,
    tool,
    setTool,
    appearance,
    setAppearance,
    canUndo,
    canRedo,
    onUndo,
    onRedo,
    onDone,
    onCancel,
    moreMenuOpen,
    setMoreMenuOpen,
    onReselect,
    onSaveAs,
    onPin,
    onOpenEditor,
    onClear,
  } = props;

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

  const toolbarHeight = 48;
  const left = Math.max(8, anchor.x);
  const top = anchor.above ? Math.max(8, anchor.y - toolbarHeight - 10) : Math.max(8, anchor.y + props.selection.height + 10);

  return (
    <>
      <div
        className="kiri-hud"
        style={{
          position: "absolute",
          left,
          top,
          transform: "translateX(-50%)",
          display: "flex",
          alignItems: "center",
          gap: 4,
          padding: "6px 7px",
          boxShadow: "0 5px 12px rgba(0,0,0,0.24)",
        }}
      >
        <ToolButton label="✕" title={t("Cancel capture · Esc")} onClick={onCancel} />
        <div style={{ width: 1, height: 24, background: "rgba(255,255,255,0.16)" }} />
        {TOOLS.map(({ tool: t2, label }) => (
          <ToolButton
            key={t2}
            label={label}
            title={t("Select (V)")}
            active={tool === t2}
            onClick={() => setTool(t2)}
          />
        ))}
        <div style={{ width: 1, height: 24, background: "rgba(255,255,255,0.16)" }} />
        {/* Context row */}
        {tool === "text" ? (
          <SegmentedControl
            width={26}
            segments={[
              { label: "", glyph: "none", title: t("Transparent background") },
              { label: "", glyph: "dark", title: t("Dark background") },
              { label: "", glyph: "light", title: t("Light background") },
            ]}
            value={appearance.textBackgroundStyle === "transparent" ? 0 : appearance.textBackgroundStyle === "dark" ? 1 : 2}
            onChange={(index) =>
              setAppearance({
                ...appearance,
                textBackgroundStyle: (["transparent", "dark", "light"] as TextBackgroundStyle[])[index],
              })
            }
          />
        ) : tool === "mosaic" ? (
          <SegmentedControl
            width={24}
            segments={[
              { label: "1", title: t("Soft") },
              { label: "2", title: t("Standard") },
              { label: "3", title: t("Strong") },
            ]}
            value={appearance.mosaicIntensity === "soft" ? 0 : appearance.mosaicIntensity === "standard" ? 1 : 2}
            onChange={(index) =>
              setAppearance({
                ...appearance,
                mosaicIntensity: (["soft", "standard", "strong"] as MosaicIntensity[])[index],
              })
            }
          />
        ) : null}
        {slider && (
          <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
            <input
              type="range"
              min={slider.min}
              max={slider.max}
              value={slider.value}
              onChange={(e) => slider.onChange(Math.round(Number(e.target.value)))}
              style={{ width: 76, accentColor: ACCENT }}
            />
            <span
              style={{
                width: 28,
                textAlign: "right",
                font: "500 9px ui-monospace, SFMono-Regular, Menlo, monospace",
              }}
            >
              {slider.value}
            </span>
          </div>
        )}
        <div style={{ width: 1, height: 24, background: "rgba(255,255,255,0.16)" }} />
        {COLOR_PRESETS.map((preset) => (
          <ColorSwatch
            key={preset}
            color={COLOR_HEX[preset]}
            selected={appearance.colorPreset === preset}
            onClick={() => setAppearance({ ...appearance, colorPreset: preset })}
          />
        ))}
        <div style={{ width: 1, height: 24, background: "rgba(255,255,255,0.16)" }} />
        <ToolButton label="↶" title={t("Undo (⌘Z)")} disabled={!canUndo} onClick={onUndo} />
        <ToolButton label="↷" title={t("Redo (⇧⌘Z)")} disabled={!canRedo} onClick={onRedo} />
        <ToolButton label="✓" title={t("Done — Copy to clipboard · Return")} primary onClick={onDone} />
        <div style={{ position: "relative" }}>
          <ToolButton label="⋯" title={t("More — Save, pin, edit, or clear")} onClick={() => setMoreMenuOpen(!moreMenuOpen)} />
          {moreMenuOpen && (
            <div
              className="kiri-hud"
              style={{
                position: "absolute",
                right: 0,
                top: 34,
                padding: 6,
                display: "flex",
                flexDirection: "column",
                minWidth: 190,
                zIndex: 10,
              }}
            >
              <MenuItem label={t("Reselect Region")} onClick={onReselect} />
              <MenuItem label={t("Save As…")} onClick={onSaveAs} />
              <MenuItem label={t("Pin on Screen")} onClick={onPin} />
              <MenuItem label={t("Open in Editor")} onClick={onOpenEditor} />
              <MenuItem label={t("Clear Annotations")} onClick={onClear} />
            </div>
          )}
        </div>
      </div>
    </>
  );
}

function ToolButton(props: {
  label: string;
  title?: string;
  active?: boolean;
  primary?: boolean;
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
        border: props.primary ? "1px solid rgba(255,255,255,0.22)" : "1px solid transparent",
        background: props.primary
          ? "linear-gradient(135deg, #634FDB, #7D69F5)"
          : props.active
            ? "rgba(125,105,245,0.32)"
            : "transparent",
        color: "#fff",
        fontSize: 12,
        fontWeight: 600,
        cursor: "default",
        opacity: props.disabled ? 0.35 : 1,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
      }}
    >
      {props.label}
    </button>
  );
}

function ColorSwatch(props: { color: string; selected: boolean; onClick(): void }) {
  return (
    <button
      onClick={props.onClick}
      style={{
        width: 22,
        height: 28,
        borderRadius: 8,
        border: "none",
        background: props.selected ? `${props.color}33` : "transparent",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        cursor: "default",
        position: "relative",
      }}
    >
      {props.selected && (
        <div
          style={{
            position: "absolute",
            inset: 0,
            borderRadius: 8,
            border: `1.5px solid ${props.color}`,
            boxSizing: "border-box",
          }}
        />
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

function SegmentedControl(props: {
  width: number;
  segments: { label: string; glyph?: string; title?: string }[];
  value: number;
  onChange(index: number): void;
}) {
  return (
    <div
      style={{
        display: "flex",
        background: "rgba(255,255,255,0.08)",
        borderRadius: 8,
        padding: 2,
        gap: 2,
      }}
    >
      {props.segments.map((segment, index) => (
        <button
          key={index}
          title={segment.title}
          onClick={() => props.onChange(index)}
          style={{
            width: props.width,
            height: 22,
            borderRadius: 6,
            border: "none",
            background: props.value === index ? "#634FDB" : "transparent",
            color: "#fff",
            fontSize: 9,
            fontWeight: 500,
            cursor: "default",
          }}
        >
          {segment.glyph === "none" ? (
            <span style={{ opacity: 0.7 }}>∅</span>
          ) : segment.glyph === "dark" ? (
            <span style={{ opacity: 0.7 }}>◼</span>
          ) : segment.glyph === "light" ? (
            <span style={{ opacity: 0.7 }}>◻</span>
          ) : (
            segment.label
          )}
        </button>
      ))}
    </div>
  );
}

function MenuItem(props: { label: string; onClick(): void }) {
  return (
    <button
      onClick={props.onClick}
      style={{
        background: "transparent",
        border: "none",
        color: "#fff",
        textAlign: "left",
        padding: "6px 10px",
        borderRadius: 8,
        font: "400 12.5px var(--kiri-font-ui)",
        cursor: "default",
      }}
    >
      {props.label}
    </button>
  );
}
