// OverlayWindow — capture overlay: mode selector, window hover, region
// selection, annotation toolbar, OCR, and recording options. Port of
// SelectionOverlayController.swift.

import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  api,
  dbg,
  DEFAULT_RECORDING_OPTIONS,
  frozenImageUrl,
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
  type MosaicStyle,
  type TextBackgroundStyle,
  type Tool,
} from "../annotation/model";
import AnnotationCanvas, { type AnnotationCanvasHandle } from "../annotation/AnnotationCanvas";
import { KiriIcon, type IconName } from "../components/KiriIcons";

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
function reportFrontend(message: string) {
  void import("@tauri-apps/api/core").then(({ invoke }) => {
    invoke("log_frontend_error", { message });
  });
}

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
  const [frozenSrc, setFrozenSrc] = useState<string>("");
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
  const [ocrFailed, setOcrFailed] = useState(false);
  const [recordOptions, setRecordOptions] = useState<RecordingOptions>(DEFAULT_RECORDING_OPTIONS);
  const [micSupported, setMicSupported] = useState(true);
  const [moreMenuOpen, setMoreMenuOpen] = useState(false);
  const canvasRef = useRef<AnnotationCanvasHandle>(null);
  const imageRef = useRef<HTMLImageElement>(null);
  const modeRef = useRef<Mode>("screenshot");
  modeRef.current = mode;

  // Load context on mount.
  useEffect(() => {
    dbg(`overlay mount: inner=${window.innerWidth}x${window.innerHeight} dpr=${window.devicePixelRatio}`);
    (window as unknown as { __kiriOverlay: boolean }).__kiriOverlay = true;
    api.startCapture()
      .then((ctx) => {
        dbg(`overlay startCapture ok: display=${ctx.displayWidth}x${ctx.displayHeight} windows=${ctx.windowRects.length} scale=${ctx.scale}`);

        setContext(ctx);
      })
      .catch((error) => {
        void import("@tauri-apps/api/core").then(({ invoke }) => {
          invoke("log_frontend_error", {
            message: `overlay startCapture rejected: ${String(error)}`,
          });
        });
        // Permission or capture failure: close the overlay so the library
        // window's error banner (emitted by the backend) is visible.
        void import("@tauri-apps/api/window").then(({ getCurrentWindow }) => {
          void getCurrentWindow().close();
        });
      });
    api.getRecordingOptions().then((options) => setRecordOptions(options)).catch(() => {});
    api.micSupported().then((supported) => setMicSupported(supported)).catch(() => {});
    // Load the frozen capture through a blob URL: canvas operations on the
    // custom-scheme image would taint the canvas and break PNG export.
    fetch(frozenImageUrl())
      .then((response) => response.blob())
      .then((blob) => {
        const img = new Image();
        img.onload = () => {
          dbg(`frozen img loaded: ${img.naturalWidth}x${img.naturalHeight}`);
        };
        img.onerror = () => {
          dbg("frozen img ERROR (blob)");
        };
        img.src = URL.createObjectURL(blob);
        setFrozenSrc(img.src);
      })
      .catch((error) => dbg(`frozen fetch failed: ${String(error)}`));
  }, []);

  const bounds: Rect = context
    ? { x: 0, y: 0, width: context.displayWidth, height: context.displayHeight }
    : { x: 0, y: 0, width: window.innerWidth, height: window.innerHeight };

  const cancel = useCallback(() => {
    dbg(`cancel() phase=${phaseRef.current}`);
    void api.cancelCapture().catch(() => {});
  }, []);

  const complete = useCallback(
    async (action: "copy" | "save" | "pin" | "edit") => {
      const canvas = canvasRef.current;
      if (!canvas) {
        reportFrontend(`complete(${action}): annotation canvas not mounted`);
        return;
      }
      const png = await canvas.exportPng();
      if (!png) {
        reportFrontend(`complete(${action}): exportPng returned no data`);
        return;
      }
      dbg(`complete(${action}) pngBytes=${png.byteLength}`);
      const bytes = Array.from(png);
      try {
        await api.confirmCapture(bytes, action);
        dbg(`complete(${action}) confirmCapture ok`);
      } catch (error) {
        reportFrontend(`confirm_capture(${action}) rejected: ${String(error)}`);
      }
    },
    [],
  );

  const runOcr = useCallback(
    async (sel: Rect) => {
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
        setOcrFailed(false);
        setOcrText(text);
        setPhase("ocr-result");
      } catch {
        // The backend returns "No Text Found" for empty results and
        // "Text Recognition Failed" for real failures.
        setOcrFailed(true);
        setOcrText("");
        setPhase("ocr-result");
      }
    },
    [bounds],
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
        const ph = phaseRef.current;
        if (ph === "annotating") {
          void complete("copy");
          return;
        }
        if (ph === "record-options" && selectionRef.current) {
          // Spec (recording §2.2): Start Recording is bound to Return.
          void api.startRecordingFlow(selectionRef.current, recordOptionsRef.current).catch(() => {});
          return;
        }
        if (ph === "selecting" && selectionRef.current && isValidSelection(selectionRef.current, 3)) {
          // Spec §5.2: Return with a valid selection confirms per mode.
          if (modeRef.current === "screenshot") {
            void complete("copy");
          } else if (modeRef.current === "record") {
            setPhase("record-options");
          } else if (modeRef.current === "ocr") {
            void runOcr(selectionRef.current);
          }
          return;
        }
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
          // Spec §6.6: switching tools commits any in-flight text edit.
          canvasRef.current?.commitTextEditing();
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
  }, [cancel, complete, runOcr]);

  const phaseRef = useRef<Phase>("mode-select");
  phaseRef.current = phase;
  const selectionRef = useRef<Rect | null>(null);
  selectionRef.current = selection;
  const hoverWindowRef = useRef<Rect | null>(null);
  const recordOptionsRef = useRef<RecordingOptions>(recordOptions);
  recordOptionsRef.current = recordOptions;

  // --- pointer interactions ---
  const toPoint = useCallback(
    (e: React.PointerEvent): Point => ({ x: e.clientX, y: e.clientY }),
    [],
  );

  const onPointerDown = useCallback(
    (e: React.PointerEvent) => {
      if (phaseRef.current !== "mode-select" && phaseRef.current !== "selecting") return;
      const p = clampPoint(toPoint(e), bounds);
      dbg(`pointerDown at ${p.x.toFixed(0)},${p.y.toFixed(0)} phase=${phaseRef.current}`);
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
      const interactive = phaseRef.current === "mode-select" || phaseRef.current === "selecting";
      if (!interactive) return;
      // Hover outline while not dragging. OCR mode never hovers windows
      // (spec §2.1: hoveredWindowSelection is always nil for .ocr).
      if (!drag && !resizeHandle && !moveDrag) {
        const candidate =
          modeRef.current !== "ocr" && context
            ? windowCandidate(p, context.windowRects, bounds)
            : null;
        if (candidate !== hoverWindowRef.current) {
          hoverWindowRef.current = candidate;
          setHoverWindow(candidate);
        }
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
    },
    [toPoint, bounds, context, drag, resizeHandle, moveDrag],
  );

  const onPointerUp = useCallback(
    (e: React.PointerEvent) => {
      const p = clampPoint(toPoint(e), bounds);
      dbg(`pointerUp at ${p.x.toFixed(0)},${p.y.toFixed(0)} phase=${phaseRef.current} drag=${!!drag}`);
      if (resizeHandle || moveDrag) {
        setResizeHandle(null);
        setMoveDrag(null);
        setDrag(null);
        return;
      }
      if (drag) {
        const moved = Math.hypot(p.x - drag.start.x, p.y - drag.start.y) >= 3;
        if (!moved && context && modeRef.current !== "ocr") {
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
      // Spec §7.1: once a region is chosen, the toolbar appears immediately
      // but the region stays adjustable (phase stays .selecting, the
      // annotation canvas stays hidden). Picking a tool (or its shortcut)
      // is what locks the region into .annotating.
      setPhase("selecting");
      setTool("select");
    } else if (modeRef.current === "ocr") {
      setPhase("ocr-drag");
      void runOcr(sel);
    } else {
      setPhase("record-options");
    }
  }

  // Spec §2.4 changeCaptureMode: switching the mode resets to the selection
  // phase, tears down the recording-options popover and the OCR panel, and
  // keeps or clears the region per mode (OCR always clears it). The mode
  // selector stays visible throughout (spec §1.2), so this can be invoked
  // at any point.
  const switchMode = useCallback(
    (next: Mode) => {
      if (next === modeRef.current) return;
      modeRef.current = next;
      setMode(next);
      setPhase("selecting");
      setDrag(null);
      setResizeHandle(null);
      setMoveDrag(null);
      setHoverWindow(null);
      hoverWindowRef.current = null;
      setOcrText("");
      setOcrFailed(false);
      setTool("select");
      if (next === "ocr") {
        // Spec: OCR always clears the selection and re-draws a region.
        setSelection(null);
      } else if (next === "record" && selectionRef.current) {
        // Existing region + record mode → show the recording options.
        setPhase("record-options");
      } else if (next === "screenshot" && selectionRef.current) {
        // Existing region + screenshot → toolbar re-appears (selecting).
        setPhase("selecting");
      } else {
        setSelection(null);
      }
      // With a valid region: screenshot re-shows the toolbar (selecting
      // phase with a selection); record shows the options popover.
    },
    [],
  );

  // --- toolbar placement (spec §7.6) ---
  // Defaults to 10pt below the selection, centered; flips above when the
  // bottom would overflow; x/y clamped with an 8pt margin.
  const toolbarAnchor = useMemo(() => {
    if (!selection) return { x: 0, y: 0 };
    const below = selection.y + selection.height + 10;
    const above = selection.y - 10;
    return {
      x: selection.x + selection.width / 2,
      // Spec §7.6: below the selection by default; flip above when the
      // bottom edge would overflow (48 = toolbar height, 8 = margin).
      y: below + 48 + 8 > bounds.height ? above : below,
    };
  }, [selection, bounds]);

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
        // While the frozen capture is still loading, stay translucent so the
        // live screen shows through; the window becomes opaque once the
        // frozen image is ready (mirroring the original's freeze behavior).
        background: frozenSrc ? "#141414" : "rgba(0,0,0,0.25)",
        overflow: "hidden",
        cursor: phase === "selecting" ? "crosshair" : "default",
      }}
      onPointerDown={phase === "annotating" ? undefined : onPointerDown}
      onPointerMove={phase === "annotating" ? undefined : onPointerMove}
      onPointerUp={phase === "annotating" ? undefined : onPointerUp}
      onContextMenu={(e) => {
        // Spec §1.6: right-click returns to region selection while
        // annotating (tearing down annotations); otherwise it cancels.
        e.preventDefault();
        if (phase === "annotating") {
          setPhase("selecting");
          setSelection(null);
          setTool("select");
          setAppearance(DEFAULT_APPEARANCE);
        } else {
          cancel();
        }
      }}
    >
      {context && frozenSrc && (
        <img
          ref={imageRef}
          src={frozenSrc}
          alt=""
          draggable={false}
          style={{ position: "absolute", inset: 0, width: "100%", height: "100%", pointerEvents: "none" }}
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
          <SizeBadge rect={displayRect} bounds={bounds} />
        </>
      )}

      {/* Selection border while annotating: white 4pt + violet 2pt */}
      {displayRect && phase === "annotating" && (
        <>
          <div
            style={{
              position: "absolute",
              left: displayRect.x,
              top: displayRect.y,
              width: displayRect.width,
              height: displayRect.height,
              border: "4px solid rgba(255,255,255,0.92)",
              boxSizing: "border-box",
              pointerEvents: "none",
              zIndex: 5,
            }}
          />
          <div
            style={{
              position: "absolute",
              left: displayRect.x,
              top: displayRect.y,
              width: displayRect.width,
              height: displayRect.height,
              border: `2px solid ${ACCENT}`,
              boxSizing: "border-box",
              pointerEvents: "none",
              zIndex: 5,
            }}
          />
        </>
      )}

      {/* Annotation canvas — mounted (hidden) as soon as a region is chosen
          (spec §7.1: the canvas exists but is hidden until a tool is picked),
          so Done/Return export works without picking a tool. */}
      {selection && (annotating || phase === "selecting") && (
        <div
          style={{
            position: "absolute",
            left: selection.x,
            top: selection.y,
            width: selection.width,
            height: selection.height,
            visibility: annotating ? "visible" : "hidden",
            pointerEvents: annotating ? "auto" : "none",
          }}
        >
          <AnnotationCanvas
            ref={canvasRef}
            image={imageRef.current}
            displaySize={
              context
                ? { width: context.displayWidth, height: context.displayHeight }
                : undefined
            }
            region={{ x: selection.x, y: selection.y, width: selection.width, height: selection.height }}
            tool={tool}
            appearance={appearance}
            onHistoryChange={(u, r) => {
              setCanUndo(u);
              setCanRedo(r);
            }}
            onCancel={cancel}
            onFinishAfterTextCommit={() => void complete("copy")}
          />
        </div>
      )}

      {/* Mode selector — ALWAYS visible (spec §1.2: never hidden), so the
          mode can be switched at any point: before/during selection and
          after a region is chosen (spec §2.4 changeCaptureMode). */}
      {phase !== "ocr-result" && (
        <>
          <div
            className="kiri-hud kiri-mode-select"
            onPointerDown={(e) => e.stopPropagation()}
          >
            <ModeButton
              active={mode === "screenshot"}
              icon="camera.viewfinder"
              label={t("Screenshot")}
              onClick={() => switchMode("screenshot")}
            />
            <ModeButton
              active={mode === "record"}
              icon="record.circle"
              label={t("Record")}
              onClick={() => switchMode("record")}
            />
            <ModeButton
              active={mode === "ocr"}
              icon="text.viewfinder"
              label={t("OCR")}
              onClick={() => switchMode("ocr")}
            />
          </div>
          {!selection && (
            <HintLabel
              text={
                mode === "screenshot"
                  ? t("Drag to choose a capture area   ·   Click a window   ·   Esc to cancel")
                  : mode === "record"
                    ? t("Drag to choose a recording area   ·   Click a window   ·   Esc to cancel")
                    : t("Drag to choose text to recognize   ·   Esc to cancel")
              }
              bottom={88 + 44 + 10}
            />
          )}
        </>
      )}

      {/* OCR states */}
      {phase === "ocr-drag" && <HintLabel text={t("Recognizing Text…")} bottom={88 + 44 + 10} />}

      {/* Drag hint while creating a region (spec §3.2.5: shown when no
          toolbar exists yet and the user is dragging). */}
      {phase === "selecting" && drag && drag.moved && !selection && (
        <HintLabel
          text={
            mode === "screenshot"
              ? t("Release to show tools")
              : mode === "record"
                ? t("Release for recording settings")
                : t("Release to recognize text")
          }
          bottom={88 + 44 + 10}
        />
      )}
      {phase === "selecting" && selection && !drag && (
        <HintLabel
          text={
            mode === "screenshot"
              ? t("Drag handles to resize · Drag inside to move")
              : mode === "record"
                ? t("Adjust the region · Recording settings below")
                : t("Release to recognize text")
          }
          bottom={88 + 44 + 10}
        />
      )}
      {phase === "ocr-result" && (
        <OcrPanel
          text={ocrText}
          failed={ocrFailed}
          onCopy={() => {
            void api.copyText(ocrText).catch(() => {});
          }}
          onClose={cancel}
        />
      )}

      {/* Recording options */}
      {phase === "record-options" && selection && (
        <RecordOptionsPanel
          anchor={selection}
          bounds={bounds}
          options={recordOptions}
          micSupported={micSupported}
          onChange={(next) => {
            // Spec (recording §3): persist each toggle change immediately.
            setRecordOptions(next);
            void api.setRecordingOptions(next).catch(() => {});
          }}
          onStart={() => {
            void api.startRecordingFlow(selection, recordOptions).catch(() => {});
          }}
          onCancel={cancel}
        />
      )}

      {/* Toolbar — appears as soon as a region is chosen (spec §7.1); the
          region stays adjustable until a tool is picked, which locks it. */}
      {(annotating || (phase === "selecting" && selection)) && (
        <Toolbar
          anchor={toolbarAnchor}
          bounds={bounds}
          tool={tool}
          setTool={(next) => {
            // Spec §6.6: switching tools commits any in-flight text edit
            // (commitTextEditing is a no-op when nothing is being edited).
            canvasRef.current?.commitTextEditing();
            if (phase === "selecting") {
              // Picking a tool locks the region into annotation mode.
              setPhase("annotating");
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
            // Spec §10.2: reselecting tears down the annotation UI and
            // resets tool/appearance to defaults.
            setPhase("selecting");
            setSelection(null);
            setTool("select");
            setAppearance(DEFAULT_APPEARANCE);
          }}
          onSaveAs={() => void complete("save")}
          onPin={() => void complete("pin")}
          onOpenEditor={() => void complete("edit")}
          onClear={() => canvasRef.current?.clearAnnotations()}
          onTextFontBegin={() => canvasRef.current?.beginTextFontSizeAdjustment()}
          onTextFontLive={(value) => canvasRef.current?.setTextFontSizeLive(value)}
          onTextFontEnd={() => canvasRef.current?.endTextFontSizeAdjustment()}
        />
      )}

    </div>
  );
}

