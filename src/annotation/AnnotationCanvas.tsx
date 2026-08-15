// AnnotationCanvas — interactive canvas port of AnnotationCanvasView.swift.
// The parent owns tool/appearance; this component owns history, selection,
// drafts, inline text editing, and export.

import React, {
  forwardRef,
  useCallback,
  useEffect,
  useImperativeHandle,
  useMemo,
  useRef,
  useState,
} from "react";
import type { Point, Rect } from "./geom";
import type { ColorPreset } from "./model";
import { clampPoint, hitTestHandle } from "./geom";
import {
  AnnotationHistory,
  markIndexAt,
  moveEndpointMark,
  resizeRectangleMark,
  translateMark,
  type AnnotationMark,
  type AppearanceSettings,
  type TextBackgroundStyle,
  type Tool,
} from "./model";
import { renderAll, textFont, type RenderContext } from "./render";

export interface AnnotationCanvasHandle {
  undo(): void;
  redo(): void;
  clearAnnotations(): void;
  deleteSelection(): void;
  commitTextEditing(): void;
  exportPng(): Promise<Uint8Array | null>;
  /**
   * Live text font-size adjustment (spec §6.6): begin records the selected
   * text mark, set applies a preview (no history), end commits one history
   * entry. The slider value itself lives in the parent's AppearanceSettings.
   */
  beginTextFontSizeAdjustment(): void;
  setTextFontSizeLive(value: number): void;
  endTextFontSizeAdjustment(): void;
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
  tool: Tool;
  appearance: AppearanceSettings;
  onHistoryChange(canUndo: boolean, canRedo: boolean): void;
  onCancel(): void;
  /**
   * Called after a text annotation is committed via Return (spec §6.6:
   * "Return commits the text and completes the capture"). The parent
   * overlay finishes the screenshot; the editor leaves this unset.
   */
  onFinishAfterTextCommit?(): void;
}

interface EditingState {
  index: number | null;
  text: string;
  rect: Rect;
  color: ColorPreset;
  background: TextBackgroundStyle;
  fontSize: number;
}

type Interaction =
  | { kind: "none" }
  | { kind: "draw"; tool: Tool; start: Point; points: Point[] }
  | { kind: "move"; index: number; original: AnnotationMark; start: Point }
  | { kind: "resize"; index: number; original: AnnotationMark; handle: string }
  | { kind: "endpoint"; index: number; original: AnnotationMark; isStart: boolean };

