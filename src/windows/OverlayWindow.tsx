// OverlayWindow — capture overlay: mode selector, window hover, region
// selection, annotation toolbar, OCR, and recording options. Port of
// SelectionOverlayController.swift.

import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  api,
  DEFAULT_RECORDING_OPTIONS,
  type CaptureContextDto,
  type PreparedOcrRequestDto,
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
import { AnnotationInteractionLock } from "../annotation/interaction-lock.js";
import { KiriIcon, type IconName } from "../components/KiriIcons";
import { RemoteOcrConsent } from "../ocr/RemoteOcrConsent";

type Phase =
  | "mode-select"
  | "selecting"
  | "annotating"
  | "ocr-preparing"
  | "ocr-consent"
  | "ocr-recognizing"
  | "ocr-result"
  | "record-options";

type Mode = "screenshot" | "record" | "ocr";

const ACCENT = "#050505";
const MODE_SELECTOR_DRAG_THRESHOLD = 4;
const FLOATING_CONTROL_MARGIN = 8;
const DEFAULT_HINT_TOP = 102;

type ModeSelectorDrag = {
  pointerId: number;
  captureTarget: Element;
  start: Point;
  origin: Point;
  size: { width: number; height: number };
  moved: boolean;
};

function clampFloatingControl(position: Point, size: { width: number; height: number }, bounds: Rect): Point {
  const minLeft = minX(bounds) + FLOATING_CONTROL_MARGIN;
  const minTop = minY(bounds) + FLOATING_CONTROL_MARGIN;
  const maxLeft = Math.max(minLeft, maxX(bounds) - size.width - FLOATING_CONTROL_MARGIN);
  const maxTop = Math.max(minTop, maxY(bounds) - size.height - FLOATING_CONTROL_MARGIN);
  return {
    x: Math.min(Math.max(position.x, minLeft), maxLeft),
    y: Math.min(Math.max(position.y, minTop), maxTop),
  };
}