// ---------------------------------------------------------------------------

function ModeButton(props: {
  active: boolean;
  icon: IconName;
  label: string;
  onClick(): void;
}) {
  return (
    <button
      onClick={props.onClick}
      className="kiri-mode-btn"
      data-active={props.active || undefined}
    >
      <span style={{ display: "flex", alignItems: "center", gap: 6 }}>
        <KiriIcon name={props.icon} size={16} />
        {props.label}
      </span>
    </button>
  );
}

function HintLabel(props: { text: string; bottom: number }) {
  return (
    <div
      style={{
        position: "absolute",
        left: "50%",
        bottom: props.bottom,
        transform: "translateX(-50%)",
        background: "rgba(0,0,0,0.72)",
        border: "1px solid rgba(255,255,255,0.16)",
        borderRadius: "999px",
        color: "#fff",
        padding: "9px 15px",
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
          // Spec §3.2.4: white outer circle (10pt) with an accent inner
          // circle (6pt) — a white-dot-with-violet-core handle.
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
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              boxShadow: "0 0 2px rgba(0,0,0,0.3)",
              pointerEvents: "none",
            }}
          >
            <div
              style={{
                width: 6,
                height: 6,
                borderRadius: "50%",
                background: ACCENT,
              }}
            />
          </div>
        );
      })}
    </>
  );
}

