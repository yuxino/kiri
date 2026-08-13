// ControlPanelWindow — recording controls (RecordingControlPanelController).
// 296×64 HUD, always visible, excluded from the recording by the backend.

import { useEffect, useState } from "react";
import { api, onRecordingState, type RecordingState } from "../lib/ipc";
import { t } from "../i18n";

export function ControlPanelWindow() {
  const [state, setState] = useState<RecordingState | null>(null);

  useEffect(() => {
    void onRecordingState(setState);
  }, []);

  const busy =
    state?.isStarting || state?.isTransitioning || state?.isFinalizing || false;

  return (
    <div
      className="kiri-hud"
      style={{
        position: "fixed",
        inset: 0,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        gap: 10,
        padding: "0 14px",
        background: "rgba(30,27,40,0.92)",
      }}
    >
      {busy ? (
        <Spinner />
      ) : state?.isPaused ? (
        <>
          <span
            style={{
              width: 10,
              height: 10,
              borderRadius: "50%",
              background: "#FF80A8",
            }}
          />
          <span style={{ fontSize: 12.5, fontWeight: 600 }}>{t("Paused")}</span>
          <span
            style={{ fontSize: 12, fontVariantNumeric: "tabular-nums", opacity: 0.8 }}
          >
            {state.elapsedLabel}
          </span>
          <ControlButton
            glyph="▶"
            title={t("Resume Recording")}
            onClick={() => void api.resumeRecording()}
          />
        </>
      ) : state?.isRecording ? (
        <>
          <span
            style={{ width: 10, height: 10, borderRadius: "50%", background: "#FA476E" }}
          />
          <span
            style={{ fontSize: 12, fontVariantNumeric: "tabular-nums" }}
          >
            {state.elapsedLabel}
          </span>
          <ControlButton
            glyph="⏸"
            title={t("Pause Recording")}
            onClick={() => void api.pauseRecording()}
          />
        </>
      ) : (
        <span style={{ fontSize: 12, opacity: 0.8 }}>{t("Preparing recording")}</span>
      )}
      <div style={{ width: 1, height: 30, background: "rgba(255,255,255,0.16)" }} />
      <ControlButton
        glyph="■"
        title={t("Stop and Save Recording")}
        danger
        onClick={() => void api.stopRecording()}
      />
    </div>
  );
}

function ControlButton(props: {
  glyph: string;
  title: string;
  danger?: boolean;
  onClick(): void;
}) {
  return (
    <button
      title={props.title}
      onClick={props.onClick}
      style={{
        width: 32,
        height: 32,
        borderRadius: 10,
        border: "1px solid rgba(255,255,255,0.16)",
        background: props.danger ? "#FA476E" : "rgba(255,255,255,0.08)",
        color: "#fff",
        fontSize: 13,
        cursor: "default",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
      }}
    >
      {props.glyph}
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
