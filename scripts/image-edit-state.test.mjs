import test from "node:test";
import assert from "node:assert/strict";
import { hasUnsavedImageChanges } from "../src/annotation/image-edit-state.js";

const text = { kind: "text", id: 1, text: "saved", rect: { x: 8, y: 5, width: 80, height: 24 }, color: "cherry", background: "transparent", fontSize: 18 };
const saved = { marks: [text], crop: null };
test("saved annotations and undo back to baseline close without a warning", () => {
  assert.equal(hasUnsavedImageChanges(saved, [structuredClone(text)], null, null), false);
  assert.equal(hasUnsavedImageChanges(saved, [], null, null), true);
  assert.equal(hasUnsavedImageChanges(saved, [text, { ...text, id: 2 }], null, null), true);
  assert.equal(hasUnsavedImageChanges(saved, saved.marks, null, null), false);
});
test("new nonempty text and deletion of existing text are protected before commit", () => {
  assert.equal(hasUnsavedImageChanges(saved, saved.marks, null, { editing: true, previousId: null, mark: { ...text, id: 2 } }), true);
  assert.equal(hasUnsavedImageChanges(saved, saved.marks, null, { editing: true, previousId: 1, mark: null }), true);
  assert.equal(hasUnsavedImageChanges(saved, saved.marks, null, { editing: true, previousId: null, mark: null }), false);
});
test("opening or cancelling text editing is clean; content and appearance changes are dirty", () => {
  const draft = { editing: true, previousId: 1, mark: { ...text, rect: { ...text.rect, width: 120 } } };
  assert.equal(hasUnsavedImageChanges(saved, saved.marks, null, draft), false);
  for (const patch of [{ text: "changed" }, { color: "blueberry" }, { background: "dark" }, { fontSize: 32 }]) {
    assert.equal(hasUnsavedImageChanges(saved, saved.marks, null, { ...draft, mark: { ...draft.mark, ...patch } }), true);
  }
  assert.equal(hasUnsavedImageChanges(saved, saved.marks, null, { ...draft, editing: false, mark: null }), false);
});
test("crop changes and a successful Save As snapshot participate in the same baseline", () => {
  const crop = { x: 0, y: 0, width: 80, height: 60 };
  assert.equal(hasUnsavedImageChanges(saved, saved.marks, crop, null), true);
  const nextSaved = { marks: saved.marks, crop };
  assert.equal(hasUnsavedImageChanges(nextSaved, saved.marks, { ...crop }, null), false);
  assert.equal(hasUnsavedImageChanges(nextSaved, saved.marks, null, null), true);
});
