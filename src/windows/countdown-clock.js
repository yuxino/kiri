// One clock per mounted countdown; native startup is never driven by CSS events.
export function createCountdownClock({ now, requestFrame, cancelFrame, onTick, onComplete }) {
  let startedAt = null;
  let frame = null;
  let stopped = false;
  const tick = () => {
    if (stopped || startedAt === null) return;
    const elapsed = Math.max(0, now() - startedAt);
    if (elapsed >= 3000) {
      stopped = true;
      frame = null;
      onComplete();
      return;
    }
    onTick({ value: 3 - Math.floor(elapsed / 1000), remaining: 1 - elapsed / 3000 });
    frame = requestFrame(tick);
  };
  return {
    start() {
      if (stopped || startedAt !== null) return;
      startedAt = now();
      tick();
    },
    stop() {
      stopped = true;
      if (frame !== null) cancelFrame(frame);
      frame = null;
    },
  };
}
