import test from "node:test";
import assert from "node:assert/strict";
import {readFileSync} from "node:fs";
import {createLibraryHarness, nodes} from "./helpers/library-render-harness.mjs";
import ts from "typescript";
import * as layout from "../src/annotation/text-layout.js";
import * as calloutLayout from "../src/annotation/callout-layout.js";
import * as composition from "../src/annotation/text-composition.js";

const canvasSource = readFileSync(new URL("../src/annotation/AnnotationCanvas.tsx", import.meta.url), "utf8");
const tree = ts.createSourceFile("AnnotationCanvas.tsx", canvasSource, ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX);
const editor = tree.statements.find(node => ts.isFunctionDeclaration(node) && node.name?.text === "TextEditor");
assert.ok(editor);
const source = `import React, {useRef, useCallback, useEffect, useLayoutEffect, useId} from "react";
import {t} from "../i18n";
import {COLOR_HEX} from "./model";
import {textFont} from "./render";
import {fitTextEditorFrame, layoutTextLines, TEXT_TAB_SIZE, textEditorInsets} from "./text-layout.js";
import {calloutLabelSize, repairCalloutLabelHeight} from "./callout-layout.js";
import {handleTextEditorKey, isTextComposition, setTextComposition} from "./text-composition.js";
${editor.getText(tree)}
export {TextEditor};`;

function input(text = "", options = {}) {
  let value = "";
  const writes = [], changes = [], commands = [], moves = [], rects = [];
  const textarea = {
    style: {},
    getBoundingClientRect() {return {left: node.props.style.left, top: node.props.style.top,
      width: node.props.style.width, height: node.props.style.height};},
    setPointerCapture() {},
    selectionStart: 0, selectionEnd: 0,
    get value() {return value;},
    set value(next) {writes.push(next); value = next; this.selectionStart = this.selectionEnd = next.length;},
    ownerDocument: {execCommand(command) {commands.push(command); return true;}},
    focus() {this.ownerDocument.activeElement = this;},
    blur() {node.props.onBlur({currentTarget: textarea});},
    type(next) {
      const start = this.selectionStart;
      value = value.slice(0, start) + next + value.slice(this.selectionEnd);
      this.selectionStart = this.selectionEnd = start + next.length;
      node.props.onChange({currentTarget: this, target: this});
    },
    nativeUndo(next) {
      value = next; this.selectionStart = this.selectionEnd = next.length;
      node.props.onChange({currentTarget: this, target: this});
    },
  };
  const h = createLibraryHarness({}, source, {
    modules: {"./model": {COLOR_HEX: {cherry: "#f53b58"}}, "./text-composition.js": composition,
      "./text-layout.js": layout, "./render": {textFont: size => `600 ${size}px sans-serif`},
      "./callout-layout.js": calloutLayout},
    attachRef(node) {if (node.type === "textarea") node.props.ref(textarea);},
    document: {createElement: () => ({getContext: () => ({measureText: text => ({width: options.measureText?.(text) ?? text.length * 9})})})},
  });
  let finishes = 0;
  const props = {editing: {id: 1, index: 0, text, callout: {}, rect: {x: 130, y: 80, width: 160, height: 41},
    maxWidth: 280, uiScale: 1, fontSize: 18, color: "cherry", background: "transparent"},
    bounds: {width: 640, height: 360}, disabled: false, onTextChange: text => changes.push(text),
    onRectChange: rect => rects.push(rect), onCommit: () => finishes++, onCancel: () => finishes++, onUndo() {}, onRedo() {}, nativeUndo: true,
    onMoveCallout: (event, first) => moves.push({clientX: event.clientX, clientY: event.clientY, first}), ...options};
  const component = h.mount("TextEditor", props);
  let node = nodes(component.render()).find(node => node?.type === "textarea");
  return {textarea, writes, changes, commands, moves, rects, finishes: () => finishes,
    render(text, patch = {}) {node = nodes(component.render({...props, editing: {...props.editing, text, ...patch}})).find(node => node?.type === "textarea"); return node;},
    buttons() {return nodes(component.render()).filter(node => node?.type === "button");},
    pointer(name, clientX, clientY, options = {}) {
      const event = {clientX, clientY, button: 0, pointerId: 1, currentTarget: textarea, target: textarea,
        defaultPrevented: false, stopped: false,
        preventDefault() {this.defaultPrevented = true;}, stopPropagation() {this.stopped = true;}, ...options};
      assert.equal(typeof node.props[name], "function", `native textarea exposes ${name}`);
      node.props[name](event); return event;
    },
    key(options = {}) {
      const event = {key: "Enter", currentTarget: textarea, target: textarea,
        defaultPrevented: false, stopped: false,
        preventDefault() {this.defaultPrevented = true;}, stopPropagation() {this.stopped = true;}, ...options};
      node.props.onKeyDown(event); return event;
    },
    compose(active) {node.props[active ? "onCompositionStart" : "onCompositionEnd"]({currentTarget: textarea});},
  };
}

