import assert from "node:assert/strict";
import test from "node:test";

import { resolveInitialEditorDocument } from "../src/annotation/editor-document.js";

function exportScale(document) {
  return {
    x: document.sourcePixels.width / document.canvas.width,
    y: document.sourcePixels.height / document.canvas.height,
  };
}

test("legacy editor geometry is stable across 1x and 2x windows", () => {
  const sourcePixels = { width: 1200, height: 800 };
  const defaultTextFontSize = 18;
  // DPR is deliberately not an input: a legacy bitmap has no trustworthy
  // capture-scale metadata, so both windows resolve the same pixel canvas.
  const openedAtOneX = resolveInitialEditorDocument(null, sourcePixels);
  const openedAtTwoX = resolveInitialEditorDocument(null, sourcePixels);

  assert.deepEqual(openedAtOneX, openedAtTwoX);
  assert.deepEqual(openedAtOneX.canvas, sourcePixels);
  assert.deepEqual(exportScale(openedAtOneX), { x: 1, y: 1 });
  assert.deepEqual(exportScale(openedAtTwoX), { x: 1, y: 1 });
  assert.equal(defaultTextFontSize * exportScale(openedAtOneX).x, 18);
  assert.equal(defaultTextFontSize * exportScale(openedAtTwoX).x, 18);
});

test("existing V1 projects keep their persisted logical canvas", () => {
  const project = {
    schemaVersion: 1,
    canvas: { width: 600, height: 400 },
    sourcePixels: { width: 1200, height: 800 },
    marks: [],
  };

  assert.equal(
    resolveInitialEditorDocument(project, project.sourcePixels),
    project,
  );
  assert.deepEqual(project.canvas, { width: 600, height: 400 });
  assert.deepEqual(exportScale(project), { x: 2, y: 2 });
});