function SizeBadge(props: { rect: Rect; bounds: Rect }) {
  const { rect, bounds } = props;
  const label = `${Math.round(rect.width)} × ${Math.round(rect.height)}`;
  // Spec §3.2.3: badge sits 6pt above the selection, x clamped to
  // [6, bounds.maxX - badgeWidth - 6]; if it would clip the top edge,
  // it moves inside the selection's top instead.
  const width = Math.max(48, label.length * 7 + 16);
  const height = 22;
  const rawX = rect.x;
  const left = Math.min(Math.max(6, rawX), Math.max(6, bounds.width - width - 6));
  const outside = rect.y - height - 6;
  const top = outside >= 6 ? outside : rect.y + 6;
  return (
    <div
      style={{
        position: "absolute",
        left,
        top,
        width,
        height,
        background: "rgba(0,0,0,0.76)",
        border: "1px solid rgba(255,255,255,0.16)",
        borderRadius: height / 2,
        color: "#fff",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        font: "500 11px ui-monospace, SFMono-Regular, Menlo, monospace",
        pointerEvents: "none",
        boxSizing: "border-box",
      }}
    >
      {label}
    </div>
  );
}

function OcrPanel(props: { text: string; failed: boolean; onCopy(): void; onClose(): void }) {
  const { text, failed, onCopy, onClose } = props;
  return (
    <div
      className="kiri-hud"
      onPointerDown={(e) => e.stopPropagation()}
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
        <span style={{ font: "600 12.5px var(--kiri-font-ui)" }}>
          {failed ? t("Text Recognition Failed") : t("Recognized Text")}
        </span>
        <button
          onClick={onClose}
          style={{ ...iconButtonStyle, display: "flex", alignItems: "center", justifyContent: "center" }}
        >
          <KiriIcon name="xmark" size={11} />
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
        {text || (failed ? t("Adjust the region and try again") : t("No Text Found"))}
      </div>
      <button className="kiri-primary-button" onClick={onCopy} disabled={!text}>
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
  cursor: "pointer",
};

function RecordOptionsPanel(props: {
  anchor: Rect;
  bounds: Rect;
  options: RecordingOptions;
  micSupported: boolean;
  onChange(options: RecordingOptions): void;
  onStart(): void;
  onCancel(): void;
}) {
  const { anchor, bounds, options, micSupported, onChange, onStart, onCancel } = props;
  const toggle = (key: keyof RecordingOptions) => {
    const next = { ...options, [key]: !options[key] };
    if (key === "showsCursor" && !next.showsCursor) next.highlightsClicks = false;
    onChange(next);
  };
  // Panel geometry: 336 wide; prefer below the selection, flip above when
  // the bottom would overflow, and as a last resort pin inside the screen so
  // a huge selection can never push the panel off-screen. x centered on the
  // selection, clamped with an 8pt margin.
  const PANEL_W = 336;
  const PANEL_H = 400;
  const margin = 8;
  const maxTop = Math.max(margin, bounds.height - PANEL_H - margin);
  const centeredTop = Math.max(margin, Math.min(maxTop, bounds.height / 2 - PANEL_H / 2 + 30));
  // Keep a small preference for hugging the selection when it's small, so
  // the panel feels attached; fall back to the centered position for big
  // selections.
  const below = anchor.y + anchor.height + 10;
  const above = anchor.y - PANEL_H - 10;
  let top: number;
  if (anchor.height > bounds.height - 240 || anchor.width > bounds.width - 240) {
    top = centeredTop;
  } else {
    const preferred = below + PANEL_H + margin > bounds.height ? above : below;
    top = Math.min(Math.max(margin, preferred), maxTop);
  }
  const centerX = anchor.x + anchor.width / 2 - PANEL_W / 2;
  const left = Math.min(
    Math.max(margin, centerX),
    Math.max(margin, bounds.width - PANEL_W - margin),
  );
  return (
    <div
      className="kiri-hud"
      onPointerDown={(e) => e.stopPropagation()}
      style={{
        position: "absolute",
        left,
        top,
        padding: 12,
        width: PANEL_W,
        display: "flex",
        flexDirection: "column",
        gap: 4,
      }}
    >
      <div style={{ font: "600 12.5px var(--kiri-font-ui)", marginBottom: 6 }}>{t("Record Region")}</div>
      <div style={{ color: "rgba(255,255,255,0.7)", font: "400 11px var(--kiri-font-ui)", marginBottom: 6 }}>
        {t("MP4 · 30 fps · Saved locally · Never uploaded")}
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
        suffix={micSupported ? undefined : t("Requires macOS 15")}
        checked={options.capturesMicrophone}
        onToggle={() => toggle("capturesMicrophone")}
        disabled={!micSupported}
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
        <button
          style={{ ...iconButtonStyle, height: 36, display: "flex", alignItems: "center", justifyContent: "center" }}
          onClick={onCancel}
        >
          <KiriIcon name="xmark" size={11} />
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
  suffix?: string;
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
        cursor: props.disabled ? "default" : "pointer",
      }}
    >
      <span style={{ font: "400 12.5px var(--kiri-font-ui)" }}>
        {props.label}
        {props.suffix && (
          <span style={{ color: "rgba(255,255,255,0.55)", fontSize: 10.5, marginLeft: 6 }}>
            {props.suffix}
          </span>
        )}
      </span>
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
  anchor: { x: number; y: number };
  bounds: Rect;
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
  onTextFontBegin?(): void;
  onTextFontLive?(value: number): void;
  onTextFontEnd?(): void;
}