test("delayed callout selection echoes preserve continuous input and the native caret", () => {
  const h = input();
  assert.equal(h.textarea.ownerDocument.activeElement, h.textarea);
  for (const char of "abcdef") h.textarea.type(char);
  h.textarea.selectionStart = h.textarea.selectionEnd = 3;
  h.textarea.type("XY");
  for (const text of ["a", "ab", "abc", "abcd", "abcde", "abcdef", "abcXYdef"]) {
    const node = h.render(text);
    assert.equal(node.props.value, undefined);
    assert.equal(node.props.defaultValue, undefined);
    assert.equal(node.props.style.background, "transparent");
    assert.equal(h.textarea.value, "abcXYdef");
    assert.equal(h.textarea.selectionStart, 5);
  }
  assert.deepEqual(h.changes, ["a", "ab", "abc", "abcd", "abcde", "abcdef", "abcXYdef"]);
  assert.deepEqual(h.writes, [""]);
});

test("inline input needs no black move grip and all annotation inputs remain transparent without a white focus ring", () => {
  const h = input();
  h.render("", {rect: {x: 480, y: 0, width: 160, height: 41}});
  assert.deepEqual(h.buttons(), []);
  for (const patch of [{}, {callout: undefined, background: "dark"}, {callout: undefined, watermark: {}}]) {
    const text = h.render("saved", patch);
    assert.equal(text.props.style.background, "transparent");
    assert.equal(text.props.style.outline, "none"); assert.equal(text.props.style.boxShadow, "none");
  }
});

test("description text keeps native selection while its frame starts moving only at three CSS pixels", () => {
  const h = input("saved");
  h.textarea.selectionStart = 1; h.textarea.selectionEnd = 3;
  assert.equal(h.pointer("onPointerDown", 160, 100).defaultPrevented, false);
  h.pointer("onPointerMove", 180, 100); h.pointer("onPointerUp", 180, 100);
  assert.deepEqual(h.moves, []);
  assert.equal(h.textarea.selectionStart, 1); assert.equal(h.textarea.selectionEnd, 3);
  assert.equal(h.pointer("onPointerDown", 131, 100).defaultPrevented, true);
  h.pointer("onPointerMove", 133, 100); h.pointer("onPointerUp", 133, 100);
  assert.deepEqual(h.moves, [], "frame click and jitter leave input/caret in the editor");
  h.pointer("onPointerDown", 131, 100); h.pointer("onPointerMove", 141, 105);
  assert.deepEqual(h.moves, [{clientX: 141, clientY: 105, first: {x: 131, y: 100}}]);
  h.pointer("onPointerMove", 150, 110);
  assert.equal(h.moves.length, 1, "the original start is handed to Canvas once");
  assert.equal(h.textarea.value, "saved"); assert.deepEqual(h.writes, ["saved"]);
  assert.equal(h.textarea.selectionStart, 1); assert.equal(h.textarea.selectionEnd, 3);
});

test("description frame dragging respects IME, cancellation, pointer identity and disabled state", () => {
  const h = input("中文");
  h.compose(true);
  assert.equal(h.pointer("onPointerDown", 131, 100).defaultPrevented, true);
  h.pointer("onPointerMove", 145, 110); assert.deepEqual(h.moves, []);
  h.compose(false);
  h.pointer("onPointerDown", 131, 100); h.pointer("onPointerMove", 145, 110, {pointerId: 2});
  h.pointer("onPointerCancel", 131, 100); h.pointer("onPointerMove", 145, 110);
  assert.deepEqual(h.moves, []);
  h.pointer("onPointerDown", 131, 100); h.pointer("onLostPointerCapture", 131, 100);
  h.pointer("onPointerMove", 145, 110); assert.deepEqual(h.moves, []);
  h.pointer("onPointerDown", 131, 100); h.compose(true); h.pointer("onPointerMove", 145, 110);
  assert.deepEqual(h.moves, []); h.compose(false);
  h.pointer("onPointerDown", 131, 100); h.pointer("onPointerMove", 141, 105);
  assert.equal(h.moves.length, 1); assert.deepEqual(h.writes, ["中文"]);
  const disabled = input("saved", {disabled: true});
  disabled.pointer("onPointerDown", 131, 100); disabled.pointer("onPointerMove", 145, 110);
  assert.deepEqual(disabled.moves, []);
});

