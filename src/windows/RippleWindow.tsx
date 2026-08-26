// Monochrome click ripple drawn over the recording region. This window is not
// excluded from capture, so enabled ripples appear in the exported video.

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

const WHITE = "rgba(255, 255, 255, 1)";

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
        // Spec §6.3 keyframes: scale keyTimes [0, 0.68, 1]; opacity
        // keyTimes [0, 0.12, 0.68, 1] with values [0, peak, peak*0.82, 0].
        const scaleAt = (from: number, to: number, time: number) => {
          const v = Math.min(time / 0.68, 1);
          return from + (to - from) * (1 - Math.pow(1 - v, 3));
        };
        const opacityAt = (peak: number, time: number) => {
          if (time < 0.12) return peak * (time / 0.12);
          if (time < 0.68) return peak;
          return peak * 0.82 * (1 - (time - 0.68) / 0.32);
        };
        return (
          <div key={ripple.id} style={{ position: "absolute", left: 0, top: 0 }}>
            {/* Halo: 42pt stroke, accent α0.30, width 6; 0.45→1.12, peak α0.72, 0.46s */}
            <Ellipse
              x={ripple.x}
              y={ripple.y}
              width={42}
              scale={scaleAt(0.45, 1.12, t / 460)}
              opacity={opacityAt(0.72, t / 460)}
              fill="none"
              stroke={WHITE.replace("1)", "0.30)")}
              strokeWidth={6}
            />
            {/* Ring: 30pt fill accent α0.12 + stroke α0.95 w2.5; 0.58→1.0, 0.34s */}
            <Ellipse
              x={ripple.x}
              y={ripple.y}
              width={30}
              scale={scaleAt(0.58, 1.0, t / 340)}
              opacity={opacityAt(1, t / 340)}
              fill={WHITE.replace("1)", "0.12)")}
              stroke={WHITE.replace("1)", "0.95)")}
              strokeWidth={2.5}
            />
            {/* Center: 7pt fill white α0.95 + accent stroke w1.5; 0.72→1.0, 0.24s */}
            <Ellipse
              x={ripple.x}
              y={ripple.y}
              width={7}
              scale={scaleAt(0.72, 1.0, t / 240)}
              opacity={opacityAt(1, t / 240)}
              fill="rgba(5,5,5,0.95)"
              stroke={WHITE}
              strokeWidth={1.5}
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
  fill?: string;
  stroke?: string;
  strokeWidth?: number;
}) {
  const { x, y, width, scale, opacity, fill, stroke, strokeWidth } = props;
  return (
    <div
      style={{
        position: "absolute",
        left: x - (width / 2) * scale,
        top: y - (width / 2) * scale,
        width: width * scale,
        height: width * scale,
        borderRadius: "50%",
        background: fill ?? "transparent",
        border: stroke ? `${strokeWidth ?? 1}px solid ${stroke}` : "none",
        boxSizing: "border-box",
        opacity,
        transform: "translateZ(0)",
      }}
    />
  );
}
