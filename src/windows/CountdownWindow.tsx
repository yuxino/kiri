// CountdownWindow — the 3-2-1 recording countdown badge (RecordingCountdown
// Controller.swift). The region is never dimmed; only the badge is drawn.

import { useEffect, useState } from "react";
import { api } from "../lib/ipc";

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

  return (
    <div
      style={{
        position: "fixed",
        inset: 0,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        background: "transparent",
      }}
    >
      <div
        key={pulse}
        style={{
          width: size,
          height: size,
          borderRadius: "50%",
          background: "rgba(26, 20, 41, 0.92)",
          border: "1px solid rgba(255,255,255,0.16)",
          color: "#fff",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          fontSize: size * 0.44,
          fontWeight: 600,
          fontVariantNumeric: "tabular-nums",
          fontFamily:
            "ui-monospace, SFMono-Regular, Menlo, Consolas, monospace",
          animation: "kiri-countdown-beat 0.22s ease-out",
        }}
      >
        {value}
      </div>
      <style>{`
        @keyframes kiri-countdown-beat {
          from { transform: scale(0.76); }
          to { transform: scale(1); }
        }
      `}</style>
    </div>
  );
}