test("callout padding includes its border without narrowing the rendered text area", () => {
  const h = input();
  const node = h.render("saved");
  assert.equal(node.props.style.padding, 8);
  assert.equal(2 * (node.props.style.padding + 1), 18);
});

test("an unchanged callout with a short saved frame repairs height without resetting native input or IME", () => {
  const text = "Dev drag abcdef\n中文保存测试", rect = {x: 130, y: 80, width: 118, height: 49};
  const measureText = value => [...value].reduce((width, ch) => width + (/\p{Script=Han}/u.test(ch) ? 14 : 7.4), 0);
  const editing = {id: 1, index: 0, text, callout: {text, fontSize: 14}, rect,
    maxWidth: 280, uiScale: 1, fontSize: 14, color: "cherry", background: "transparent"};
  const h = input(text, {editing, measureText});
  const height = Math.ceil(layout.layoutTextLines(text, rect.width - 14, measureText).length * 14 * 1.25 + 14);
  assert.deepEqual(h.rects, [{...rect, height}]);
  h.textarea.selectionStart = 3; h.textarea.selectionEnd = 5; h.compose(true);
  h.render(text, {rect: {...rect, height}});
  assert.deepEqual(h.rects, [{...rect, height}], "the repaired frame must settle without a sizing loop");
  assert.equal(h.textarea.value, text); assert.deepEqual(h.writes, [text]);
  assert.equal(h.textarea.selectionStart, 3); assert.equal(h.textarea.selectionEnd, 5);
  assert.equal(composition.isTextComposition({target: h.textarea}), true);
});

test("the native editor keeps all repaired description lines inside the bottom edge", () => {
  const text = "Dev drag abcdef\n中文保存测试", rect = {x: 130, y: 320, width: 118, height: 30};
  const measureText = value => [...value].reduce((width, ch) => width + (/\p{Script=Han}/u.test(ch) ? 14 : 7.4), 0);
  const h = input(text, {measureText, editing: {id: 1, index: 0, text, callout: {text, fontSize: 14}, rect,
    maxWidth: 280, uiScale: 1, fontSize: 14, color: "cherry", background: "transparent"}});
  const height = Math.ceil(layout.layoutTextLines(text, rect.width - 14, measureText).length * 14 * 1.25 + 14);
  assert.deepEqual(h.rects, [{...rect, y: 360 - height, height}]);
  assert.equal(h.textarea.value, text); assert.deepEqual(h.writes, [text]);
});

test("native Undo returning to the original prop cannot leave stale echoes or rewrite the input", () => {
  const h = input("saved");
  h.textarea.type("x"); h.textarea.nativeUndo("saved"); h.render("saved");
  h.textarea.nativeUndo("savedx"); h.render("savedx");
  h.textarea.type("\nsecond line");
  for (const echo of ["saved", "savedx", "savedx\nsecond line"]) h.render(echo);
  assert.equal(h.textarea.value, "savedx\nsecond line");
  assert.deepEqual(h.changes, ["savedx", "saved", "savedx", "savedx\nsecond line"]);
  assert.deepEqual(h.writes, ["saved"]);
});

test("callout composition owns Enter/Escape/undo, then native history executes once", () => {
  const h = input();
  h.compose(true); h.textarea.type("zhong"); h.render("zhong");
  for (const options of [{key: "Enter"}, {key: "Escape"}, {key: "z", metaKey: true}]) {
    const event = h.key(options);
    assert.equal(event.defaultPrevented, false); assert.equal(event.stopped, true);
  }
  assert.equal(h.finishes(), 0); assert.deepEqual(h.commands, []);
  h.compose(false);
  assert.equal(h.key({key: "Escape", keyCode: 229}).defaultPrevented, false);
  assert.equal(h.key().defaultPrevented, false, "ordinary Return inserts a description newline");
  for (const modifier of ["metaKey", "ctrlKey"]) for (const shiftKey of [false, true]) {
    const event = h.key({key: "z", [modifier]: true, shiftKey});
    assert.equal(event.defaultPrevented, true); assert.equal(event.stopped, true);
  }
  assert.deepEqual(h.commands, ["undo", "redo", "undo", "redo"]);
  h.textarea.ownerDocument.execCommand = () => false;
  assert.equal(h.key({key: "z", ctrlKey: true}).defaultPrevented, false);
  h.compose(true); h.textarea.blur();
  assert.equal(composition.isTextComposition({target: h.textarea}), false);
  assert.equal(h.finishes(), 0);
  assert.equal(h.key({key: "Escape"}).defaultPrevented, true);
  assert.equal(h.finishes(), 1);
  assert.equal(h.key({key: "Enter", ctrlKey: true}).defaultPrevented, true);
  assert.equal(h.finishes(), 2);
});