const AnnotationCanvas = forwardRef<AnnotationCanvasHandle, Props>(
  function AnnotationCanvas(
    {
      image,
      region,
      displaySize,
      tool,
      appearance,
      onHistoryChange,
      onCancel,
      onFinishAfterTextCommit,
    },
    ref,
  ) {
    const canvasRef = useRef<HTMLCanvasElement>(null);
    const [marks, setMarks] = useState<AnnotationMark[]>([]);
    const [selectedIndex, setSelectedIndex] = useState<number | null>(null);
    const [draft, setDraft] = useState<AnnotationMark | null>(null);
    const [brushCursor, setBrushCursor] = useState<Point | null>(null);
    const [selectCursor, setSelectCursor] = useState<string>("default");
    const [editing, setEditing] = useState<EditingState | null>(null);
    const historyRef = useRef(new AnnotationHistory());
    const interactionRef = useRef<Interaction>({ kind: "none" });
    const appearanceRef = useRef(appearance);
    appearanceRef.current = appearance;
    const toolRef = useRef(tool);
    toolRef.current = tool;
    const imageRef = useRef<HTMLImageElement | null>(null);
    imageRef.current = image;
    const editingRef = useRef<EditingState | null>(null);
    const brushCursorRef = useRef<Point | null>(null);
    useEffect(() => {
      editingRef.current = editing;
      brushCursorRef.current = brushCursor;
    }, [editing, brushCursor]);

    // Convert the HTMLImageElement to a canvas for pixel access. Built lazily
    // once the image is fully decoded (drawImage on a loading image yields a
    // blank canvas that would corrupt the export).
    const sourceCanvasRef = useRef<HTMLCanvasElement | null>(null);
    const ensureSourceCanvas = useCallback((): HTMLCanvasElement | null => {
      const img = imageRef.current;
      if (!img || !img.complete || img.naturalWidth === 0) return null;
      const cached = sourceCanvasRef.current;
      if (cached && cached.width === img.naturalWidth && cached.height === img.naturalHeight) {
        return cached;
      }
      const c = document.createElement("canvas");
      c.width = img.naturalWidth;
      c.height = img.naturalHeight;
      c.getContext("2d")!.drawImage(img, 0, 0);
      sourceCanvasRef.current = c;
      return c;
    }, []);

    const view = useMemo(() => {
      return { width: region.width, height: region.height };
    }, [region]);

    const publishHistory = useCallback(() => {
      onHistoryChange(historyRef.current.canUndo, historyRef.current.canRedo);
    }, [onHistoryChange]);

    const syncMarks = useCallback(() => {
      setMarks(historyRef.current.elements.slice());
      publishHistory();
    }, [publishHistory]);

    const redraw = useCallback(() => {
      const canvas = canvasRef.current;
      if (!canvas) return;
      const ctx = canvas.getContext("2d");
      if (!ctx) return;
      const sourceCanvas = ensureSourceCanvas();
      if (!sourceCanvas) return;
      ctx.setTransform(devicePixelRatio, 0, 0, devicePixelRatio, 0, 0);
      const context: RenderContext = {
        ctx,
        sourceImage: sourceCanvas,
        sourceOffset: { x: region.x, y: region.y },
        regionSize: { x: 0, y: 0, width: region.width, height: region.height },
        scaleX: sourceCanvas.width / (displaySize?.width ?? region.width),
        scaleY: sourceCanvas.height / (displaySize?.height ?? region.height),
        exporting: false,
      };
      renderAll(context, marks, {
        draft,
        brushCursor,
        brushDiameter: appearanceRef.current.mosaicBrushDiameter,
        selectedIndex: editing ? null : selectedIndex,
        editingIndex: editing ? editing.index : null,
      });
    }, [marks, draft, brushCursor, selectedIndex, editing, region, ensureSourceCanvas]);

    useEffect(() => {
      redraw();
    }, [redraw]);

    const toPoint = useCallback((e: React.PointerEvent | MouseEvent): Point => {
      const canvas = canvasRef.current!;
      const rect = canvas.getBoundingClientRect();
      return { x: e.clientX - rect.left, y: e.clientY - rect.top };
    }, []);

    const commitText = useCallback(() => {
      setEditing((current) => {
        if (!current) return null;
        const text = current.text.trim();
        const frame = current.rect;
        const textRect: Rect = {
          x: frame.x + 8,
          y: frame.y + 5,
          width: Math.max(1, frame.width - 16),
          height: Math.max(1, frame.height - 10),
        };
        if (!text) {
          if (current.index !== null) {
            historyRef.current.remove(current.index);
            setSelectedIndex(null);
            syncMarks();
          }
          return null;
        }
        const previous =
          current.index !== null ? historyRef.current.elements[current.index] : null;
        const newMark: AnnotationMark = {
          // Reuse the previous id so an unchanged edit compares equal and
          // does not create a no-op history entry (spec §6.6).
          id: previous && previous.kind === "text" ? previous.id : Date.now() + Math.random(),
          kind: "text",
          text,
          rect: textRect,
          color: current.color,
          background: current.background,
          fontSize: current.fontSize,
        };
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
                previous.fontSize === newMark.fontSize
              : false;
          if (!unchanged) {
            historyRef.current.replace(current.index, newMark);
            syncMarks();
          }
          setSelectedIndex(current.index);
        } else {
          historyRef.current.append(newMark);
          syncMarks();
        }
        return null;
      });
      // Spec §6.6: commit (unchanged edits do not write history). The
      // Return key additionally finishes the capture — handled in the
      // TextEditor's Enter branch so other commit triggers (tool switch,
      // undo, export) do not complete the capture.
    }, [syncMarks]);

    const onPointerDown = useCallback(
      (e: React.PointerEvent) => {
        const canvas = canvasRef.current!;
        canvas.setPointerCapture(e.pointerId);
        const p = clampPoint(toPoint(e), { x: 0, y: 0, width: region.width, height: region.height });
        const t = toolRef.current;
        const ap = appearanceRef.current;

        if (editingRef.current) {
          commitText();
        }

        if (t === "text") {
          const width = Math.min(180, Math.max(96, region.width));
          const frame: Rect = {
            x: Math.min(p.x, region.width - width),
            y: Math.min(p.y, region.height - 34),
            width,
            height: 34,
          };
          setEditing({
            index: selectedIndex,
            text: "",
            rect: frame,
            color: ap.colorPreset,
            background: ap.textBackgroundStyle,
            fontSize: ap.textFontSize,
          });
          setSelectedIndex(null);
          return;
        }

        if (t === "select") {
          const current = historyRef.current.elements;
          let handleInteraction: string | null = null;
          if (selectedIndex !== null && current[selectedIndex]?.kind === "rectangle") {
            handleInteraction = hitTestHandle(
              p,
              (current[selectedIndex] as { rect: Rect }).rect,
              9,
            );
          } else if (selectedIndex !== null) {
            const mark = current[selectedIndex];
            if (mark && (mark.kind === "line" || mark.kind === "arrow")) {
              if (Math.hypot(p.x - mark.start.x, p.y - mark.start.y) <= 10) {
                handleInteraction = "start";
              } else if (Math.hypot(p.x - mark.end.x, p.y - mark.end.y) <= 10) {
                handleInteraction = "end";
              }
            }
          }
          const index = handleInteraction ? selectedIndex : markIndexAt(current, p);
          if (index === null || index === undefined) {
            setSelectedIndex(null);
            interactionRef.current = { kind: "none" };
            redraw();
            return;
          }
          const mark = current[index];
          if (mark.kind === "text" && e.detail >= 2) {
            setEditing({
              index,
              text: mark.text,
              rect: {
                x: mark.rect.x - 8,
                y: mark.rect.y - 5,
                width: mark.rect.width + 16,
                height: mark.rect.height + 10,
              },
              color: mark.color,
              background: mark.background,
              fontSize: mark.fontSize,
            });
            setSelectedIndex(index);
            return;
          }
          setSelectedIndex(index);
          if (handleInteraction === "start" || handleInteraction === "end") {
            interactionRef.current = {
              kind: "endpoint",
              index,
              original: mark,
              isStart: handleInteraction === "start",
            };
          } else if (handleInteraction) {
            interactionRef.current = { kind: "resize", index, original: mark, handle: handleInteraction };
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
        if (t === "pen") {
          setDraft({ kind: "pen", id: -1, points, color: ap.colorPreset, width: ap.penWidth });
        } else if (t === "mosaic") {
          setDraft({
            kind: "mosaic",
            id: -1,
            points,
            brushDiameter: ap.mosaicBrushDiameter,
            intensity: ap.mosaicIntensity,
          });
        } else if (t === "rectangle") {
          setDraft({
            kind: "rectangle",
            id: -1,
            rect: { x: p.x, y: p.y, width: 0, height: 0 },
            color: ap.colorPreset,
            width: ap.shapeWidth,
          });
        } else {
          setDraft({ kind: "line", id: -1, start: p, end: p, color: ap.colorPreset, width: ap.shapeWidth });
        }
      },
      [toPoint, region, selectedIndex, redraw, commitText],
    );

    const onPointerMove = useCallback(
      (e: React.PointerEvent) => {
        const p = clampPoint(toPoint(e), { x: 0, y: 0, width: region.width, height: region.height });
        const interaction = interactionRef.current;
        if (interaction.kind === "none") {
          if (toolRef.current === "mosaic") setBrushCursor(p);
          else if (brushCursorRef.current) setBrushCursor(null);
          if (toolRef.current === "select") {
            // Spec §6.7: handle → crosshair, over a mark → open hand,
            // otherwise arrow.
            const current = historyRef.current.elements;
            const selected = selectedIndexRef.current;
            let cursor = "default";
            if (selected !== null && current[selected]?.kind === "rectangle") {
              if (hitTestHandle(p, (current[selected] as { rect: Rect }).rect, 9)) {
                cursor = "crosshair";
              }
            }
            if (cursor === "default" && markIndexAt(current, p) !== null) {
              cursor = "grab";
            }
            setSelectCursor(cursor);
          }
          return;
        }
        if (interaction.kind === "draw") {
          const t = interaction.tool;
          // Spec §7.4: the brush cursor tracks the drag point while drawing.
          if (t === "mosaic") setBrushCursor(p);
          if (t === "pen" || t === "mosaic") {
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
              setDraft({
                kind: "mosaic",
                id: -1,
                points: [...points],
                brushDiameter: appearanceRef.current.mosaicBrushDiameter,
                intensity: appearanceRef.current.mosaicIntensity,
              });
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
                kind: "line",
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
          const by = { x: p.x - interaction.start.x, y: p.y - interaction.start.y };
          setDraft(
            translateMark(interaction.original, by, {
              x: 0,
              y: 0,
              width: region.width,
              height: region.height,
            }),
          );
          return;
        }
        if (interaction.kind === "resize") {
          setDraft(
            resizeRectangleMark(interaction.original, interaction.handle, p, {
              x: 0,
              y: 0,
              width: region.width,
              height: region.height,
            }),
          );
          return;
        }
        if (interaction.kind === "endpoint") {
          setDraft(moveEndpointMark(interaction.original, interaction.isStart, p));
        }
      },
      [toPoint, region],
    );

    const onPointerUp = useCallback(
      (e: React.PointerEvent) => {
        const p = clampPoint(toPoint(e), { x: 0, y: 0, width: region.width, height: region.height });
        const interaction = interactionRef.current;
        interactionRef.current = { kind: "none" };
        if (interaction.kind === "move") setSelectCursor("default");

        if (interaction.kind === "draw") {
          const t = interaction.tool;
          const ap = appearanceRef.current;
          if (t === "pen") {
            const points = interaction.points;
            const last = points[points.length - 1];
            if (Math.hypot(p.x - last.x, p.y - last.y) >= 0.5) points.push(p);
            if (points.length > 1) {
              historyRef.current.append({
                kind: "pen",
                id: Date.now() + Math.random(),
                points: [...points],
                color: ap.colorPreset,
                width: ap.penWidth,
              });
              syncMarks();
            }
          } else if (t === "mosaic") {
            const points = interaction.points;
            const last = points[points.length - 1];
            if (Math.hypot(p.x - last.x, p.y - last.y) >= 0.5) points.push(p);
            historyRef.current.append({
              kind: "mosaic",
              id: Date.now() + Math.random(),
              points: [...points],
              brushDiameter: ap.mosaicBrushDiameter,
              intensity: ap.mosaicIntensity,
            });
            syncMarks();
          } else if (t === "rectangle") {
            const start = interaction.start;
            historyRef.current.append({
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
            syncMarks();
          } else {
            const start = interaction.start;
            if (Math.hypot(p.x - start.x, p.y - start.y) >= 3) {
              historyRef.current.append({
                kind: t === "arrow" ? "arrow" : "line",
                id: Date.now() + Math.random(),
                start,
                end: p,
                color: ap.colorPreset,
                width: ap.shapeWidth,
              });
              syncMarks();
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
          const preview = draft;
          // Spec §6.3: only commit a drag when it actually changed the
          // mark (≥1pt of movement) — a click without movement must not
          // write a no-op history entry.
          const changed =
            interaction.kind === "move"
              ? Math.hypot(p.x - interaction.start.x, p.y - interaction.start.y) >= 1
              : preview !== null &&
                JSON.stringify(preview) !== JSON.stringify(interaction.original);
          if (preview && changed) {
            historyRef.current.replace(interaction.index, preview);
            syncMarks();
          }
          setDraft(null);
        }
        redraw();
      },
      [toPoint, region, draft, redraw, syncMarks],
    );

    // Keyboard shortcuts (overlay-level keys are handled by the parent).
    useEffect(() => {
      const onKeyDown = (e: KeyboardEvent) => {
        const canvasWindow = window as unknown as { __kiriOverlay?: boolean };
        if (!canvasWindow.__kiriOverlay) return;
        if (editingRef.current) return;
        if (e.key === "Delete" || e.key === "Backspace") {
          if (toolRef.current === "select" && selectedIndexRef.current !== null) {
            historyRef.current.remove(selectedIndexRef.current);
            setSelectedIndex(null);
            syncMarks();
          }
        }
      };
      window.addEventListener("keydown", onKeyDown);
      return () => window.removeEventListener("keydown", onKeyDown);
    }, [syncMarks]);

    const selectedIndexRef = useRef<number | null>(null);
    useEffect(() => {
      selectedIndexRef.current = selectedIndex;
    }, [selectedIndex]);

    const undoRef = useRef<() => void>(() => {});
    const redoRef = useRef<() => void>(() => {});
    const deleteRef = useRef<() => void>(() => {});
    undoRef.current = () => {
      commitText();
      historyRef.current.undo();
      setSelectedIndex(null);
      syncMarks();
    };
    redoRef.current = () => {
      commitText();
      historyRef.current.redo();
      setSelectedIndex(null);
      syncMarks();
    };
    deleteRef.current = () => {
      if (toolRef.current !== "select" || selectedIndexRef.current === null) return;
      historyRef.current.remove(selectedIndexRef.current);
      setSelectedIndex(null);
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
      commitText();
      const index = selectedIndexRef.current;
      const mark = index !== null ? historyRef.current.elements[index] : undefined;
      if (index !== null && mark && mark.kind === "text") {
        fontAdjustRef.current = { index, original: mark };
      } else {
        fontAdjustRef.current = null;
      }
    };
    setFontLiveRef.current = (value: number) => {
      const adjust = fontAdjustRef.current;
      if (!adjust) return;
      const mark = historyRef.current.elements[adjust.index];
      if (!mark || mark.kind !== "text") return;
      const updated: AnnotationMark = { ...mark, fontSize: value };
      // Preview: swap the element without recording history.
      const before = historyRef.current.elements.slice();
      before[adjust.index] = updated;
      historyRef.current.overwrite(before);
      syncMarks();
    };
    endFontAdjustRef.current = () => {
      const adjust = fontAdjustRef.current;
      fontAdjustRef.current = null;
      if (!adjust) return;
      const mark = historyRef.current.elements[adjust.index];
      if (mark && mark.kind === "text" && mark.fontSize !== adjust.original.fontSize) {
        historyRef.current.replace(adjust.index, mark);
        syncMarks();
      }
    };

    useImperativeHandle(
      ref,
      () => ({
        undo: () => undoRef.current(),
        redo: () => redoRef.current(),
        clearAnnotations: () => {
          if (!historyRef.current.canUndo && !historyRef.current.canRedo) return;
          // Spec §10.1: clear also discards an in-flight text edit.
          setEditing(null);
          historyRef.current.clear();
          setSelectedIndex(null);
          syncMarks();
        },
        deleteSelection: () => deleteRef.current(),
        commitTextEditing: () => commitText(),
        exportPng: async () => {
          commitText();
          const img = imageRef.current;
          if (!img) return null;
          if (!img.complete) {
            try {
              await img.decode();
            } catch {
              return null;
            }
          }
          const sourceCanvas = ensureSourceCanvas();
          if (!sourceCanvas) return null;
          const out = document.createElement("canvas");
          const scaleX = sourceCanvas.width / (displaySize?.width ?? region.width);
          const scaleY = sourceCanvas.height / (displaySize?.height ?? region.height);
          out.width = Math.round(region.width * scaleX);
          out.height = Math.round(region.height * scaleY);
          const ctx = out.getContext("2d")!;
          const context: RenderContext = {
            ctx,
            sourceImage: sourceCanvas,
            sourceOffset: { x: region.x, y: region.y },
            regionSize: { x: 0, y: 0, width: region.width, height: region.height },
            scaleX,
            scaleY,
            exporting: true,
          };
          renderAll(context, historyRef.current.elements, {});
          const blob = await new Promise<Blob | null>((resolve) =>
            out.toBlob(resolve, "image/png"),
          );
          if (!blob) return null;
          return new Uint8Array(await blob.arrayBuffer());
        },
        beginTextFontSizeAdjustment: () => beginFontAdjustRef.current(),
        setTextFontSizeLive: (value: number) => setFontLiveRef.current(value),
        endTextFontSizeAdjustment: () => endFontAdjustRef.current(),
      }),
      [commitText, region, displaySize, ensureSourceCanvas, syncMarks],
    );

    return (
      <div
        className="annotation-canvas-root"
        style={{ position: "relative", width: "100%", height: "100%" }}
      >
        <canvas
          ref={canvasRef}
          width={Math.round(view.width * devicePixelRatio)}
          height={Math.round(view.height * devicePixelRatio)}
          style={{
            width: view.width,
            height: view.height,
            // Spec §6.7: mosaic uses a crosshair, select tracks hover
            // (arrow/hand/crosshair), all other tools use the arrow.
            cursor:
              tool === "mosaic"
                ? "crosshair"
                : tool === "select"
                  ? selectCursor
                  : "default",
          }}
          onPointerDown={onPointerDown}
          onPointerMove={onPointerMove}
          onPointerUp={onPointerUp}
        />
        {editing && (
          <TextEditor
            editing={editing}
            onTextChange={(text) => setEditing((prev) => (prev ? { ...prev, text } : prev))}
            onCommit={commitText}
            onFinish={onFinishAfterTextCommit}
            onUndo={() => undoRef.current()}
            onRedo={() => redoRef.current()}
            onCancel={onCancel}
          />
        )}
      </div>
    );
  },
);

function TextEditor(props: {
  editing: EditingState;
  onTextChange(text: string): void;
  onCommit(): void;
  onFinish?(): void;
  onUndo(): void;
  onRedo(): void;
  onCancel(): void;
}) {
  const { editing, onTextChange, onCommit, onFinish, onUndo, onRedo, onCancel } = props;
  const ref = useRef<HTMLTextAreaElement>(null);
  const [size, setSize] = useState<{ width: number; height: number }>({
    width: editing.rect.width,
    height: editing.rect.height,
  });

  // Spec §6.6 resizeTextEditor: min 120×34, grows with text/font, clamped
  // to the right/bottom edges of the region.
  const focusedOnce = useRef(false);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const font = textFont(editing.fontSize);
    const canvas = document.createElement("canvas");
    const ctx = canvas.getContext("2d")!;
    ctx.font = font;
    const text = editing.text || "Type something…";
    const measured = ctx.measureText(text);
    const lines = text.split("\n").length;
    const lineHeight = editing.fontSize * 1.25;
    const width = Math.max(120, Math.ceil(measured.width) + 16 + 2);
    const height = Math.max(34, Math.ceil(lines * lineHeight) + 10 + 2);
    setSize((current) =>
      current.width === width && current.height === height ? current : { width, height },
    );
    // Focus + select only on first mount; re-running el.select() on every
    // keystroke would yank the caret to the end and break mid-text edits.
    if (!focusedOnce.current) {
      focusedOnce.current = true;
      el.focus();
      el.select();
    }
  }, [editing.text, editing.fontSize]);

  return (
    <textarea
      ref={ref}
      value={editing.text}
      placeholder="Type something…"
      spellCheck={false}
      autoCorrect="off"
      autoCapitalize="off"
      onChange={(e) => onTextChange(e.target.value)}
      onKeyDown={(e) => {
        if (e.key === "Escape") {
          e.preventDefault();
          onCancel();
        } else if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "z") {
          // Spec §10.1: undo/redo commit the text edit first, then act on
          // the canvas history (never the textarea's native undo).
          e.preventDefault();
          onCommit();
          if (e.shiftKey) onRedo();
          else onUndo();
        } else if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
          e.preventDefault();
          onCommit();
          // Spec §6.6: Return commits the text and completes the capture.
          onFinish?.();
        }
        e.stopPropagation();
      }}
      style={{
        position: "absolute",
        left: editing.rect.x,
        top: editing.rect.y,
        width: size.width,
        height: size.height,
        boxSizing: "border-box",
        padding: "5px 8px",
        font: textFont(editing.fontSize),
        color: editing.color,
        background:
          editing.background === "dark"
            ? "rgba(0,0,0,0.72)"
            : editing.background === "light"
              ? "rgba(255,255,255,0.9)"
              : "transparent",
        border: `1px solid ${editing.color}cc`,
        borderRadius: 7,
        resize: "none",
        outline: "none",
        overflow: "hidden",
        whiteSpace: "pre-wrap",
        wordBreak: "break-word",
        lineHeight: 1.25,
      }}
    />
  );
}

export default AnnotationCanvas;
