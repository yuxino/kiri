import assert from "node:assert/strict";
import test from "node:test";
import { captureToolbarPosition } from "../src/windows/toolbar-layout.js";

test("800×600 bottom-right region leaves a measured wrapped toolbar above the selection", () => {
  const selection = { x: 650, y: 420, width: 140, height: 160 };
  const size = { width: 784, height: 81 };
  const position = captureToolbarPosition(selection, { x: 0, y: 0, width: 800, height: 600 }, size);
  assert.deepEqual(position, { left: 8, top: 329 });
  assert.equal(position.top + size.height, selection.y - 10);
});

test("wide displays retain the centered row below the selection", () => {
  assert.deepEqual(captureToolbarPosition(
    { x: 500, y: 200, width: 500, height: 150 },
    { x: 0, y: 0, width: 1512, height: 982 },
    { width: 810, height: 48 },
  ), { left: 345, top: 360 });
});

test("all corners remain inside logical viewport bounds after height changes", () => {
  for (const [width, height] of [[640, 480], [800, 600], [1512, 982]]) {
    for (const toolbarHeight of [48, 81, 116]) {
      for (const x of [0, width - 140]) for (const y of [0, height - 160]) {
        const size = { width: Math.min(810, width - 16), height: toolbarHeight };
        const p = captureToolbarPosition({ x, y, width: 140, height: 160 }, { x: 0, y: 0, width, height }, size);
        assert.ok(p.left >= 8 && p.left + size.width <= width - 8);
        assert.ok(p.top >= 96 && p.top + size.height <= height - 8);
      }
    }
  }
});

test("nonzero bounds and constrained height do not force the toolbar off screen", () => {
  const bounds = { x: -800, y: -200, width: 800, height: 180 };
  const size = { width: 784, height: 116 };
  assert.deepEqual(captureToolbarPosition(
    { x: -150, y: -100, width: 140, height: 70 }, bounds, size,
  ), { left: -792, top: -144 });
});
