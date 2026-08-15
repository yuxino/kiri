// EditorWindow — annotation editor for saved captures
// (EditorWindowController.swift): 880×620, dark, 58pt toolbar, same canvas.

import { useEffect, useMemo, useRef, useState } from "react";
import { api } from "../lib/ipc";
import { t } from "../i18n";
import type { Rect } from "../annotation/geom";
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
import { KiriIcon, type IconName } from "../components/KiriIcons";

const TOOLS: { tool: Tool; icon: IconName; title: string }[] = [
  { tool: "select", icon: "cursorarrow", title: "Select (V)" },
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
  const [tool, setTool] = useState<Tool>("select");
  const [appearance, setAppearance] = useState<AppearanceSettings>(DEFAULT_APPEARANCE);
  const [canUndo, setCanUndo] = useState(false);
  const [canRedo, setCanRedo] = useState(false);
  const canvasRef = useRef<AnnotationCanvasHandle>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    // Blob URL keeps the canvas CORS-clean for export.
    fetch(`kiri://asset/${props.id}`)
      .then((response) => response.blob())
      .then((blob) => {
        const src = URL.createObjectURL(blob);
        const img = new Image();
        img.src = src;
        img.onload = () => {
          setImage(img);
          setImageSize({ w: img.naturalWidth, h: img.naturalHeight });
        };
      })
      .catch(() => {});
  }, [props.id]);

  // Aspect-fit rect for the image within the container.
  const region = useMemo<Rect>(() => {
    if (!imageSize) return { x: 0, y: 0, width: 1, height: 1 };
    const container = containerRef.current;
    const width = container?.clientWidth ?? 800;
    const height = container?.clientHeight ?? 560;
    const scale = Math.min(width / imageSize.w, height / imageSize.h);
    return { x: 0, y: 0, width: imageSize.w * scale, height: imageSize.h * scale };
  }, [imageSize]);

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        void closeWindow();
        return;
      }
      if (e.key === "Enter" && !e.isComposing) {
        void complete(true);
        return;
      }
      const mod = e.metaKey || e.ctrlKey;
      if (mod && e.key.toLowerCase() === "z") {
        e.preventDefault();
        if (e.shiftKey) canvasRef.current?.redo();
        else canvasRef.current?.undo();
        return;
      }
      if (e.key === "Delete" || e.key === "Backspace") {
        canvasRef.current?.deleteSelection();
        return;
      }
      if (!mod && !e.altKey) {
        const keyMap: Record<string, Tool> = {
          v: "select",
          p: "pen",
          r: "rectangle",
          l: "line",
          a: "arrow",
          t: "text",
          m: "mosaic",
        };
        const next = keyMap[e.key.toLowerCase()];
        if (next) setTool(next);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  function closeWindow() {
    void import("@tauri-apps/api/window").then(({ getCurrentWindow }) => {
      void getCurrentWindow().close();
    });
  }

  async function complete(copyToClipboard: boolean) {
    const png = await canvasRef.current?.exportPng();
    if (!png) return;
    try {
      const savePath = copyToClipboard ? null : await api.saveFileDialog(`kiri-${props.id}.png`);
      await api.updateAsset(props.id, {
        png: Array.from(png),
        copyToClipboard,
        savePath: savePath ?? null,
      });
    } catch {
      return;
    }
    // Spec: copying finishes and closes the editor; saving keeps it open
    // so the user can continue (they can close manually or hit Copy).
    if (copyToClipboard) closeWindow();
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
    <div className="kiri-dark" style={{ height: "100%", display: "flex", flexDirection: "column", background: "#15131D" }}>
      {/* 58pt toolbar */}
      <div
        style={{
          height: 58,
          display: "flex",
          alignItems: "center",
          gap: 4,
          padding: "0 10px",
          borderBottom: "1px solid #40394E",
          background: "#1E1B28",
        }}
      >
        {TOOLS.map(({ tool: t2, icon, title }) => (
          <EditorToolButton
            key={t2}
            icon={icon}
            title={t(title)}
            active={tool === t2}
            onClick={() => setTool(t2)}
          />
        ))}
        <div style={{ width: 1, height: 26, background: "#40394E", margin: "0 4px" }} />
        {tool === "text" ? (
          <EditorSegments
            segments={[
              { icon: "square.dashed", label: t("Transparent"), title: t("No background") },
              { icon: "moon.fill", label: t("Dark"), title: t("Dark background") },
              { icon: "sun.max.fill", label: t("Light"), title: t("Light background") },
            ]}
            value={appearance.textBackgroundStyle === "transparent" ? 0 : appearance.textBackgroundStyle === "dark" ? 1 : 2}
            onChange={(i) =>
              setAppearance({ ...appearance, textBackgroundStyle: (["transparent", "dark", "light"] as TextBackgroundStyle[])[i] })
            }
          />
        ) : tool === "mosaic" ? (
          <EditorSegments
            segments={[{ label: "1" }, { label: "2" }, { label: "3" }]}
            value={appearance.mosaicIntensity === "soft" ? 0 : appearance.mosaicIntensity === "standard" ? 1 : 2}
            onChange={(i) =>
              setAppearance({ ...appearance, mosaicIntensity: (["soft", "standard", "strong"] as MosaicIntensity[])[i] })
            }
          />
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
              style={{ width: 90, accentColor: "#7D69F5" }}
            />
            <span style={{ width: 28, textAlign: "right", fontSize: 9, fontVariantNumeric: "tabular-nums" }}>
              {slider.value}
            </span>
          </div>
        )}
        <div style={{ width: 1, height: 26, background: "#40394E", margin: "0 4px" }} />
        {COLOR_PRESETS.map((preset) => (
          <EditorSwatch
            key={preset}
            color={COLOR_HEX[preset]}
            selected={appearance.colorPreset === preset}
            onClick={() => setAppearance({ ...appearance, colorPreset: preset })}
          />
        ))}
        <div style={{ width: 1, height: 26, background: "#40394E", margin: "0 4px" }} />
        <EditorToolButton icon="arrow.uturn.backward" title={t("Undo (⌘Z)")} disabled={!canUndo} onClick={() => canvasRef.current?.undo()} />
        <EditorToolButton icon="arrow.uturn.forward" title={t("Redo (⇧⌘Z)")} disabled={!canRedo} onClick={() => canvasRef.current?.redo()} />
        <EditorToolButton
          icon="xmark"
          title={t("Clear Annotations")}
          disabled={!canUndo && !canRedo}
          onClick={() => canvasRef.current?.clearAnnotations()}
        />
        <div style={{ flex: 1 }} />
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
            cursor: "default",
          }}
          onClick={() => void complete(false)}
        >
          {t("Save As…")}
        </button>
        <EditorToolButton icon="ellipsis.circle" title={t("Cancel (Esc)")} onClick={closeWindow} />
        <button
          className="kiri-primary-button"
          style={{ minHeight: 32, borderRadius: 10 }}
          onClick={() => void complete(true)}
        >
          {t("Copy")}
        </button>
      </div>

      {/* Canvas area */}
      <div ref={containerRef} style={{ flex: 1, display: "flex", alignItems: "center", justifyContent: "center", background: "#141414", position: "relative" }}>
        {imageSize && (
          <div style={{ position: "relative", width: region.width, height: region.height }}>
            <AnnotationCanvas
              ref={canvasRef}
              image={image}
              region={region}
              tool={tool}
              appearance={appearance}
              onHistoryChange={(u, r) => {
                setCanUndo(u);
                setCanRedo(r);
              }}
              onCancel={closeWindow}
            />
          </div>
        )}
      </div>
    </div>
  );
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
        background: props.active ? "rgba(125,105,245,0.32)" : "transparent",
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
        cursor: "default",
        position: "relative",
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
    <div style={{ display: "flex", background: "rgba(255,255,255,0.06)", borderRadius: 8, padding: 2, gap: 2 }}>
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
            background: props.value === index ? "#634FDB" : "transparent",
            color: "#fff",
            fontSize: 10,
            cursor: "default",
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
