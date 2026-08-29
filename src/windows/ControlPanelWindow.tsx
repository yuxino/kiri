// ControlPanelWindow — recording controls (RecordingControlPanelController).
// Draggable 296×64 recording HUD. The backend keeps it above other windows
// and excludes it from the exported recording; the frontend remembers the
// user's preferred position.

import { useEffect, useRef, useState } from "react";
import { getCurrentWindow, LogicalPosition } from "@tauri-apps/api/window";
import { api, onRecordingState, type RecordingState } from "../lib/ipc";
import { t } from "../i18n";
import { KiriIcon, type IconName } from "../components/KiriIcons";

const RED = "#FF3B30"; // system red (spec: Color.red)
const PAUSED = "rgba(255,255,255,0.72)";

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
      className="kiri-dark"
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
          borderRadius: 12,
          background: "rgba(8, 8, 8, 0.88)",
          backdropFilter: "blur(22px)",
          WebkitBackdropFilter: "blur(22px)",
          border: "1px solid rgba(255,255,255,0.22)",
          boxShadow: "none",
          boxSizing: "border-box",
        }}
      >
        <span
          style={{
            width: 10,
            height: 10,
            borderRadius: "50%",
            background: paused ? PAUSED : RED,
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
            color: paused ? PAUSED : textStyle.color,
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
      type="button"
      className="kiri-record-control-button"
      data-danger={props.danger || undefined}
      title={props.title}
      aria-label={props.title}
      disabled={props.disabled}
      onClick={props.onClick}
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
