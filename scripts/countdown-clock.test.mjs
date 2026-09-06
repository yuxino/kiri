import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import { createCountdownClock } from "../src/windows/countdown-clock.js";

function harness() {
  let time = 0, serial = 0, completed = 0;
  const frames = new Map(), ticks = [];
  const clock = createCountdownClock({
    now: () => time,
    requestFrame: (fn) => { frames.set(++serial, fn); return serial; },
    cancelFrame: (id) => frames.delete(id),
    onTick: (tick) => ticks.push(tick),
    onComplete: () => completed++,
  });
  return { clock, frames, ticks, completed: () => completed,
    advance(ms) { time += ms; const next = [...frames.values()]; frames.clear(); next.forEach(fn => fn()); },
  };
}

test("the countdown starts only when the native window is ready", () => {
  const h = harness(); h.advance(5000);
  assert.equal(h.completed(), 0); assert.equal(h.ticks.length, 0);
  h.clock.start(); assert.deepEqual(h.ticks.at(-1), { value: 3, remaining: 1 });
  h.advance(999); assert.equal(h.ticks.at(-1).value, 3);
  h.advance(1); assert.equal(h.ticks.at(-1).value, 2);
  h.advance(1000); assert.equal(h.ticks.at(-1).value, 1);
  h.advance(999); assert.equal(h.completed(), 0);
  h.advance(1); assert.equal(h.completed(), 1);
  h.advance(5000); h.clock.start(); assert.equal(h.completed(), 1);
});

test("click or Escape at the last frame cannot start recording", () => {
  const h = harness(); h.clock.start(); h.advance(2999);
  const late = [...h.frames.values()][0]; h.clock.stop();
  h.advance(1); late(); assert.equal(h.completed(), 0);
});

test("cancel during native readiness cannot restart a disposed clock", () => {
  const h = harness(); h.clock.stop(); h.advance(500); h.clock.start();
  h.advance(4000); assert.equal(h.completed(), 0); assert.equal(h.frames.size, 0);
});

test("StrictMode remount leaves only the current clock alive", () => {
  const stale = harness(); stale.clock.start(); stale.clock.stop();
  const current = harness(); current.clock.start(); current.clock.start();
  stale.advance(3000); current.advance(3000);
  assert.equal(stale.completed(), 0); assert.equal(current.completed(), 1);
});

test("delayed frames complete once without a zero or negative numeral", () => {
  const h = harness(); h.clock.start(); h.advance(9000); h.advance(9000);
  assert.equal(h.completed(), 1);
  assert.ok(h.ticks.every(({value, remaining}) => value >= 1 && value <= 3 && remaining <= 1 && remaining > 0));
});

test("the displayed circle and cancel action keep capture protection and scoped IPC", () => {
  const read = p => readFileSync(new URL(p, import.meta.url), "utf8");
  const ui = read("../src/windows/CountdownWindow.tsx");
  const css = read("../src/windows/countdown.css");
  const backend = read("../src-tauri/src/commands.rs");
  const panel = read("../src/windows/ControlPanelWindow.tsx");
  assert.match(ui, /api\.recordingCountdownReady\(sessionId\)/);
  assert.match(ui, /api\.cancelRecordingFlow\(sessionId\)/);
  assert.match(ui, /api\.beginRecording\(sessionId\)/);
  assert.match(ui, /className="kiri-countdown-action"/);
  assert.match(ui, /aria-label=\{t\("Cancel Countdown"\)\}/);
  assert.match(ui, /onClick=\{cancel\}/);
  assert.doesNotMatch(ui, /kiri-countdown-cancel|kiri-countdown-stop|<kbd/);
  assert.match(ui, /event\.key === "Escape"/);
  assert.match(css, /prefers-reduced-motion/);
  assert.doesNotMatch(css, /backdrop-filter|radial-gradient|linear-gradient/);
  assert.match(backend, /set_window_capture_excluded\(&app, "countdown", true\)/);
  assert.match(backend, /set_window_capture_excluded\(app, "control-panel", true\)/);
  assert.match(backend, /window\.set_focus\(\)\.map_err/);
  assert.match(panel, /api\.getRecordingState\(\)/);
  assert.match(panel, /!disposed && !receivedEvent/);
});
