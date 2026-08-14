// CountdownWindow — the 3-2-1 recording countdown badge (RecordingCountdown
// Controller.swift). The region is never dimmed; only the badge is drawn.
// Visual details per recording.md §5.4.

import { useEffect, useState } from "react";
import { api } from "../lib/ipc";
import { t } from "../i18n";

export function CountdownWindow() {
  const [value, setValue] = useState(3);
  const [pulse, setPulse] = useState(0);

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        void api.cancelRecordingFlow().catch(() => {});
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  useEffect(() => {
    const started = Date.now();
    const timer = setInterval(() => {
      const elapsed = (Date.now() - started) / 1000;
      const next = 3 - Math.floor(elapsed);
      if (next <= 0) {
        clearInterval(timer);
        void api.beginRecording().catch(() => {});
        return;
      }
      setValue(next);
      setPulse((p) => p + 1);
    }, 1000);
    return () => clearInterval(timer);
  }, []);

  const size = Math.min(96, Math.max(68, Math.min(window.innerWidth, window.innerHeight) - 16));
  if (window.innerWidth < 2000) {
    console.log(`[countdown] window=${window.innerWidth}x${window.innerHeight} badgeSize=${size}`);
  }
  // Spec §5.4: font min(46, size*0.48); digit lifted 6pt; hide Esc hint
  // when the badge is small.
  const fontSize = Math.min(46, size * 0.48);
  const showHint = size >= 80;

  return (
    <div
      style={{
        position: "fixed",
        inset: 0,
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        justifyContent: "center",
        gap: 12,
        background: "transparent",
      }}
    >
      <div
        key={pulse}
        style={{
          width: size,
          height: size,
          borderRadius: "50%",
          boxSizing: "border-box",
          background: "rgba(26, 20, 41, 0.92)",
          // Spec: 1.5pt accentSoft α0.92 border, black shadow α0.32 r20 y-5.
          border: "1.5px solid rgba(171, 148, 255, 0.92)",
          boxShadow: "0 -5px 20px rgba(0, 0, 0, 0.32)",
          color: "#fff",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          fontSize,
          fontWeight: 600,
          fontVariantNumeric: "tabular-nums",
          fontFamily:
            "ui-monospace, SFMono-Regular, Menlo, Consolas, monospace",
          animation: "kiri-countdown-beat 0.22s ease-out",
        }}
      >
        {value}
      </div>
      {showHint && (
        <span
          style={{
            fontSize: 9,
            fontWeight: 500,
            color: "rgba(255,255,255,0.68)",
            // Clear gap below the badge so the hint never touches it.
            marginTop: 10,
            pointerEvents: "none",
          }}
        >
          {t("Esc to cancel")}
        </span>
      )}
      <style>{`
        @keyframes kiri-countdown-beat {
          from { transform: scale(0.76); opacity: 0; }
          to { transform: scale(1); opacity: 1; }
        }
      `}</style>
    </div>
  );
}
