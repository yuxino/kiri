import assert from "node:assert/strict";
import test from "node:test";

import {
  cropAnnotationDocument,
  cropPixelsFromDocumentRect,
  fullCropRect,
  isFullCrop,
} from "../src/annotation/crop.js";

function documentWithMarks() {
  return {
    schemaVersion: 1,
    canvas: { width: 600, height: 400 },
    sourcePixels: { width: 1200, height: 800 },
    marks: [
      { kind: "rectangle", id: 1, rect: { x: 50, y: 40, width: 100, height: 80 }, color: "violet", width: 3 },
      { kind: "line", id: 2, start: { x: 250, y: 150 }, end: { x: 500, y: 300 }, color: "orange", width: 3 },
      { kind: "text", id: 3, text: "outside", rect: { x: 500, y: 20, width: 80, height: 30 }, color: "white", background: "transparent", fontSize: 18 },
    ],
  };
}

test("full crop is a no-op at Retina scale", () => {
  const document = documentWithMarks();
  const full = fullCropRect(document);
  assert.equal(isFullCrop(document, full), true);
  assert.deepEqual(cropPixelsFromDocumentRect(document, full), {
    x: 0,
    y: 0,
    width: 1200,
    height: 800,
  });
});

test("crop snaps to source pixels, translates intersections, and drops outside marks", () => {
  const result = cropAnnotationDocument(documentWithMarks(), {
    x: 100.2,
    y: 79.8,
    width: 300.1,
    height: 200.4,
  });
  assert.deepEqual(result.cropPixels, { x: 200, y: 160, width: 601, height: 400 });
  assert.deepEqual(result.document.canvas, { width: 300.5, height: 200 });
  assert.deepEqual(result.document.sourcePixels, { width: 601, height: 400 });
  assert.deepEqual(result.document.marks.map((mark) => mark.id), [1, 2]);
  assert.deepEqual(result.document.marks[0].rect, {
    x: -50,
    y: -40,
    width: 100,
    height: 80,
  });
  assert.deepEqual(result.document.marks[1].start, { x: 150, y: 70 });
});

test("a crossing mark survives even when both endpoints are outside", () => {
  const document = documentWithMarks();
  document.marks = [{
    kind: "line",
    id: 9,
    start: { x: 0, y: 200 },
    end: { x: 600, y: 200 },
    color: "white",
    width: 2,
  }];
  const result = cropAnnotationDocument(document, { x: 200, y: 100, width: 100, height: 200 });
  assert.equal(result.document.marks.length, 1);
  assert.deepEqual(result.document.marks[0].start, { x: -200, y: 100 });
  assert.deepEqual(result.document.marks[0].end, { x: 400, y: 100 });
});

test("an arrow survives when only its minimum-size head enters the crop", () => {
  const document = documentWithMarks();
  document.marks = [{
    kind: "arrow",
    id: 10,
    start: { x: 40, y: 120 },
    end: { x: 90, y: 120 },
    color: "white",
    width: 1,
  }];
  const result = cropAnnotationDocument(document, { x: 76, y: 113, width: 20, height: 2 });
  assert.equal(result.document.marks.length, 1);
});
