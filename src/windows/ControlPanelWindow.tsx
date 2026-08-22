// ControlPanelWindow — recording controls (RecordingControlPanelController).
// Draggable 296×64 recording HUD. The backend keeps it above other windows
// and excludes it from the exported recording; the frontend remembers the
// user's preferred position.

import { useEffect, useRef, useState } from "react";
import { getCurrentWindow, LogicalPosition } from "@tauri-apps/api/window";
import { api, onRecordingState, type RecordingState } from "../lib/ipc";
import { t } from "../i18n";
import { KiriIcon, type IconName } from "../components/KiriIcons";

const ACCENT = "rgba(125, 105, 245, 1)"; // #7D69F5
const RED = "#FF3B30"; // system red (spec: Color.red)
const CORAL = "#FF80A8";

export function ControlPanelWindow() {
  const [state, setState] = useState<RecordingState | null>(null);

  useEffect(() => {
    const unlisten = onRecordingState(setState);
    return () => {
      void unlisten.then((dispose) => dispose()).catch(() => {});
    };
  }, []);

  // Apply the user's saved panel position on mount (default: bottom-right).
  useEffect(() => {
    const win = getCurrentWindow();
    const saved = localStorage.getItem("kiri-panel-pos");
    if (saved) {
      try {
        const { x, y } = JSON.parse(saved);
        void win.setPosition(new LogicalPosition(x, y)).catch(() => {});
      } catch {
        /* ignore malformed */
      }
    }
  }, []);

  // Dragging the panel (anywhere on the material surface) moves the window;
  // the final position is remembered for the next recording.
  const onPanelPointerDown = (e: React.PointerEvent) => {
    const target = e.target as HTMLElement;
    if (target.closest("button")) return; // don't drag from controls
    void getCurrentWindow().startDragging().catch(() => {});
  };

  // Recording hotkeys stay scoped to the focused control panel: Space
  // pauses/resumes and Esc stops, matching the original native controller.
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      const s = stateRef.current;
      if (!s) return;
      if (e.key === "Escape") {
        e.preventDefault();
        if (
          (s.isStarting || s.isRecording || s.isPaused) &&
          !s.isTransitioning &&
          !s.isFinalizing
        ) {
          void api.stopRecording().catch(() => {});
        }
        return;
      }
      if (e.code === "Space" && !e.repeat) {
        e.preventDefault();
        if (s.isStarting || s.isTransitioning || s.isFinalizing) return;
        if (s.isPaused) void api.resumeRecording().catch(() => {});
        else if (s.isRecording) void api.pauseRecording().catch(() => {});
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  // Remember the panel position after it moves (drag or otherwise).
  useEffect(() => {
    const win = getCurrentWindow();
    const unlisten = win.onMoved(() => {
      void Promise.all([win.outerPosition(), win.scaleFactor()])
        .then(([pos, scale]) => {
          const logical = pos.toLogical(scale);
          localStorage.setItem(
            "kiri-panel-pos",
            JSON.stringify({ x: logical.x, y: logical.y }),
          );
        })
        .catch(() => {});
    });
    return () => {
      void unlisten.then((dispose) => dispose()).catch(() => {});
    };
  }, []);

  const stateRef = useRef<RecordingState | null>(null);
  stateRef.current = state;

  const busy = Boolean(
    state?.isStarting || state?.isTransitioning || state?.isFinalizing,
  );
  const paused = Boolean(state?.isPaused);
  const hasSession = Boolean(state?.isRecording || state?.isPaused);
  const canStop = Boolean(state?.isStarting || hasSession);

  // All text is white on the dark material panel (spec: white foreground).
  const textStyle: React.CSSProperties = { color: "#fff" };

  return (
    <div
      style={{
        position: "fixed",
        inset: 0,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        // Spec §5.2: outer padding 4 around the rounded material panel.
        padding: 4,
        background: "transparent",
      }}
    >
      <div
        onPointerDown={onPanelPointerDown}
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          gap: 10,
          height: 56,
          width: "100%",
          borderRadius: 18,
          background: "rgba(30, 28, 40, 0.72)",
          backdropFilter: "blur(22px) saturate(1.4)",
          WebkitBackdropFilter: "blur(22px) saturate(1.4)",
          border: "1px solid rgba(255,255,255,0.14)",
          boxShadow: "0 6px 14px rgba(0,0,0,0.18)",
          boxSizing: "border-box",
        }}
      >
        <span
          style={{
            width: 10,
            height: 10,
            borderRadius: "50%",
            background: paused ? CORAL : RED,
            boxShadow:
              !paused && hasSession && !busy
                ? "0 0 0 4px rgba(255,59,48,0.22)"
                : "none",
            flexShrink: 0,
          }}
        />
        <span
          style={{
            ...textStyle,
            color: paused ? CORAL : textStyle.color,
            fontSize: 12,
            fontWeight: 600,
            fontVariantNumeric: "tabular-nums",
            minWidth: 58,
            textAlign: "left",
          }}
        >
          {paused
            ? t("Paused")
            : state?.elapsedLabel ?? t("Preparing recording")}
        </span>
        <div style={{ width: 1, height: 22, background: "rgba(255,255,255,0.16)" }} />
        {busy ? (
          <div
            title={t("Preparing recording")}
            style={{ width: 28, height: 28, display: "grid", placeItems: "center" }}
          >
            <Spinner />
          </div>
        ) : (
          <ControlButton
            icon={paused ? "play.fill" : "pause.fill"}
            title={paused ? t("Resume Recording") : t("Pause Recording")}
            disabled={!hasSession}
            onClick={() => {
              if (paused) void api.resumeRecording().catch(() => {});
              else void api.pauseRecording().catch(() => {});
            }}
          />
        )}
        <ControlButton
          icon="stop.fill"
          title={state?.isStarting ? t("Cancel") : t("Stop and Save Recording")}
          danger
          disabled={Boolean(state?.isTransitioning || state?.isFinalizing || !canStop)}
          onClick={() => void api.stopRecording().catch(() => {})}
        />
      </div>
    </div>
  );
}

