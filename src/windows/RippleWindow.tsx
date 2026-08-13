// RippleWindow — the violet click ripple drawn over the recording region
// (RecordingClickHighlighterController.swift). This window is NOT excluded
// from capture, so ripples appear in the exported video.

import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";

interface ClickEvent {
  x: number;
  y: number;
}

interface Ripple {
  id: number;
  x: number;
  y: number;
  startedAt: number;
}

const VIOLET = "rgba(125, 105, 245, 1)"; // accent (0.49, 0.41, 0.96)

let nextId = 1;

export function RippleWindow() {
  const [ripples, setRipples] = useState<Ripple[]>([]);
  const [now, setNow] = useState(performance.now());

  useEffect(() => {
    const unlisten = listen<ClickEvent>("ripple-click", (event) => {
      const { x, y } = event.payload;
      setRipples((current) => [
        ...current.slice(-8),
        { id: nextId++, x, y, startedAt: performance.now() },
      ]);
    });
    const timer = setInterval(() => setNow(performance.now()), 50);
    return () => {
      void unlisten.then((fn) => fn());
      clearInterval(timer);
    };
  }, []);


  const visible = ripples.filter((ripple) => now - ripple.startedAt < 460);

  return (
    <div style={{ position: "fixed", inset: 0, background: "transparent", overflow: "hidden" }}>
      {visible.map((ripple) => {
        const t = now - ripple.startedAt;
        const halo = Math.min(t / 460, 1); // 42px halo, 0.46s
        const ring = Math.min(t / 340, 1); // 30px ring, 0.34s
        const center = Math.min(t / 240, 1); // 7px center, 0.24s
        const easeOut = (v: number) => 1 - Math.pow(1 - v, 3);
        // Scale keyTimes [0, 0.68, 1]; opacity plateau 1 until 0.68, then fade.
        const scale = (v: number) => easeOut(Math.min(v / 0.68, 1));
        const alpha = (v: number) =>
          v < 0.12 ? v / 0.12 : v < 0.68 ? 1 : (1 - (v - 0.68) / 0.32) * 0.9;
        return (
          <div key={ripple.id} style={{ position: "absolute", left: 0, top: 0 }}>
            <Ellipse
              x={ripple.x}
              y={ripple.y}
              width={42}
              scale={scale(halo)}
              opacity={alpha(halo)}
            />
            <Ellipse
              x={ripple.x}
              y={ripple.y}
              width={30}
              scale={scale(ring)}
              opacity={alpha(ring)}
            />
            <Ellipse
              x={ripple.x}
              y={ripple.y}
              width={7}
              scale={scale(center)}
              opacity={alpha(center)}
            />
          </div>
        );
      })}
    </div>
  );
}

function Ellipse(props: {
  x: number;
  y: number;
  width: number;
  scale: number;
  opacity: number;
}) {
  const { x, y, width, scale, opacity } = props;
  return (
    <div
      style={{
        position: "absolute",
        left: x - (width / 2) * scale,
        top: y - (width / 2) * scale,
        width: width * scale,
        height: width * scale,
        borderRadius: "50%",
        border: `2px solid ${VIOLET}`,
        opacity,
        transform: "translateZ(0)",
      }}
    />
  );
}