// --- window hover candidate (WindowSelectionGeometry.candidate port) ---
function reportFrontend(message: string) {
  void invoke("log_frontend_error", { message }).catch(() => {});
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
  const [preparedOcr, setPreparedOcr] = useState<PreparedOcrRequestDto | null>(null);
  const [remoteOcrFailed, setRemoteOcrFailed] = useState(false);
  const [recordOptions, setRecordOptions] = useState<RecordingOptions>(DEFAULT_RECORDING_OPTIONS);
  const [micSupported, setMicSupported] = useState(true);
  const [modeSelectorPosition, setModeSelectorPosition] = useState<Point | null>(null);
  const [modeSelectorDragging, setModeSelectorDragging] = useState(false);
  const canvasRef = useRef<AnnotationCanvasHandle>(null);
  const imageRef = useRef<HTMLImageElement>(null);
  const modeSelectorRef = useRef<HTMLDivElement>(null);
  const modeSelectorDragRef = useRef<ModeSelectorDrag | null>(null);
  const suppressModeSelectorClickRef = useRef(false);
  const completionLock = useMemo(() => new AnnotationInteractionLock(), []);
  const [completing, setCompleting] = useState(false);
  const modeRef = useRef<Mode>("screenshot");
  const preparedOcrRef = useRef<PreparedOcrRequestDto | null>(null);
  const ocrGenerationRef = useRef(0);
  modeRef.current = mode;

  // Load context on mount.
  useEffect(() => {
    let disposed = false;
    let failed = false;
    let frozenBlobUrl: string | null = null;
    const abortOverlay = (message: string) => {
      if (disposed || failed) return;
      failed = true;
      reportFrontend(message);
      void api.cancelCapture().catch(() => getCurrentWindow().close());
    };
    const captureToken = new URLSearchParams(window.location.search).get("captureToken");
    if (!captureToken || !/^[a-f0-9]{32}$/.test(captureToken)) {
      abortOverlay("overlay capture token is missing or invalid");
      return () => {
        disposed = true;
      };
    }
    const frozenCaptureUrl = `kiri://capture/frozen/${captureToken}.png`;
    (window as unknown as { __kiriOverlay: boolean }).__kiriOverlay = true;
    api.startCapture()
      .then((ctx) => {
        setContext(ctx);
      })
      .catch((error) => {
        void invoke("log_frontend_error", {
          message: `overlay startCapture rejected: ${String(error)}`,
        }).catch(() => {});
        // Permission or capture failure: close the overlay so the library
        // window's error banner (emitted by the backend) is visible.
        void getCurrentWindow().close();
      });
    api.getRecordingOptions().then((options) => setRecordOptions(options)).catch(() => {});
    api.micSupported().then((supported) => setMicSupported(supported)).catch(() => {});
    // Load the frozen capture through a blob URL: canvas operations on the
    // custom-scheme image would taint the canvas and break PNG export.
    fetch(frozenCaptureUrl)
      .then((response) => {
        if (!response.ok) throw new Error("frozen capture unavailable");
        return response.blob();
      })
      .then((blob) => {
        if (disposed) return;
        const img = new Image();
        img.onerror = () => abortOverlay("overlay could not decode the frozen capture");
        frozenBlobUrl = URL.createObjectURL(blob);
        img.src = frozenBlobUrl;
        setFrozenSrc(img.src);
      })
      .catch((error) =>
        abortOverlay(`overlay could not load the frozen capture: ${String(error)}`),
      );
    return () => {
      disposed = true;
      if (frozenBlobUrl) URL.revokeObjectURL(frozenBlobUrl);
    };
  }, []);

  const bounds: Rect = context
    ? { x: 0, y: 0, width: context.displayWidth, height: context.displayHeight }
    : { x: 0, y: 0, width: window.innerWidth, height: window.innerHeight };

  const modeSelectorPointerDown = (event: React.PointerEvent<HTMLDivElement>) => {
    if (
      completionLock.locked ||
      event.button !== 0 ||
      !event.isPrimary ||
      modeSelectorDragRef.current
    ) {
      return;
    }
    event.stopPropagation();
    const selector = modeSelectorRef.current;
    if (!selector) return;
    const rect = selector.getBoundingClientRect();
    const captureTarget = event.target instanceof Element ? event.target : event.currentTarget;
    suppressModeSelectorClickRef.current = false;
    modeSelectorDragRef.current = {
      pointerId: event.pointerId,
      captureTarget,
      start: { x: event.clientX, y: event.clientY },
      origin: { x: rect.left, y: rect.top },
      size: { width: rect.width, height: rect.height },
      moved: false,
    };
    captureTarget.setPointerCapture(event.pointerId);
  };

  const modeSelectorPointerMove = (event: React.PointerEvent<HTMLDivElement>) => {
    event.stopPropagation();
    const gesture = modeSelectorDragRef.current;
    if (!gesture || gesture.pointerId !== event.pointerId) return;
    if (completionLock.locked) return;
    const dx = event.clientX - gesture.start.x;
    const dy = event.clientY - gesture.start.y;
    if (!gesture.moved && Math.hypot(dx, dy) < MODE_SELECTOR_DRAG_THRESHOLD) return;
    event.preventDefault();
    if (!gesture.moved) {
      gesture.moved = true;
      setModeSelectorDragging(true);
    }
    setModeSelectorPosition(
      clampFloatingControl(
        { x: gesture.origin.x + dx, y: gesture.origin.y + dy },
        gesture.size,
        bounds,
      ),
    );
  };

  const finishModeSelectorDrag = (event: React.PointerEvent<HTMLDivElement>) => {
    event.stopPropagation();
    const gesture = modeSelectorDragRef.current;
    if (!gesture || gesture.pointerId !== event.pointerId) return;
    if (gesture.captureTarget.hasPointerCapture(event.pointerId)) {
      gesture.captureTarget.releasePointerCapture(event.pointerId);
    }
    modeSelectorDragRef.current = null;
    setModeSelectorDragging(false);
    if (gesture.moved) {
      suppressModeSelectorClickRef.current = true;
      window.setTimeout(() => {
        suppressModeSelectorClickRef.current = false;
      }, 0);
    }
  };

  const cancelModeSelectorDrag = (event: React.PointerEvent<HTMLDivElement>) => {
    event.stopPropagation();
    const gesture = modeSelectorDragRef.current;
    if (!gesture || gesture.pointerId !== event.pointerId) return;
    modeSelectorDragRef.current = null;
    setModeSelectorDragging(false);
  };

  useEffect(() => {
    setModeSelectorPosition((current) => {
      const selector = modeSelectorRef.current;
      if (!current || !selector) return current;
      const next = clampFloatingControl(
        current,
        { width: selector.offsetWidth, height: selector.offsetHeight },
        bounds,
      );
      return next.x === current.x && next.y === current.y ? current : next;
    });
  }, [bounds.x, bounds.y, bounds.width, bounds.height]);

  const discardPreparedOcr = useCallback(() => {
    ocrGenerationRef.current += 1;
    const pending = preparedOcrRef.current;
    preparedOcrRef.current = null;
    setPreparedOcr(null);
    setRemoteOcrFailed(false);
    if (pending) void api.cancelPreparedOcr(pending.requestId).catch(() => {});
  }, []);

  const cancel = useCallback(() => {
    discardPreparedOcr();
    void api.cancelCapture().catch(() => {});
  }, [discardPreparedOcr]);

  const complete = useCallback(
    async () => {
      if (!completionLock.acquire()) return;
      const modeSelectorGesture = modeSelectorDragRef.current;
      if (modeSelectorGesture) {
        if (modeSelectorGesture.captureTarget.hasPointerCapture(modeSelectorGesture.pointerId)) {
          modeSelectorGesture.captureTarget.releasePointerCapture(modeSelectorGesture.pointerId);
        }
        modeSelectorDragRef.current = null;
        setModeSelectorDragging(false);
      }
      setCompleting(true);
      try {
        const canvas = canvasRef.current;
        if (!canvas) {
          reportFrontend("complete: annotation canvas not mounted");
          return;
        }
        if (!selection) {
          reportFrontend("complete: annotation selection not available");
          return;
        }
        const result = await canvas.exportResult();
        if (!result) {
          reportFrontend("complete: exportResult returned no data");
          return;
        }
        await api.confirmCapture(result.png, { selection, document: result.document });
      } catch (error) {
        reportFrontend(`confirm_capture rejected: ${String(error)}`);
      } finally {
        completionLock.release();
        setCompleting(false);
      }
    },
    [completionLock, selection],
  );

  const recognizePreparedLocal = useCallback(async () => {
    const pending = preparedOcrRef.current;
    if (!pending) return;
    const generation = ocrGenerationRef.current;
    setRemoteOcrFailed(false);
    setPhase("ocr-recognizing");
    try {
      const text = await api.recognizePreparedOcrLocal(pending.requestId);
      if (generation !== ocrGenerationRef.current) return;
      preparedOcrRef.current = null;
      setPreparedOcr(null);
      setOcrFailed(false);
      setOcrText(text);
      setPhase("ocr-result");
    } catch {
      if (generation !== ocrGenerationRef.current) return;
      preparedOcrRef.current = null;
      setPreparedOcr(null);
      void api.cancelPreparedOcr(pending.requestId).catch(() => {});
      setOcrFailed(true);
      setOcrText("");
      setPhase("ocr-result");
    }
  }, []);

  const recognizePreparedRemote = useCallback(async () => {
    const pending = preparedOcrRef.current;
    const profile = pending?.profile;
    if (!pending || !profile || pending.engine.kind !== "profile") return;
    const generation = ocrGenerationRef.current;
    setRemoteOcrFailed(false);
    setPhase("ocr-recognizing");
    try {
      const text = await api.recognizePreparedOcrRemote(
        pending.requestId,
        profile.id,
        profile.revision,
      );
      if (generation !== ocrGenerationRef.current) return;
      preparedOcrRef.current = null;
      setPreparedOcr(null);
      setOcrFailed(false);
      setOcrText(text);
      setPhase("ocr-result");
    } catch {
      if (generation !== ocrGenerationRef.current) return;
      // Keep the same prepared image available for an explicit Retry or the
      // local-only action. A failed remote request is never retried silently.
      setRemoteOcrFailed(true);
      setPhase("ocr-consent");
    }
  }, []);

  const runOcr = useCallback(
    async (sel: Rect) => {
      const generation = ocrGenerationRef.current + 1;
      ocrGenerationRef.current = generation;
      const previous = preparedOcrRef.current;
      preparedOcrRef.current = null;
      setPreparedOcr(null);
      setRemoteOcrFailed(false);
      setOcrFailed(false);
      setOcrText("");
      setPhase("ocr-preparing");
      if (previous) void api.cancelPreparedOcr(previous.requestId).catch(() => {});

      try {
        const prepared = await api.prepareOcrRequest({
          x: sel.x,
          y: sel.y,
          width: sel.width,
          height: sel.height,
        });
        if (generation !== ocrGenerationRef.current) {
          void api.cancelPreparedOcr(prepared.requestId).catch(() => {});
          return;
        }

        preparedOcrRef.current = prepared;
        setPreparedOcr(prepared);
        if (prepared.engine.kind === "local") {
          void recognizePreparedLocal();
          return;
        }

        const profile = prepared.profile;
        if (
          !profile ||
          !profile.hasApiKey ||
          profile.id !== prepared.engine.profileId
        ) {
          preparedOcrRef.current = null;
          setPreparedOcr(null);
          void api.cancelPreparedOcr(prepared.requestId).catch(() => {});
          setOcrFailed(true);
          setPhase("ocr-result");
          return;
        }
        setPhase("ocr-consent");
      } catch {
        if (generation !== ocrGenerationRef.current) return;
        setOcrFailed(true);
        setOcrText("");
        setPhase("ocr-result");
      }
    },
    [recognizePreparedLocal],
  );

  useEffect(
    () => () => {
      ocrGenerationRef.current += 1;
      const pending = preparedOcrRef.current;
      preparedOcrRef.current = null;
      if (pending) void api.cancelPreparedOcr(pending.requestId).catch(() => {});
    },
    [],
  );

  // Auto-run OCR when the user switches to OCR mode with an existing valid
  // selection (reuse instead of re-drawing the region). Fires once per
  // transition because runOcr moves the phase to "ocr-result".
  useEffect(() => {
    if (mode === "ocr" && phase === "selecting" && selection && isValidSelection(selection, 3)) {
      void runOcr(selection);
    }
  }, [mode, phase, selection, runOcr]);

  // Esc is a window-level capture action. Register it separately in the
  // capture phase so focused text/number controls cannot consume it before
  // the overlay closes, while leaving their other keys (notably Return)
  // available to the normal bubbling shortcut handler below.
  useEffect(() => {
    const onEscape = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      e.stopImmediatePropagation();
      if (completionLock.locked) return;
      cancel();
    };
    window.addEventListener("keydown", onEscape, true);
    return () => window.removeEventListener("keydown", onEscape, true);
  }, [cancel, completionLock]);

  // --- keyboard ---
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (completionLock.locked) {
        e.preventDefault();
        e.stopPropagation();
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
        if (phaseRef.current === "annotating") void complete();
        return;
      }
      if (e.key === "Enter" || e.key === "Return") {
        const ph = phaseRef.current;
        if (ph === "ocr-consent") {
          // Privacy default: Return never sends an image to a remote provider.
          // It recognizes this one prepared image locally instead.
          e.preventDefault();
          void recognizePreparedLocal();
          return;
        }
        if (ph === "annotating") {
          void complete();
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
            void complete();
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
  }, [complete, completionLock, recognizePreparedLocal, runOcr]);

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
      // OCR results: allow drawing a fresh region directly (no button) —
      // clicking/dragging in blank space starts a new selection which
      // re-runs recognition on release. phaseRef must be updated
      // synchronously so the same pointer-down proceeds into the drag
      // start below (setPhase alone is async and would return early).
      if (phaseRef.current === "ocr-result") {
        setOcrText("");
        setOcrFailed(false);
        setPhase("selecting");
        phaseRef.current = "selecting";
        setSelection(null);
        selectionRef.current = null;
      }
      if (
        phaseRef.current !== "mode-select" &&
        phaseRef.current !== "selecting" &&
        phaseRef.current !== "record-options"
      ) return;
      const p = clampPoint(toPoint(e), bounds);
      if (
        (phaseRef.current === "selecting" || phaseRef.current === "record-options") &&
        selectionRef.current
      ) {
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
      const interactive =
        phaseRef.current === "mode-select" ||
        phaseRef.current === "selecting" ||
        phaseRef.current === "record-options" ||
        (phaseRef.current === "ocr-result" && !!drag);
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
        } else if (moved && selectionRef.current && isValidSelection(selectionRef.current, 3)) {
          // OCR (and screenshot/record) recognize only after an actual drag
          // release — a plain click must not trigger recognition.
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
      setPhase("ocr-preparing");
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
      if (completionLock.locked) return;
      if (next === modeRef.current) return;
      discardPreparedOcr();
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
        // Reuse an existing region when switching into OCR: run recognition
        // on it right away instead of forcing a fresh drag (the effect below
        // watches for a valid selection in OCR+selecting). Only clear when
        // there is no usable selection yet.
        if (selectionRef.current && isValidSelection(selectionRef.current, 3)) {
          setPhase("selecting");
        } else {
          setSelection(null);
        }
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
    [completionLock, discardPreparedOcr],
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
  const activeDimRect = displayRect ?? hoverWindow;
  const annotating = phase === "annotating";

  return (
    <div
      className="overlay-root kiri-dark"
      aria-busy={completing}
      data-interaction-disabled={completing || undefined}
      inert={completing}
      style={{
        position: "fixed",
        inset: 0,
        // While the frozen capture is still loading, stay translucent so the
        // live screen shows through; the window becomes opaque once the
        // frozen image is ready (mirroring the original's freeze behavior).
        background: frozenSrc ? "#141414" : "transparent",
        overflow: "hidden",
        cursor:
          completing
            ? "progress"
            : phase === "selecting" || phase === "record-options"
            ? "crosshair"
            : "default",
      }}
      onPointerDown={completing || phase === "annotating" ? undefined : onPointerDown}
      onPointerMove={completing || phase === "annotating" ? undefined : onPointerMove}
      onPointerUp={completing || phase === "annotating" ? undefined : onPointerUp}
      onContextMenu={(e) => {
        // Spec §1.6: right-click returns to region selection while
        // annotating (tearing down annotations); otherwise it cancels.
        e.preventDefault();
        if (completing) return;
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
      {!activeDimRect && (
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
      {activeDimRect && (
        <div style={{ position: "absolute", inset: 0, pointerEvents: "none" }}>
          <div
            style={{
              position: "absolute",
              left: activeDimRect.x,
              top: activeDimRect.y,
              width: activeDimRect.width,
              height: activeDimRect.height,
              boxShadow: "0 0 0 9999px rgba(0,0,0," + (displayRect ? "0.48" : "0.34") + ")",
            }}
          />
        </div>
      )}

      {/* Window hover outline: black edge stays legible against the white frozen-screen keyline. */}
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

      {/* Selection outline: white outer keyline + black inner keyline. */}
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
          <SizeBadge
            rect={displayRect}
            bounds={bounds}
            pixelScale={context?.scale ?? 1}
          />
        </>
      )}

      {/* Selection border while annotating: heavier white + black keylines. */}
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
            interactionDisabled={completing}
            interactionLock={completionLock}
            tool={tool}
            appearance={appearance}
            onHistoryChange={(u, r) => {
              setCanUndo(u);
              setCanRedo(r);
            }}
            onCancel={cancel}
            onFinishAfterTextCommit={() => void complete()}
          />
        </div>
      )}

      {/* Mode selector — ALWAYS visible (spec §1.2: never hidden), so the
          mode can be switched at any point: before/during selection and
          after a region is chosen (spec §2.4 changeCaptureMode). */}
      {phase !== "ocr-result" && (
        <>
          <div
            ref={modeSelectorRef}
            className="kiri-hud kiri-mode-select"
            data-dragging={modeSelectorDragging || undefined}
            style={
              modeSelectorPosition
                ? {
                    left: modeSelectorPosition.x,
                    top: modeSelectorPosition.y,
                    transform: "none",
                  }
                : undefined
            }
            onPointerDown={modeSelectorPointerDown}
            onPointerMove={modeSelectorPointerMove}
            onPointerUp={finishModeSelectorDrag}
            onPointerCancel={cancelModeSelectorDrag}
            onLostPointerCapture={cancelModeSelectorDrag}
            onClickCapture={(event) => {
              if (!suppressModeSelectorClickRef.current) return;
              event.preventDefault();
              event.stopPropagation();
              suppressModeSelectorClickRef.current = false;
            }}
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
              top={DEFAULT_HINT_TOP}
            />
          )}
        </>
      )}

      {/* OCR states */}
      {phase === "ocr-preparing" && <HintLabel text={t("Preparing Text…")} top={DEFAULT_HINT_TOP} />}
      {phase === "ocr-recognizing" && <HintLabel text={t("Recognizing Text…")} top={DEFAULT_HINT_TOP} />}

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
          top={DEFAULT_HINT_TOP}
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
          top={DEFAULT_HINT_TOP}
        />
      )}
      {phase === "ocr-result" && (
        <OcrPanel
          text={ocrText}
          failed={ocrFailed}
          anchor={selection ?? { x: 0, y: 0, width: bounds.width, height: 0 }}
          bounds={bounds}
          onCopy={() => {
            void api.copyText(ocrText).catch(() => {});
          }}
          onClose={cancel}
        />
      )}
      {phase === "ocr-consent" && preparedOcr?.profile && (
        <RemoteOcrConsent
          prepared={preparedOcr}
          anchor={selection ?? { x: 0, y: 0, width: bounds.width, height: 0 }}
          bounds={bounds}
          failed={remoteOcrFailed}
          onCancel={cancel}
          onUseLocal={() => void recognizePreparedLocal()}
          onSend={() => void recognizePreparedRemote()}
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
          canSetSize={phase === "selecting"}
          disabled={completing}
          onUndo={() => canvasRef.current?.undo()}
          onRedo={() => canvasRef.current?.redo()}
          onDone={() => void complete()}
          onCancel={cancel}
          onTextFontBegin={() => canvasRef.current?.beginTextFontSizeAdjustment()}
          onTextFontLive={(value) => canvasRef.current?.setTextFontSizeLive(value)}
          onTextFontEnd={() => canvasRef.current?.endTextFontSizeAdjustment()}
          onSetSize={(wPx, hPx) => {
            if (phase !== "selecting" || !selectionRef.current) return;
            const sc = context?.scale ?? 1;
            const w = Math.min(wPx / sc, bounds.width);
            const h = Math.min(hPx / sc, bounds.height);
            const cur = selectionRef.current;
            const x = Math.min(
              Math.max(0, cur.x + (cur.width - w) / 2),
              Math.max(0, bounds.width - w),
            );
            const y = Math.min(
              Math.max(0, cur.y + (cur.height - h) / 2),
              Math.max(0, bounds.height - h),
            );
            setSelection({ x, y, width: w, height: h });
          }}
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
      <span style={{ display: "flex", alignItems: "center", gap: 5 }}>
        <KiriIcon name={props.icon} size={14} />
        {props.label}
      </span>
    </button>
  );
}

