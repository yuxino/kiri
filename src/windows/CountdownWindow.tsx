// Compact 3-2-1 recording countdown. The selected region is never dimmed.

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
        role="status"
        aria-live="polite"
        aria-label={String(value)}
        style={{
          width: "1ch",
          textAlign: "center",
          color: "rgba(24, 24, 28, 0.94)",
          fontFamily:
            'ui-rounded, "SF Pro Rounded", "Segoe UI Variable Display", "Segoe UI Variable", system-ui, sans-serif',
          fontSize: "clamp(68px, 62vmin, 72px)",
          fontWeight: 700,
          fontVariantNumeric: "tabular-nums lining-nums",
          fontFeatureSettings: '"tnum" 1, "lnum" 1',
          lineHeight: 1,
          letterSpacing: "-0.025em",
          WebkitTextStroke: "1.1px rgba(255, 255, 255, 0.96)",
          animation: "kiri-countdown-enter 0.17s cubic-bezier(0.2, 0.8, 0.2, 1)",
        }}
      >
        {value}
      </div>
      <style>{`
        @keyframes kiri-countdown-enter {
          from { transform: scale(0.975); opacity: 0; }
          to { transform: scale(1); opacity: 1; }
        }
        @media (prefers-reduced-motion: reduce) {
          [role="status"] { animation: none !important; }
        }
      `}</style>
    </div>
  );
}