const TOOLS: { tool: Tool; icon: IconName; title: string }[] = [
  { tool: "select", icon: "cursorarrow", title: "Select (V)" },
  { tool: "pen", icon: "pencil.tip", title: "Pen (P)" },
  { tool: "rectangle", icon: "rectangle.dashed", title: "Rectangle (R)" },
  { tool: "line", icon: "line.diagonal", title: "Line (L)" },
  { tool: "arrow", icon: "arrow.up.right", title: "Arrow (A)" },
  { tool: "text", icon: "textformat", title: "Text (T)" },
  { tool: "mosaic", icon: "square.grid.3x3.fill", title: "Mosaic (M)" },
];

function Toolbar(props: ToolbarProps) {
  const {
    anchor,
    bounds,
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
    onTextFontBegin,
    onTextFontLive,
    onTextFontEnd,
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
  const barRef = useRef<HTMLDivElement>(null);
  const [barWidth, setBarWidth] = useState(420);
  // Measure the real toolbar width so the centering clamp keeps the whole
  // bar on screen (spec §7.6: translate, never shrink or wrap).
  useEffect(() => {
    const el = barRef.current;
    if (!el) return;
    const measure = () => setBarWidth(el.offsetWidth || 420);
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    return () => observer.disconnect();
  }, [tool, moreMenuOpen, appearance]);
  // Spec §7.6: centered on the selection's midX; clamp so the bar's left
  // and right edges stay ≥8pt inside the screen. y is already resolved.
  const left = Math.min(
    Math.max(8, anchor.x - barWidth / 2),
    Math.max(8, bounds.x + bounds.width - barWidth - 8),
  );
  const top = Math.min(Math.max(8, anchor.y), Math.max(8, bounds.y + bounds.height - toolbarHeight - 8));

  const sep = <div style={{ width: 1, height: 26, background: "rgba(255,255,255,0.14)", margin: "0 3px", flexShrink: 0 }} />;

  return (
    <>
      <div
        ref={barRef}
        className="kiri-hud"
        onPointerDown={(e) => e.stopPropagation()}
        style={{
          position: "absolute",
          left,
          top,
          display: "flex",
          alignItems: "center",
          gap: 3,
          padding: "6px 8px",
          boxShadow: "0 5px 14px rgba(0,0,0,0.28)",
        }}
      >
        <ToolButton icon="xmark" title={t("Cancel capture · Esc")} onClick={onCancel} />
        {sep}
        {TOOLS.map(({ tool: t2, icon, title }) => (
          <ToolButton
            key={t2}
            icon={icon}
            title={t(title)}
            active={tool === t2}
            onClick={() => setTool(t2)}
          />
        ))}
        {sep}
        {/* Context row */}
        {tool === "text" ? (
          <SegmentedControl
            segments={[
              { icon: "square.dashed", label: t("Transparent"), title: t("No background") },
              { icon: "moon.fill", label: t("Dark"), title: t("Dark background") },
            ]}
            value={appearance.textBackgroundStyle === "transparent" ? 0 : 1}
            onChange={(index) =>
              setAppearance({
                ...appearance,
                textBackgroundStyle: (["transparent", "dark"] as TextBackgroundStyle[])[index],
              })
            }
          />
        ) : tool === "mosaic" ? (
          <>
            <SegmentedControl
              segments={[
                { label: t("Pixel"), title: t("Pixel mosaic") },
                { label: t("Blur"), title: t("Gaussian blur") },
              ]}
              value={appearance.mosaicStyle === "pixel" ? 0 : 1}
              onChange={(index) =>
                setAppearance({
                  ...appearance,
                  mosaicStyle: (["pixel", "blur"] as MosaicStyle[])[index],
                })
              }
            />
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
          </>
        ) : null}
        {slider && (
          <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
            <input
              type="range"
              className="kiri-range"
              min={slider.min}
              max={slider.max}
              value={slider.value}
              onChange={(e) => {
                const value = Math.round(Number(e.target.value));
                slider.onChange(value);
                // Live preview for the selected text mark (spec §6.6).
                if (tool === "text") onTextFontLive?.(value);
              }}
              onPointerDown={() => {
                if (tool === "text") onTextFontBegin?.();
              }}
              onPointerUp={() => {
                if (tool === "text") onTextFontEnd?.();
              }}
              onPointerLeave={() => {
                if (tool === "text") onTextFontEnd?.();
              }}
            />
            <span className="kiri-toolbar-value">{slider.value}</span>
          </div>
        )}
        {sep}
        {COLOR_PRESETS.map((preset) => (
          <ColorSwatch
            key={preset}
            color={COLOR_HEX[preset]}
            selected={appearance.colorPreset === preset}
            onClick={() => setAppearance({ ...appearance, colorPreset: preset })}
          />
        ))}
        {sep}
        <ToolButton icon="arrow.uturn.backward" title={t("Undo (⌘Z)")} disabled={!canUndo} onClick={onUndo} />
        <ToolButton icon="arrow.uturn.forward" title={t("Redo (⇧⌘Z)")} disabled={!canRedo} onClick={onRedo} />
        <ToolButton icon="checkmark" title={t("Done — Copy to clipboard · Return")} primary onClick={onDone} />
        <div style={{ position: "relative" }}>
          <ToolButton icon="ellipsis.circle" title={t("More — Save, pin, edit, or clear")} onClick={() => setMoreMenuOpen(!moreMenuOpen)} />
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
              <MenuItem icon="crop" label={t("Reselect Region")} onClick={onReselect} />
              <div style={{ height: 1, background: "rgba(255,255,255,0.14)", margin: "4px 0" }} />
              <MenuItem icon="square.and.arrow.down" label={t("Save As…")} onClick={onSaveAs} />
              <MenuItem icon="pin" label={t("Pin on Screen")} onClick={onPin} />
              <MenuItem icon="slider.horizontal.3" label={t("Open in Editor")} onClick={onOpenEditor} />
              <div style={{ height: 1, background: "rgba(255,255,255,0.14)", margin: "4px 0" }} />
              <MenuItem icon="trash" label={t("Clear Annotations")} onClick={onClear} />
            </div>
          )}
        </div>
      </div>
    </>
  );
}

