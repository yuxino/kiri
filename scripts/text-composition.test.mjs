import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import ts from "typescript";
import { handleTextEditorKey, isTextComposition, setTextComposition } from "../src/annotation/text-composition.js";
import { installVideoProjectShortcuts } from "../src/windows/video-project-shortcuts.js";

function key(options = {}) {
  return { key: "Enter", target: {}, isComposing: false, keyCode: 13,
    defaultPrevented: false, stopped: false,
    preventDefault() { this.defaultPrevented = true; },
    stopPropagation() { this.stopped = true; }, ...options };
}
function actions() {
  const calls = [];
  return { calls, ...Object.fromEntries(["cancel", "commit", "undo", "redo", "finish"]
    .map(name => [name, () => calls.push(name)])) };
}

test("IME Enter/Escape/undo preserve composition and never act on the canvas", () => {
  for (const signal of [{ isComposing: true }, { keyCode: 229 }, { lifecycle: true }]) {
    for (const eventKey of ["Enter", "Escape", "z"]) {
      const e = key({ ...signal, key: eventKey, ctrlKey: eventKey === "z" });
      if (signal.lifecycle) setTextComposition(e.target, true);
      const a = actions();
      // Check the same predicate used before the textarea in window capture.
      assert.equal(isTextComposition(e), true);
      handleTextEditorKey(e, a, true);
      assert.deepEqual(a.calls, []);
      assert.equal(e.defaultPrevented, false);
      assert.equal(e.stopped, true);
      setTextComposition(e.target, false);
    }
  }
});
test("composition end or blur releases only its own input", () => {
  const first = {}, second = {};
  setTextComposition(first, true);
  assert.equal(isTextComposition(key({ target: second })), false);
  assert.equal(isTextComposition(key({ nativeEvent: { target: first, isComposing: false, keyCode: 13 } })), true);
  setTextComposition(first, false);
  const e = key({ target: first }), a = actions();
  handleTextEditorKey(e, a, true);
  assert.deepEqual(a.calls, ["commit", "finish"]);
});
test("native Ctrl/Cmd undo and redo do not commit or prevent native input history", () => {
  for (const modifier of ["ctrlKey", "metaKey"]) for (const shiftKey of [false, true]) {
    const e = key({ key: "z", [modifier]: true, shiftKey }), a = actions();
    handleTextEditorKey(e, a, true);
    assert.deepEqual(a.calls, []);
    assert.equal(e.defaultPrevented, false);
    assert.equal(e.stopped, true);
  }
});
test("normal Escape cancels edit; Shift+Enter keeps newline; legacy history stays ordered", () => {
  const esc = key({ key: "Escape" }), a = actions();
  handleTextEditorKey(esc, a, true);
  assert.deepEqual(a.calls, ["cancel"]);
  assert.equal(esc.defaultPrevented, true);
  const newline = key({ shiftKey: true }), b = actions();
  handleTextEditorKey(newline, b, true);
  assert.deepEqual(b.calls, []);
  assert.equal(newline.defaultPrevented, false);
  const redo = key({ key: "z", ctrlKey: true, shiftKey: true }), c = actions();
  handleTextEditorKey(redo, c, false);
  assert.deepEqual(c.calls, ["commit", "redo"]);
});
test("capture-phase video save/close does not commit an active IME even with false flags", () => {
  let handler;
  const surface = { addEventListener(_, callback) { handler = callback; }, removeEventListener() {} };
  const calls = [];
  const stop = installVideoProjectShortcuts(surface, { save: () => calls.push("save"), close: () => calls.push("close") });
  const target = {};
  setTextComposition(target, true);
  for (const eventKey of ["s", "w"]) {
    const e = key({ target, key: eventKey, ctrlKey: true });
    handler(e);
    assert.deepEqual(calls, []);
    assert.equal(e.defaultPrevented, false);
  }
  setTextComposition(target, false);
  handler(key({ target, key: "s", ctrlKey: true }));
  assert.deepEqual(calls, ["save"]);
  stop();
});

// Execute the actual window handlers with isolated action boundaries. This
// catches a missed integration, not only correctness of the shared predicate.
function windowHandler(filename, name) {
  const source = readFileSync(new URL(`../src/windows/${filename}`, import.meta.url), "utf8");
  const tree = ts.createSourceFile(filename, source, ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX);
  let handler;
  function visit(node) {
    if (ts.isVariableDeclaration(node) && node.name.getText(tree) === name && node.initializer) {
      assert.equal(handler, undefined, "handler must have an unambiguous production boundary");
      handler = node.initializer.getText(tree);
    }
    ts.forEachChild(node, visit);
  }
  visit(tree);
  assert.ok(handler);
  const calls = [];
  const action = name => () => calls.push(name);
  const context = { isTextComposition, completionLock: { locked: false },
    phaseRef: { current: "annotating" }, tool: "select",
    canvasRef: { current: { cancelTextEditing: () => false,
      undo: action("undo"), redo: action("redo"), deleteSelection: action("delete") } },
    cancel: action("cancel"), closeWindow: action("close"), complete: action("complete"),
    cancelCrop: action("cancelCrop"), selectTool: action("selectTool"), Element: class Element {} };
  const compiled = ts.transpileModule(`const handler = ${handler};`, {
    compilerOptions: { target: ts.ScriptTarget.ES2022 }
  }).outputText;
  const run = new Function(...Object.keys(context), `${compiled}\nreturn handler;`)(...Object.values(context));
  return { run, calls };
}
for (const [file, name, ordinaryKey, expected] of [
  ["OverlayWindow.tsx", "onEscape", "Escape", "cancel"],
  ["OverlayWindow.tsx", "onKeyDown", "Enter", "complete"],
  ["EditorWindow.tsx", "onKeyDown", "Escape", "close"],
]) test(`${file} ${name} leaves composition with false key flags alone`, () => {
  const { run, calls } = windowHandler(file, name), target = {};
  setTextComposition(target, true);
  for (const eventKey of ["Enter", "Escape", "z"]) {
    const e = key({ target, key: eventKey, ctrlKey: eventKey === "z",
      stopImmediatePropagation() { this.stopped = true; } });
    run(e);
    assert.deepEqual(calls, []);
    assert.equal(e.defaultPrevented, false);
  }
  setTextComposition(target, false);
  run(key({ target, key: ordinaryKey, stopImmediatePropagation() { this.stopped = true; } }));
  assert.deepEqual(calls, [expected]);
});
