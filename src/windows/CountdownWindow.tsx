// Recording countdown. The rest of the display remains clear and undimmed.
import { useEffect, useRef, useState } from "react";
import { api } from "../lib/ipc";
import { fmt, t } from "../i18n";
import { createCountdownClock, type CountdownClock } from "./countdown-clock.js";
import "./countdown.css";

export function CountdownWindow() {
  const sessionId = new URLSearchParams(window.location.search).get("session") ?? "";
  const reduceMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  const [tick, setTick] = useState({ value: 3, remaining: 1 });
  const [cancelling, setCancelling] = useState(false);
  const [error, setError] = useState<"start" | "cancel" | null>(null);
  const clock = useRef<CountdownClock | null>(null);
  const cancelled = useRef(false);
  const cancelPending = useRef(false);
  const cancelButton = useRef<HTMLButtonElement>(null);

  const cancel = () => {
    if (cancelPending.current) return;
    // Stop locally before IPC: Esc/click at the last tick must not enqueue start.
    cancelled.current = true;
    clock.current?.stop();
    cancelPending.current = true;
    setCancelling(true);
    setError(null);
    void api.cancelRecordingFlow(sessionId).catch(() => {
      cancelPending.current = false;
      setCancelling(false);
      setError("cancel");
    });
  };
  const cancelRef = useRef(cancel);
  cancelRef.current = cancel;

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        cancelRef.current();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  useEffect(() => {
    let disposed = false;
    let paintFrame = 0;
    const timer = createCountdownClock({
      now: () => performance.now(),
      requestFrame: (callback) => requestAnimationFrame(callback),
      cancelFrame: (frame) => cancelAnimationFrame(frame),
      onTick: setTick,
      onComplete: () => {
        if (disposed || cancelled.current) return;
        void api.beginRecording(sessionId).catch(() => {
          if (!disposed && !cancelled.current) setError("start");
        });
      },
    });
    clock.current = timer;
    // Native placement, exclusion and focus finish before the first three seconds.
    // Do not wait for RAF while hidden: some WebViews throttle hidden documents.
    void api.recordingCountdownReady(sessionId).then(() => {
      if (disposed || cancelled.current) return;
      cancelButton.current?.focus({ preventScroll: true });
      paintFrame = requestAnimationFrame(() => {
        paintFrame = requestAnimationFrame(() => {
          if (!disposed && !cancelled.current) timer.start();
        });
      });
    }).catch(() => {
      if (!disposed && !cancelled.current) setError("start");
    });
    return () => {
      disposed = true;
      cancelAnimationFrame(paintFrame);
      timer.stop();
      if (clock.current === timer) clock.current = null;
    };
  }, [sessionId]);

  return (
    <div className="kiri-countdown">
      <div className="kiri-countdown-ring">
        <svg viewBox="0 0 192 192" aria-hidden="true">
          <circle className="kiri-countdown-disc" cx="96" cy="96" r="87" />
          <circle className="kiri-countdown-track" cx="96" cy="96" r="87" />
          <circle
            className="kiri-countdown-progress"
            cx="96" cy="96" r="87" pathLength="1"
            strokeDasharray="1"
            strokeDashoffset={1 - (reduceMotion ? tick.value / 3 : tick.remaining)}
            transform="rotate(-90 96 96)"
          />
        </svg>
        <span
          className="kiri-countdown-number"
          role="status"
          aria-live="polite"
          aria-atomic="true"
          aria-label={fmt("Recording starts in %d", tick.value)}
        >
          {tick.value}
        </span>
        <button
          ref={cancelButton}
          type="button"
          className="kiri-countdown-cancel"
          onClick={cancel}
          disabled={cancelling}
        >
          <span className="kiri-countdown-stop" aria-hidden="true" />
          {t("Cancel Countdown")}
          <kbd aria-hidden="true">Esc</kbd>
        </button>
        {error && (
          <p className="kiri-countdown-error" role="alert">
            {t(error === "cancel"
              ? "Could not cancel recording. Try again."
              : "Could not start recording. Cancel and try again.")}
          </p>
        )}
      </div>
    </div>
  );
}