function ToolButton(props: {
  icon?: IconName;
  label?: string;
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
        // Spec §7.3: .tool selected = accent 0.18 fill + accent 0.32 border;
        // .primary = accentStrong fill + white 0.22 border + accent shadow.
        border: props.primary
          ? "1px solid rgba(255,255,255,0.22)"
          : props.active
            ? "1px solid rgba(125,105,245,0.32)"
            : "1px solid transparent",
        background: props.primary
          ? "#634FDB"
          : props.active
            ? "rgba(125,105,245,0.18)"
            : "transparent",
        color: props.active ? "#AB94FF" : "#fff",
        fontSize: 12,
        fontWeight: 600,
        cursor: props.disabled ? "default" : "pointer",
        opacity: props.disabled ? 0.35 : 1,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        boxShadow: props.primary ? "0 3px 7px rgba(99, 79, 219, 0.25)" : "none",
        transition: "transform 0.14s ease-out, background 0.14s ease-out",
      }}
      onMouseEnter={(e) => {
        if (props.primary && !props.disabled) {
          e.currentTarget.style.filter = "brightness(1.12)";
          e.currentTarget.style.boxShadow = "0 5px 12px rgba(99, 79, 219, 0.38)";
        } else if (!props.active && !props.disabled) {
          e.currentTarget.style.background = "rgba(125,105,245,0.10)";
        }
      }}
      onMouseLeave={(e) => {
        if (props.primary) {
          e.currentTarget.style.filter = "none";
          e.currentTarget.style.boxShadow = "0 3px 7px rgba(99, 79, 219, 0.25)";
        } else if (!props.active) {
          e.currentTarget.style.background = "transparent";
        }
      }}
      onMouseDown={(e) => {
        e.currentTarget.style.transform = "scale(0.94)";
      }}
      onMouseUp={(e) => {
        e.currentTarget.style.transform = "scale(1)";
      }}
      onPointerLeave={(e) => {
        e.currentTarget.style.transform = "scale(1)";
      }}
    >
      {props.icon ? (
        <KiriIcon name={props.icon} size={15} />
      ) : (
        props.label
      )}
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
        cursor: "pointer",
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
  width?: number;
  segments: { label?: string; icon?: IconName; title?: string }[];
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
            minWidth: props.width,
            height: 24,
            padding: "0 7px",
            borderRadius: 6,
            border: "none",
            background: props.value === index ? "#634FDB" : "transparent",
            color: "#fff",
            fontSize: 10,
            fontWeight: 500,
            cursor: "pointer",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            gap: 4,
            whiteSpace: "nowrap",
          }}
        >
          {segment.icon ? (
            <KiriIcon name={segment.icon} size={12} style={{ opacity: 0.85 }} />
          ) : null}
          {segment.label}
        </button>
      ))}
    </div>
  );
}

function MenuItem(props: { label: string; icon?: IconName; onClick(): void }) {
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
        cursor: "pointer",
        display: "flex",
        alignItems: "center",
        gap: 8,
      }}
    >
      {props.icon && (
        <span style={{ width: 14, display: "flex", justifyContent: "center", opacity: 0.75 }}>
          <KiriIcon name={props.icon} size={13} />
        </span>
      )}
      {props.label}
    </button>
  );
}