function ControlButton(props: {
  icon: IconName;
  title: string;
  danger?: boolean;
  disabled?: boolean;
  onClick(): void;
}) {
  return (
    <button
      title={props.title}
      aria-label={props.title}
      disabled={props.disabled}
      onClick={props.onClick}
      style={{
        width: 28,
        height: 28,
        borderRadius: 9,
        border: "1px solid transparent",
        background: props.danger
          ? RED
          : "rgba(125,105,245,0.14)",
        color: props.danger ? "#fff" : ACCENT,
        fontSize: 13,
        cursor: props.disabled ? "default" : "pointer",
        opacity: props.disabled ? 0.46 : 1,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        transition:
          "background 0.14s ease-out, transform 0.14s ease-out, box-shadow 0.14s ease-out, color 0.14s ease-out",
      }}
      onMouseEnter={(e) => {
        // Consistent hover language: deepen the fill, lift slightly, and
        // brighten the icon so the control reads as interactive.
        e.currentTarget.style.background = props.danger
          ? "#FF4D42"
          : "rgba(125,105,245,0.32)";
        e.currentTarget.style.boxShadow = props.danger
          ? "0 2px 10px rgba(255, 59, 48, 0.5)"
          : "0 1px 6px rgba(0,0,0,0.3)";
        e.currentTarget.style.color = props.danger ? "#fff" : "#8f7bff";
        e.currentTarget.style.transform = "scale(1.04)";
      }}
      onMouseLeave={(e) => {
        e.currentTarget.style.background = props.danger
          ? RED
          : "rgba(125,105,245,0.14)";
        e.currentTarget.style.boxShadow = "none";
        e.currentTarget.style.color = props.danger ? "#fff" : ACCENT;
        e.currentTarget.style.transform = "scale(1)";
      }}
      onMouseDown={(e) => {
        e.currentTarget.style.transform = "scale(0.92)";
      }}
      onMouseUp={(e) => {
        e.currentTarget.style.transform = "scale(1)";
      }}
      onPointerLeave={(e) => {
        e.currentTarget.style.transform = "scale(1)";
      }}
    >
      <KiriIcon name={props.icon} size={13} />
    </button>
  );
}

function Spinner() {
  return (
    <div
      style={{
        width: 16,
        height: 16,
        border: "2px solid rgba(255,255,255,0.25)",
        borderTopColor: "#fff",
        borderRadius: "50%",
        animation: "kiri-spin 0.9s linear infinite",
      }}
    >
      <style>{`@keyframes kiri-spin { to { transform: rotate(360deg); } }`}</style>
    </div>
  );
}
