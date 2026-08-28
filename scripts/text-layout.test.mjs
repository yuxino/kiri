import assert from "node:assert/strict";
import test from "node:test";

import {
  fitTextEditorFrame,
  layoutTextLines,
} from "../src/annotation/text-layout.js";

const measureText = (text) => text.length * 10;

test("text layout preserves explicit and empty lines", () => {
  assert.deepEqual(layoutTextLines("A\nB", 100, measureText), ["A", "B"]);
  assert.deepEqual(layoutTextLines("A\n\nB", 100, measureText), ["A", "", "B"]);
  assert.deepEqual(layoutTextLines("  Kiri\nnext line  ", 200, measureText), [
    "  Kiri",
    "next line  ",
  ]);
});

test("text layout counts Latin and CJK soft wrapping", () => {
  assert.deepEqual(layoutTextLines("one two", 34, measureText), ["one", "two"]);
  assert.deepEqual(layoutTextLines("abcdef", 20, measureText), ["ab", "cd", "ef"]);
  assert.deepEqual(layoutTextLines("中文测试", 20, measureText), ["中文", "测试"]);
});

test("text editor frame covers wrapped lines and remains inside narrow bounds", () => {
  const multiline = fitTextEditorFrame({
    text: "one two",
    fontSize: 20,
    x: 40,
    y: 40,
    maxWidth: 50,
    boundsWidth: 50,
    boundsHeight: 100,
    measureText,
  });
  assert.deepEqual(multiline, { x: 0, y: 38, width: 50, height: 62 });

  const narrow = fitTextEditorFrame({
    text: "A\nB",
    fontSize: 20,
    x: 10,
    y: 10,
    maxWidth: 20,
    boundsWidth: 20,
    boundsHeight: 20,
    measureText,
  });
  assert.deepEqual(narrow, { x: 0, y: 0, width: 20, height: 20 });
});
