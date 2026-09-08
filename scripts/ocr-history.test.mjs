import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { createLibraryHarness, deferred, nodes, settleRequests, testAsset } from "./helpers/library-render-harness.mjs";

const source = 'import React from "react";\n' + readFileSync(new URL("../src/ocr/TextHistory.tsx", import.meta.url), "utf8");
const pause = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const record = (id, text) => ({ ...testAsset, id, ocrText: text });
const hasText = (tree, text) => nodes(tree).includes(text);
const button = (tree, text) => nodes(tree).find((node) => node?.type === "button" && hasText(node, text));

test("an older search response cannot replace the current text history", async () => {
  const first = deferred(), second = deferred();
  const queries = [];
  const harness = createLibraryHarness({ listOcrRecords: (query) => { queries.push(query); return query ? second.promise : first.promise; } }, source);
  const component = harness.mount("TextHistory");
  let tree = component.render();
  await pause(5);
  nodes(tree).find((node) => node?.type === "input").props.onChange({ target: { value: "new" } });
  component.render();
  await pause(170);
  second.resolve([record("new", "Newest result")]);
  await settleRequests();
  tree = component.render();
  assert.equal(hasText(tree, "Newest result"), true);
  first.resolve([record("old", "Old result")]);
  await settleRequests();
  tree = component.render();
  assert.equal(hasText(tree, "Newest result"), true);
  assert.equal(hasText(tree, "Old result"), false);
  assert.deepEqual(queries, ["", "new"]);
  component.unmount();
});

test("copy completion for another record never marks the newly selected text copied", async () => {
  const pending = deferred();
  const copied = [];
  const harness = createLibraryHarness({ copyHistoryText: (text) => { copied.push(text); return pending.promise; } }, source);
  const component = harness.mount("TextReader", { text: "first", imageId: "one" });
  let tree = component.render();
  button(tree, "Copy Text").props.onClick();
  component.render({ text: "second", imageId: "two" });
  pending.resolve(); await settleRequests();
  tree = component.render();
  assert.deepEqual(copied, ["first"]);
  assert.equal(hasText(tree, "Text Copied"), false);
  component.unmount();
});

test("copy failures and unsaved recognition remain visible without losing text", async () => {
  const harness = createLibraryHarness({ copyHistoryText: async () => { throw new Error("clipboard unavailable"); } }, source);
  const component = harness.mount("TextReader", { text: "keep this text", saved: false });
  let tree = component.render();
  button(tree, "Copy Text").props.onClick(); await settleRequests(); tree = component.render();
  assert.equal(hasText(tree, "keep this text"), true);
  assert.equal(hasText(tree, "Couldn't copy text."), true);
  assert.equal(hasText(tree, "History wasn't saved. Copy the text before closing."), true);
  component.unmount();
});

test("OCR dialog starts once and reads saved text without recognizing again", async () => {
  let calls = 0;
  const pending = deferred();
  const harness = createLibraryHarness({ recognizeAssetLocal: () => { calls++; return pending.promise; } }, source);
  const component = harness.mount("OcrDialog", { asset: testAsset, onClose() {} });
  component.render(); component.render(); assert.equal(calls, 1);
  component.unmount(); pending.resolve({ text: "complete", saved: true, asset: record("new", "complete") }); await settleRequests();
  const saved = harness.mount("OcrDialog", { asset: record("saved", "already recognized"), onClose() {} });
  saved.render(); assert.equal(calls, 1); saved.unmount();
});

test("load failures are not presented as empty history", async () => {
  const harness = createLibraryHarness({ listOcrRecords: async () => { throw new Error("offline"); } }, source);
  const component = harness.mount("TextHistory"); component.render(); await pause(5); await settleRequests();
  const tree = component.render();
  assert.equal(hasText(tree, "Couldn't load Text History."), true);
  assert.equal(hasText(tree, "Your text, ready to revisit"), false);
  component.unmount();
});
