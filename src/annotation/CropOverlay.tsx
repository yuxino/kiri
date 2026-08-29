import { useRef } from "react";
import type { PointerEvent as ReactPointerEvent } from "react";
import type { Rect } from "./geom";
import {
  ALL_HANDLES,
  clampPoint,
  contains,
  handlePoint,
  hitTestHandle,
  maxX,
  maxY,
  minX,
  minY,
  normalized,
  resized,
} from "./geom";
import type { AnnotationDocumentV1 } from "./model";
import { cropPixelsFromDocumentRect, MIN_CROP_SOURCE_PIXELS } from "./crop.js";
import { t } from "../i18n";

type ResizeHandle = Parameters<typeof resized>[1];
type Drag =
  | { kind: "none" }
  | { kind: "resize"; handle: ResizeHandle; original: Rect; latest: Rect }
  | { kind: "move"; start: { x: number; y: number }; original: Rect; latest: Rect }
  | { kind: "create"; start: { x: number; y: number }; original: Rect; latest: Rect };

export function CropOverlay(props: {
  document: AnnotationDocumentV1;
  viewSize: { width: number; height: number };
  selection: Rect;
  active: boolean;
  onChange(selection: Rect): void;
  onCommit(previous: Rect, next: Rect): void;
}) {
  const { document, viewSize, selection, active, onChange, onCommit } = props;
  const dragRef = useRef<Drag>({ kind: "none" });
  const scaleX = viewSize.width / document.canvas.width;
  const scaleY = viewSize.height / document.canvas.height;
  const viewRect = {
    x: selection.x * scaleX,
    y: selection.y * scaleY,
    width: selection.width * scaleX,
    height: selection.height * scaleY,
  };
  const bounds = { x: 0, y: 0, width: document.canvas.width, height: document.canvas.height };
  const minimumX = MIN_CROP_SOURCE_PIXELS * document.canvas.width / document.sourcePixels.width;
  const minimumY = MIN_CROP_SOURCE_PIXELS * document.canvas.height / document.sourcePixels.height;
  const pixels = cropPixelsFromDocumentRect(document, selection);

  const point = (event: ReactPointerEvent<HTMLDivElement>) => {
    const rect = event.currentTarget.getBoundingClientRect();
    return clampPoint({
      x: ((event.clientX - rect.left) * document.canvas.width) / Math.max(1, rect.width),
      y: ((event.clientY - rect.top) * document.canvas.height) / Math.max(1, rect.height),
    }, bounds);
  };

  const onPointerDown = (event: ReactPointerEvent<HTMLDivElement>) => {
    if (!active || event.button !== 0) return;
    const p = point(event);
    const handle = hitTestHandle(
      p,
      selection,
      9 * Math.max(document.canvas.width / viewSize.width, document.canvas.height / viewSize.height),
    );
    if (handle) {
      dragRef.current = { kind: "resize", handle, original: selection, latest: selection };
    } else if (contains(selection, p)) {
      dragRef.current = { kind: "move", start: p, original: selection, latest: selection };
    } else {
      dragRef.current = { kind: "create", start: p, original: selection, latest: selection };
    }
    event.currentTarget.setPointerCapture(event.pointerId);
    event.preventDefault();
  };

  const onPointerMove = (event: ReactPointerEvent<HTMLDivElement>) => {
    const drag = dragRef.current;
    if (!active || drag.kind === "none") return;
    const p = point(event);
    if (drag.kind === "resize") {
      const next = resized(selection, drag.handle, p, bounds, Math.max(minimumX, minimumY));
      if (next.width >= minimumX && next.height >= minimumY) {
        drag.latest = next;
        onChange(next);
      }
    } else if (drag.kind === "move") {
      const dx = Math.min(
        Math.max(p.x - drag.start.x, -minX(drag.original)),
        maxX(bounds) - maxX(drag.original),
      );
      const dy = Math.min(
        Math.max(p.y - drag.start.y, -minY(drag.original)),
        maxY(bounds) - maxY(drag.original),
      );
      const next = { ...drag.original, x: drag.original.x + dx, y: drag.original.y + dy };
      drag.latest = next;
      onChange(next);
    } else {
      const next = normalized(drag.start, p);
      if (next.width >= minimumX && next.height >= minimumY) {
        drag.latest = next;
        onChange(next);
      }
    }
  };

  const finish = (event: ReactPointerEvent<HTMLDivElement>) => {
    const drag = dragRef.current;
    if (drag.kind !== "none") onCommit(drag.original, drag.latest);
    dragRef.current = { kind: "none" };
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
  };

  return (
    <div
      aria-label={t("Crop area")}
      style={{
        position: "absolute",
        inset: 0,
        overflow: "hidden",
        pointerEvents: active ? "auto" : "none",
        cursor: active ? "crosshair" : "default",
        touchAction: "none",
      }}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={finish}
      onPointerCancel={finish}
    >
      <div
        style={{
          position: "absolute",
          left: viewRect.x,
          top: viewRect.y,
          width: viewRect.width,
          height: viewRect.height,
          boxSizing: "border-box",
          border: `1px solid rgba(255,255,255,${active ? 0.96 : 0.7})`,
          boxShadow: `0 0 0 9999px rgba(0,0,0,${active ? 0.5 : 0.25})`,
        }}
      >
        {active && ALL_HANDLES.map((handle) => {
          const p = handlePoint(handle, viewRect);
          return (
            <span
              key={handle}
              style={{
                position: "absolute",
                left: p.x - viewRect.x - 4,
                top: p.y - viewRect.y - 4,
                width: 8,
                height: 8,
                boxSizing: "border-box",
                border: "1px solid rgba(0,0,0,0.72)",
                background: "#fff",
              }}
            />
          );
        })}
        {active && (
          <span
            style={{
              position: "absolute",
              left: "50%",
              bottom: 7,
              transform: "translateX(-50%)",
              padding: "2px 5px",
              borderRadius: 5,
              background: "rgba(0,0,0,0.72)",
              color: "#fff",
              font: "500 9px/12px var(--kiri-font-ui)",
              fontVariantNumeric: "tabular-nums",
              whiteSpace: "nowrap",
            }}
          >
            {pixels.width} × {pixels.height}
          </span>
        )}
      </div>
    </div>
  );
}