function HintLabel(props: { text: string; top: number }) {
  return (
    <div
      style={{
        position: "absolute",
        left: "50%",
        top: props.top,
        transform: "translateX(-50%)",
        background: "rgba(8,8,8,0.86)",
        backdropFilter: "blur(12px)",
        WebkitBackdropFilter: "blur(12px)",
        border: "1px solid rgba(255,255,255,0.24)",
        borderRadius: "999px",
        color: "#fff",
        padding: "6px 11px",
        font: "550 11px var(--kiri-font-ui)",
        letterSpacing: "0.005em",
        whiteSpace: "pre",
        pointerEvents: "none",
        zIndex: 12,
        boxShadow: "0 8px 20px rgba(0,0,0,0.16)",
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
          // White outer circle with a black core remains visible over both
          // light and dark capture content.
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

function SizeBadge(props: { rect: Rect; bounds: Rect; pixelScale: number }) {
  const { rect, bounds } = props;
  const pixelScale =
    Number.isFinite(props.pixelScale) && props.pixelScale > 0 ? props.pixelScale : 1;
  // Selection geometry is expressed in display points, while the exact-size
  // controls and exported capture use physical pixels. Keep the badge in the
  // same unit so a 200 × 300 px request does not appear as 100 × 150 on a
  // Retina display.
  const pixelWidth = Math.round(rect.width * pixelScale);
  const pixelHeight = Math.round(rect.height * pixelScale);
  const label = `${pixelWidth} × ${pixelHeight}`;
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

function OcrPanel(props: {
  text: string;
  failed: boolean;
  anchor: Rect;
  bounds: Rect;
  onCopy(): void;
  onClose(): void;
}) {
  const { text, failed, anchor, bounds, onCopy, onClose } = props;
  // Bubble hugs the recognized region: below it, flipping above when the
  // bottom overflows, then pinned inside the screen. The little tail points
  // at the region (top-right when below, bottom-right when above).
  const PANEL_W = 368;
  const PANEL_H = 276;
  const margin = 8;
  const maxTop = Math.max(margin, bounds.height - PANEL_H - margin);
  const below = anchor.y + anchor.height + 10;
  const above = anchor.y - PANEL_H - 10;
  const belowFits = below + PANEL_H + margin <= bounds.height;
  const top = Math.min(Math.max(margin, belowFits ? below : above), maxTop);
  const centerX = anchor.x + anchor.width / 2 - PANEL_W / 2;
  const left = Math.min(
    Math.max(margin, centerX),
    Math.max(margin, bounds.width - PANEL_W - margin),
  );
  const tailAbove = !belowFits;
  // Tail tip x follows the region's center, clamped away from the rounded
  // corners so the bubble still reads cleanly near a display edge.
  const tipX = Math.min(
    Math.max(28, anchor.x + anchor.width / 2 - left),
    PANEL_W - 28,
  );
  return (
    <div
      className="kiri-hud"
      onPointerDown={(e) => e.stopPropagation()}
      style={{
        position: "absolute",
        left,
        top,
        width: PANEL_W,
        padding: 14,
        boxSizing: "border-box",
        borderRadius: 16,
        display: "flex",
        flexDirection: "column",
        gap: 10,
        boxShadow: "0 16px 42px rgba(0,0,0,0.22)",
      }}
    >
      {/* Tail pointing at the recognized region. */}
      <div
        style={{
          position: "absolute",
          [tailAbove ? "bottom" : "top"]: -5,
          left: tipX - 6,
          width: 12,
          height: 12,
          background: "rgba(8, 8, 8, 0.96)",
          transform: "rotate(45deg)",
          borderRadius: 2,
          zIndex: 0,
        }}
      />
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <span
            style={{
              width: 28,
              height: 28,
              display: "grid",
              placeItems: "center",
              borderRadius: 8,
              background: "#fff",
              color: "#000",
            }}
          >
            <KiriIcon name="text.viewfinder" size={15} />
          </span>
          <span style={{ font: "700 13px var(--kiri-font-ui)" }}>
            {failed ? t("Text Recognition Failed") : t("Recognized Text")}
          </span>
        </div>
        <button
          type="button"
          aria-label={t("Close")}
          title={t("Close")}
          onClick={onClose}
          style={{
            ...iconButtonStyle,
            width: 28,
            height: 28,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            border: "1px solid rgba(255,255,255,0.15)",
            background: "rgba(255,255,255,0.055)",
          }}
        >
          <KiriIcon name="xmark" size={11} />
        </button>
      </div>
      <div
        style={{
          background: "rgba(255,255,255,0.97)",
          color: "#0a0a0a",
          borderRadius: 11,
          padding: "12px 14px",
          minHeight: 96,
          maxHeight: 150,
          boxSizing: "border-box",
          overflow: "auto",
          font: "450 13px/1.48 var(--kiri-font-ui)",
          userSelect: "text",
          whiteSpace: "pre-wrap",
          border: "1px solid rgba(255,255,255,0.24)",
          boxShadow: "inset 0 0 0 1px rgba(0,0,0,0.08)",
        }}
      >
        {text || (failed ? t("Adjust the region and try again") : t("No Text Found"))}
      </div>
      <button
        type="button"
        className="kiri-primary-button"
        onClick={onCopy}
        disabled={!text}
        style={{
          minHeight: 38,
          borderRadius: 10,
          alignSelf: "flex-end",
          minWidth: 112,
          padding: "0 16px",
          display: "inline-flex",
          alignItems: "center",
          justifyContent: "center",
          gap: 7,
        }}
      >
        <KiriIcon name="doc.on.doc" size={13} />
        {t("Copy")}
      </button>
    </div>
  );
}

const sizeInputStyle: React.CSSProperties = {
  width: 58,
  height: 26,
  boxSizing: "border-box",
  borderRadius: 8,
  border: "1px solid rgba(255,255,255,0.18)",
  background: "rgba(255,255,255,0.08)",
  color: "#fff",
  fontSize: 11,
  textAlign: "center",
  outline: "none",
  fontFamily: "var(--kiri-font-ui)",
};

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
  const gifOutput = options.outputFormat === "gif";
  const toggle = (
    key:
      | "usesCountdown"
      | "capturesSystemAudio"
      | "capturesMicrophone"
      | "showsCursor"
      | "highlightsClicks",
  ) => {
    const next = { ...options, [key]: !options[key] };
    if (key === "showsCursor" && !next.showsCursor) next.highlightsClicks = false;
    onChange(next);
  };
  // Panel geometry follows the visible rows: GIF omits inapplicable audio
  // controls instead of leaving a tall block of disabled settings. Prefer
  // below the selection, flip above when needed, and pin inside the screen as
  // a last resort. Keep x centered on the selection with an 8pt margin.
  const PANEL_W = 360;
  const PANEL_H = gifOutput ? 314 : 382;
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
        padding: 14,
        width: PANEL_W,
        boxSizing: "border-box",
        display: "flex",
        flexDirection: "column",
        gap: 10,
        borderRadius: 16,
        boxShadow: "0 16px 42px rgba(0,0,0,0.22)",
      }}
    >
      <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
        <span
          style={{
            width: 26,
            height: 26,
            display: "grid",
            placeItems: "center",
            borderRadius: 8,
            background: "#fff",
            color: "#000",
          }}
        >
          <KiriIcon name="record.circle" size={14} />
        </span>
        <span style={{ font: "700 13px var(--kiri-font-ui)" }}>{t("Record Region")}</span>
      </div>
      <div
        role="radiogroup"
        aria-label={t("Recording format")}
        style={{
          display: "grid",
          gridTemplateColumns: "1fr 1fr",
          gap: 4,
          padding: 4,
          borderRadius: 12,
          background: "rgba(255,255,255,0.08)",
          border: "1px solid rgba(255,255,255,0.14)",
        }}
      >
        {(["mp4", "gif"] as const).map((format) => {
          const selected = options.outputFormat === format;
          return (
            <button
              key={format}
              type="button"
              role="radio"
              aria-checked={selected}
              onClick={() => onChange({ ...options, outputFormat: format })}
              style={{
                height: 32,
                border: "none",
                borderRadius: 9,
                background: selected ? "#fff" : "transparent",
                color: selected ? "#000" : "rgba(255,255,255,0.68)",
                font: "700 11.5px var(--kiri-font-ui)",
                cursor: "pointer",
                boxShadow: selected ? "0 2px 8px rgba(0,0,0,0.2)" : "none",
                transition: "background 0.16s ease-out, color 0.16s ease-out, box-shadow 0.16s ease-out",
              }}
            >
              {t(format === "mp4" ? "MP4" : "GIF")}
            </button>
          );
        })}
      </div>
      <div
        style={{
          color: "rgba(255,255,255,0.72)",
          font: "500 10.5px/1.4 var(--kiri-font-ui)",
          padding: "7px 9px",
          borderRadius: 9,
          background: "rgba(255,255,255,0.055)",
          border: "1px solid rgba(255,255,255,0.08)",
        }}
      >
        {gifOutput
          ? t("GIF · 12 fps · 720 px long edge · No audio")
          : t("MP4 · 30 fps · Saved locally · Never uploaded")}
      </div>
      <div
        style={{
          overflow: "hidden",
          borderRadius: 12,
          border: "1px solid rgba(255,255,255,0.11)",
          background: "rgba(255,255,255,0.035)",
        }}
      >
        <ToggleRow
          label={t("3-second countdown")}
          checked={options.usesCountdown}
          onToggle={() => toggle("usesCountdown")}
        />
        {!gifOutput && (
          <>
            <ToggleRow
              divider
              label={t("System audio")}
              checked={options.capturesSystemAudio}
              onToggle={() => toggle("capturesSystemAudio")}
            />
            <ToggleRow
              divider
              label={t("Microphone")}
              suffix={micSupported ? undefined : t("Requires macOS 15")}
              checked={options.capturesMicrophone}
              onToggle={() => toggle("capturesMicrophone")}
              disabled={!micSupported}
            />
          </>
        )}
        <ToggleRow
          divider
          label={t("Show pointer")}
          checked={options.showsCursor}
          onToggle={() => toggle("showsCursor")}
        />
        <ToggleRow
          divider
          label={t("Highlight clicks")}
          checked={options.highlightsClicks}
          onToggle={() => toggle("highlightsClicks")}
          disabled={!options.showsCursor}
        />
      </div>
      <div style={{ display: "flex", gap: 8 }}>
        <button className="kiri-primary-button" style={{ flex: 1, minHeight: 38, borderRadius: 10 }} onClick={onStart}>
          {gifOutput ? t("Start GIF Recording") : t("Start Recording")}
        </button>
        <button
          aria-label={t("Cancel")}
          title={t("Cancel")}
          style={{
            ...iconButtonStyle,
            width: 38,
            height: 38,
            flexShrink: 0,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            border: "1px solid rgba(255,255,255,0.18)",
            background: "rgba(255,255,255,0.06)",
            borderRadius: 10,
          }}
          onClick={onCancel}
        >
          <KiriIcon name="xmark" size={12} />
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
  divider?: boolean;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={props.checked}
      disabled={props.disabled}
      onClick={props.onToggle}
      style={{
        width: "100%",
        minHeight: 34,
        display: "flex",
        justifyContent: "space-between",
        alignItems: "center",
        padding: "5px 10px",
        border: "none",
        borderTop: props.divider ? "1px solid rgba(255,255,255,0.08)" : "none",
        background: "transparent",
        color: "#fff",
        textAlign: "left",
        opacity: props.disabled ? 0.4 : 1,
        cursor: props.disabled ? "default" : "pointer",
        transition: "background 0.14s ease-out, opacity 0.14s ease-out",
      }}
      onMouseEnter={(event) => {
        if (!props.disabled) event.currentTarget.style.background = "rgba(255,255,255,0.055)";
      }}
      onMouseLeave={(event) => {
        event.currentTarget.style.background = "transparent";
      }}
    >
      <span style={{ font: "550 12px var(--kiri-font-ui)" }}>
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
          background: props.checked ? "#fff" : "rgba(255,255,255,0.2)",
          position: "relative",
          flexShrink: 0,
          boxShadow: "inset 0 0 0 1px rgba(255,255,255,0.08)",
          transition: "background 0.16s ease-out",
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
            background: props.checked ? "#000" : "#fff",
            transition: "left 0.16s ease-out, background 0.16s ease-out",
          }}
        />
      </div>
    </button>
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
  canSetSize: boolean;
  disabled: boolean;
  onUndo(): void;
  onRedo(): void;
  onDone(): void;
  onCancel(): void;
  onTextFontBegin?(): void;
  onTextFontLive?(value: number): void;
  onTextFontEnd?(): void;
  /** Apply an exact pixel size to the selection (keeps its center). */
  onSetSize(widthPx: number, heightPx: number): void;
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
    canSetSize,
    disabled,
    onUndo,
    onRedo,
    onDone,
    onCancel,
    onTextFontBegin,
    onTextFontLive,
    onTextFontEnd,
    onSetSize,
  } = props;

  // Quick pixel-size entry: keep edits local until the user confirms them.
  // Values are output pixels; the selection is point-based, so onSetSize
  // converts them using the display scale.
  const [sizeW, setSizeW] = useState("");
  const [sizeH, setSizeH] = useState("");
  const applySize = () => {
    const w = Math.round(Number(sizeW));
    const h = Math.round(Number(sizeH));
    if (!Number.isFinite(w) || !Number.isFinite(h) || w <= 0 || h <= 0) return;
    onSetSize(w, h);
  };

  const onSizeInputKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter" || e.key === "Return") applySize();
    // Number-field input must not trigger the overlay's tool shortcuts or
    // finish the screenshot when Return is used to confirm the dimensions.
    e.stopPropagation();
  };

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
  }, [tool, appearance]);
  // Spec §7.6: centered on the selection's midX; clamp so the bar's left
  // and right edges stay ≥8pt inside the screen. y is already resolved.
  const left = Math.min(
    Math.max(8, anchor.x - barWidth / 2),
    Math.max(8, bounds.x + bounds.width - barWidth - 8),
  );
  // Mode selector sits at the top-center (16 + ~44pt tall). Keep the
  // toolbar BELOW that zone so the two HUDs never overlap: minimum top is
  // 70 (mode selector zone), maximum keeps the bar inside the screen.
  const top = Math.min(
    Math.max(96, anchor.y),
    Math.max(96, bounds.y + bounds.height - toolbarHeight - 8),
  );

  const sep = <div style={{ width: 1, height: 26, background: "rgba(255,255,255,0.14)", margin: "0 3px", flexShrink: 0 }} />;

  return (
    <>
      <div
        ref={barRef}
        className="kiri-hud"
        aria-disabled={disabled}
        onPointerDown={(e) => e.stopPropagation()}
        style={{
          position: "absolute",
          left,
          top,
          display: "flex",
          alignItems: "center",
          gap: 3,
          padding: "6px 8px",
          boxShadow: "none",
          opacity: disabled ? 0.62 : 1,
          transition: "opacity 0.12s ease-out",
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
        {sep}
        {canSetSize && (
          /* Quick pixel-size entry — confirm before resizing the selection. */
          <div
            style={{
              display: "flex",
              alignItems: "center",
              gap: 3,
              padding: "2px 4px",
            }}
          >
            <input
              type="number"
              min={1}
              value={sizeW}
              onChange={(e) => setSizeW(e.target.value)}
              onKeyDown={onSizeInputKeyDown}
              placeholder={t("Width (px)").charAt(0)}
              title={t("Width (px)")}
              style={sizeInputStyle}
            />
            <span style={{ color: "rgba(255,255,255,0.55)", fontSize: 11 }}>×</span>
            <input
              type="number"
              min={1}
              value={sizeH}
              onChange={(e) => setSizeH(e.target.value)}
              onKeyDown={onSizeInputKeyDown}
              placeholder={t("Height (px)").charAt(0)}
              title={t("Height (px)")}
              style={sizeInputStyle}
            />
            <ToolButton icon="checkmark" title={t("Apply size")} onClick={applySize} />
          </div>
        )}
        <ToolButton icon="checkmark" title={t("Done — Copy to clipboard · Return")} primary onClick={onDone} />
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
        border: props.primary || props.active
          ? "1px solid #fff"
          : "1px solid transparent",
        background: props.primary || props.active ? "#fff" : "transparent",
        color: props.primary || props.active ? "#000" : "#fff",
        fontSize: 12,
        fontWeight: 600,
        cursor: props.disabled ? "default" : "pointer",
        opacity: props.disabled ? 0.35 : 1,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        boxShadow: "none",
        transition: "transform 0.14s ease-out, background 0.14s ease-out",
      }}
      onMouseEnter={(e) => {
        if (props.primary && !props.disabled) {
          e.currentTarget.style.opacity = "0.82";
        } else if (!props.active && !props.disabled) {
          e.currentTarget.style.background = "rgba(255,255,255,0.10)";
        }
      }}
      onMouseLeave={(e) => {
        if (props.primary) {
          e.currentTarget.style.opacity = "1";
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
            background: props.value === index ? "#fff" : "transparent",
            color: props.value === index ? "#000" : "#fff",
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
