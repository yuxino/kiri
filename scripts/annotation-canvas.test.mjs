import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import ts from "typescript";
import { createLibraryHarness, nodes } from "./helpers/library-render-harness.mjs";
import * as project from "../src/annotation/project.js";
import * as crop from "../src/annotation/crop.js";
import * as layout from "../src/annotation/text-layout.js";
import * as calloutLayout from "../src/annotation/callout-layout.js";
import * as composition from "../src/annotation/text-composition.js";
import * as watermarkGeometry from "../src/annotation/watermark-geometry.js";

const dataUrl = source => `data:text/javascript;base64,${Buffer.from(source).toString("base64")}`;
const compile = source => ts.transpileModule(source, {compilerOptions: {
  module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022,
}}).outputText;
const geom = await import(dataUrl(compile(readFileSync(new URL("../src/annotation/geom.ts", import.meta.url), "utf8"))));
const geomUrl = dataUrl(compile(readFileSync(new URL("../src/annotation/geom.ts", import.meta.url), "utf8")));
const model = await import(dataUrl(compile(readFileSync(new URL("../src/annotation/model.ts", import.meta.url), "utf8")
  .replaceAll('"./geom"', JSON.stringify(geomUrl))
  .replaceAll('"./watermark-geometry.js"', JSON.stringify(new URL("../src/annotation/watermark-geometry.js", import.meta.url).href)))));
const source = readFileSync(new URL("../src/annotation/AnnotationCanvas.tsx", import.meta.url), "utf8");
const text = {kind: "text", id: 1, text: "first line\nsecond line", rect: {x: 40, y: 40, width: 180, height: 45},
  color: "white", background: "transparent", fontSize: 18};
const rectangle = {kind: "rectangle", id: 2, rect: {x: 100, y: 100, width: 100, height: 80}, color: "white", width: 3};
const appearance = model.DEFAULT_APPEARANCE;

const label = {...text, color:"cherry", labelDirection:"left", rect:{x:140,y:100,width:150,height:45}};
const dotOf = mark => model.labelGeometry(mark.rect, mark.fontSize, mark.labelDirection).dot;
function assertPoint(actual, expected) {
  assert.ok(Math.abs(actual.x - expected.x) < 1e-7, `x ${actual.x} must stay at ${expected.x}`);
  assert.ok(Math.abs(actual.y - expected.y) < 1e-7, `y ${actual.y} must stay at ${expected.y}`);
}
function dotButton(h) {
  const node = nodes(h.component.render()).find(node=>node?.type?.name==="LabelDot");
  assert.ok(node, "the visible label has an accessible dot control");
  return node.type(node.props);
}

test("a label flips around its dot through saved and inline controls without mirroring its text", async () => {
  const mark = {...label, rect: {...label.rect, x: 330}}, anchor = dotOf(mark);
  const h = annotation(documentWith([mark]), {selectedMarkId: mark.id});
  dotButton(h).props.onClick({stopPropagation() {}}); h.component.render();
  const flipped = h.changes.at(-1)[0];
  assertPoint(dotOf(flipped), anchor);
  assert.equal(flipped.labelDirection, "right"); assert.equal(flipped.text, mark.text);
  assert.ok(flipped.rect.x < anchor.x && mark.rect.x > anchor.x);
  h.ref.current.editSelectedText(); h.component.render();
  const editor = () => nodes(h.component.render()).find(node => node?.type?.name === "TextEditor");
  editor().props.onToggleDirection(); h.component.render();
  const result = await h.ref.current.exportResult();
  assertPoint(dotOf(result.document.marks[0]), anchor);
  assert.equal(result.document.marks[0].labelDirection, "left");
  assert.equal(result.document.marks[0].text, mark.text);
  const reopened = annotation(result.document, {selectedMarkId: mark.id});
  reopened.ref.current.updateSelectionAppearance({labelDirection: "right", textFontSize: 24}); reopened.component.render();
  assertPoint(dotOf((await reopened.ref.current.exportResult()).document.marks[0]), anchor);
});

test("an edge label wraps on its new side while its dot and saved text remain unchanged", async () => {
  const h = annotation(documentWith([label]), {selectedMarkId: label.id}), anchor = dotOf(label);
  for (const direction of ["right", "left", "right"]) {
    dotButton(h).props.onClick({stopPropagation() {}}); h.component.render();
    const mark = (await h.ref.current.exportResult()).document.marks[0];
    assert.equal(mark.labelDirection, direction); assert.equal(mark.text, label.text);
    assertPoint(dotOf(mark), anchor);
    const bounds = model.selectionBounds(mark);
    assert.ok(bounds.x >= -1e-7 && bounds.y >= -1e-7 && bounds.x + bounds.width <= 640 + 1e-7 && bounds.y + bounds.height <= 360 + 1e-7);
  }
});

test("opening a saved label keeps its anchor and layout, and inline wrapping grows around that same point", async () => {
  const h = annotation(documentWith([label]), {selectedMarkId: label.id}), anchor = dotOf(label);
  h.ref.current.editSelectedText(); h.component.render();
  const editor = () => nodes(h.component.render()).find(node => node?.type?.name === "TextEditor");
  const first = editor().props.editing;
  assertPoint(dotOf({...label, rect: {x: first.rect.x + 9, y: first.rect.y + 6,
    width: first.rect.width - 18, height: first.rect.height - 12}}), anchor);
  assert.deepEqual((await h.ref.current.exportResult()).document.marks, [label]);
  assert.deepEqual(h.changes, [], "opening and closing does not repair or move a saved label");
  h.ref.current.editSelectedText(); h.component.render();
  editor().props.onTextChange("中文\nlonger explanation"); h.component.render();
  editor().props.onRectChange({...first.rect, width: first.rect.width + 60, height: first.rect.height + 20}); h.component.render();
  const result = (await h.ref.current.exportResult()).document.marks[0];
  assertPoint(dotOf(result), anchor); assert.equal(result.text, "中文\nlonger explanation");
  assert.equal(h.changes.length, 1);
  h.ref.current.undo(); h.component.render(); assert.deepEqual(h.changes.at(-1), [label]);
});

test("clicking a label dot flips once around its point, starts no drag, and survives undo/reopen", async()=>{
  const h=annotation(documentWith([label]),{selectedMarkId:label.id});
  let stopped=0, prevented=0;
  const event={key:"Enter",stopPropagation(){stopped++;},preventDefault(){prevented++;}};
  const button=dotButton(h);
  button.props.onPointerDown(event); button.props.onKeyDown(event); button.props.onKeyUp(event);
  assert.equal(stopped,3); assert.equal(prevented,1);
  button.props.onClick(event); h.component.render();
  const flipped=h.changes.at(-1)[0];
  assertPoint(dotOf(flipped),dotOf(label)); assert.equal(flipped.labelDirection,"right");
  assert.equal(flipped.text,label.text);
  assert.equal(h.changes.length,1);
  const saved=await h.ref.current.exportResult();
  const reopened=annotation(saved.document);
  assert.deepEqual((await reopened.ref.current.exportResult()).document.marks,[flipped]);
  h.ref.current.undo();h.component.render();assert.deepEqual(h.changes.at(-1),[label]);
  h.ref.current.redo();h.component.render();assert.deepEqual(h.changes.at(-1),[flipped]);
});

test("label font changes remain a single edit and keep its fixed point and visible bubble in the canvas",()=>{
  const h=annotation(documentWith([label]),{selectedMarkId:label.id});
  h.ref.current.updateSelectionAppearance({textFontSize:64},true);
  h.ref.current.updateSelectionAppearance({textFontSize:32},true);
  h.ref.current.finishAppearanceAdjustment();h.component.render();
  assert.equal(h.changes.length,1);
  const resized=h.changes[0][0];
  const bounds=model.labelMovementBounds(resized);
  assert.ok(bounds.x>=0&&bounds.y>=0&&bounds.x+bounds.width<=640&&bounds.y+bounds.height<=360);
  assert.equal(resized.labelDirection,"left");
  assertPoint(dotOf(resized),dotOf(label));
  h.ref.current.undo();h.component.render();assert.deepEqual(h.changes.at(-1),[label]);
});

for (const tool of ["text", "label"]) {
  for (const mark of [text, label]) {
    for (const selected of [false, true]) {
      test(`${tool} tool directly drags ${mark.labelDirection ? "a label" : "plain text"} on the first ${selected ? "selected" : "unselected"} gesture`, async () => {
        const h = annotation(documentWith([mark]), {tool, selectedMarkId: selected ? mark.id : null});
        const start = {x: mark.rect.x + mark.rect.width / 2, y: mark.rect.y + mark.rect.height / 2};
        h.pointer("onPointerDown", start.x, start.y, {detail: 0});
        assert.equal(nodes(h.component.render()).some(node => node?.type?.name === "TextEditor"), false,
          "a pointer press must not open the native editor before deciding whether it is a drag");
        h.pointer("onPointerMove", start.x + 30, start.y + 20);
        const moved = model.translateMark(mark, {x: 30, y: 20}, {x: 0, y: 0, width: 640, height: 360});
        assert.deepEqual(h.frames.at(-1).marks, [moved]);
        assert.deepEqual(h.changes, [], "a live drag does not commit a document edit");
        h.pointer("onPointerCancel", start.x + 30, start.y + 20);
        assert.deepEqual(h.frames.at(-1).marks, [mark]);
        assert.deepEqual(h.changes, []);
        h.pointer("onPointerDown", start.x, start.y, {detail: 0});
        h.pointer("onPointerMove", start.x + 30, start.y + 20);
        h.pointer("onPointerUp", start.x + 30, start.y + 20);
        assert.equal(h.changes.length, 1);
        assert.deepEqual(h.changes[0], [moved]);
        h.ref.current.undo(); h.component.render(); assert.deepEqual(h.changes.at(-1), [mark]);
        h.ref.current.redo(); h.component.render(); assert.deepEqual(h.changes.at(-1), [moved]);
        assert.deepEqual((await h.ref.current.exportResult()).document.marks, [moved]);
        h.component.unmount();
      });
    }

    test(`${tool} tool selects ${mark.labelDirection ? "a label" : "plain text"} on a click and edits on a native double click`, async () => {
      let finishes = 0;
      const h = annotation(documentWith([mark]), {tool, onFinishOnBlankDoubleClick: () => finishes++});
      const point = {x: mark.rect.x + mark.rect.width / 2, y: mark.rect.y + mark.rect.height / 2};
      h.pointer("onPointerDown", point.x, point.y, {detail: 0});
      h.pointer("onPointerUp", point.x, point.y, {detail: 0});
      h.mouse("onClick", point.x, point.y, 1);
      assert.equal(nodes(h.component.render()).some(node => node?.type?.name === "TextEditor"), false);
      assert.deepEqual(h.changes, [], "selection is not a document edit");
      h.pointer("onPointerDown", point.x, point.y, {detail: 0});
      h.pointer("onPointerUp", point.x, point.y, {detail: 0});
      h.mouse("onClick", point.x, point.y, 2);
      h.mouse("onDoubleClick", point.x, point.y, 2);
      const editor = nodes(h.component.render()).find(node => node?.type?.name === "TextEditor");
      assert.equal(editor?.props.editing.id, mark.id);
      assert.equal(editor.props.editing.text, mark.text);
      assert.equal(finishes, 0);
      assert.deepEqual((await h.ref.current.exportResult()).document.marks, [mark]);
      h.ref.current.editSelectedText(); h.component.render();
      assert.equal(nodes(h.component.render()).find(node => node?.type?.name === "TextEditor")?.props.editing.id, mark.id,
        "the explicit Edit text action still reopens the selected text");
      h.component.unmount();
    });

    test(`${tool} tool resizes selected ${mark.labelDirection ? "label" : "text"} handles outside its body`, () => {
      const h = annotation(documentWith([mark]), {tool, selectedMarkId: mark.id});
      const bounds = model.selectionBounds(mark);
      const point = {x: bounds.x + bounds.width + 8, y: bounds.y + bounds.height};
      assert.equal(model.markIndexAt([mark], point), null, "the resize hit is outside the normal body target");
      h.pointer("onPointerDown", point.x, point.y);
      h.pointer("onPointerMove", point.x + 30, point.y + 20);
      assert.equal(nodes(h.component.render()).some(node => node?.type?.name === "TextEditor"), false);
      assert.equal(h.frames.at(-1).marks.length, 1);
      assert.notDeepEqual(h.frames.at(-1).marks[0], mark);
      h.pointer("onPointerUp", point.x + 30, point.y + 20);
      assert.equal(h.changes.length, 1);
      h.ref.current.undo(); h.component.render(); assert.deepEqual(h.changes.at(-1), [mark]);
      h.component.unmount();
    });
  }

  test(`${tool} tool creates on blank canvas and moves the newly committed text without replacing the old selection`, async () => {
    let finishes = 0;
    const h = annotation(documentWith([text]), {tool, selectedMarkId: text.id, onFinishOnBlankDoubleClick: () => finishes++});
    h.pointer("onPointerDown", 360, 180, {detail: 0});
    h.pointer("onPointerUp", 360, 180, {detail: 0});
    h.mouse("onDoubleClick", 360, 180, 2);
    let editor = nodes(h.component.render()).find(node => node?.type?.name === "TextEditor");
    assert.equal(editor.props.editing.index, null);
    assert.equal(editor.props.editing.labelDirection, tool === "label" ? appearance.labelDirection : undefined);
    editor.props.onTextChange("中文 label abc"); h.component.render();
    h.ref.current.commitTextEditing(); h.component.render();
    const [old, created] = (await h.ref.current.exportResult()).document.marks;
    assert.deepEqual(old, text); assert.notEqual(created.id, text.id); assert.equal(finishes, 0);
    const point = {x: created.rect.x + created.rect.width / 2, y: created.rect.y + created.rect.height / 2};
    h.pointer("onPointerDown", point.x, point.y, {detail: 0});
    h.pointer("onPointerMove", point.x + 20, point.y + 15);
    h.pointer("onPointerUp", point.x + 20, point.y + 15);
    const saved = (await h.ref.current.exportResult()).document;
    assert.deepEqual(saved.marks[0], text);
    assert.deepEqual(saved.marks[1], model.translateMark(created, {x: 20, y: 15}, {x: 0, y: 0, width: 640, height: 360}));
    const reopened = annotation(saved, {tool});
    const next = saved.marks[1], nextPoint = {x: next.rect.x + next.rect.width / 2, y: next.rect.y + next.rect.height / 2};
    reopened.pointer("onPointerDown", nextPoint.x, nextPoint.y, {detail: 0});
    reopened.pointer("onPointerMove", nextPoint.x + 10, nextPoint.y + 10);
    reopened.pointer("onPointerUp", nextPoint.x + 10, nextPoint.y + 10);
    assert.deepEqual(reopened.changes.at(-1)[1], model.translateMark(next, {x: 10, y: 10}, {x: 0, y: 0, width: 640, height: 360}));
    h.component.unmount(); reopened.component.unmount();
  });
}

test("dragging the direction dot never flips or moves its label, while click and keyboard activation still flip once", () => {
  const h = annotation(documentWith([label]), {tool: "label"});
  const button = dotButton(h), target = {setPointerCapture() {}};
  const event = (x, y, overrides = {}) => ({clientX: x, clientY: y, pointerId: 2, button: 0, detail: 1,
    currentTarget: target, stopPropagation() {}, preventDefault() {}, ...overrides});
  button.props.onPointerDown(event(100, 100));
  button.props.onPointerMove(event(108, 100));
  button.props.onPointerUp(event(100, 100));
  button.props.onClick(event(100, 100)); h.component.render();
  assert.deepEqual(h.changes, [], "returning to the press point must not turn a drag into a click");
  button.props.onPointerDown(event(100, 100));
  button.props.onPointerUp(event(104, 100));
  button.props.onClick(event(104, 100)); h.component.render();
  assert.deepEqual(h.changes, [], "the released position also suppresses flips when no move event arrived");
  button.props.onPointerDown(event(100, 100));
  button.props.onPointerCancel(event(100, 100));
  button.props.onClick(event(100, 100)); h.component.render();
  assert.deepEqual(h.changes, []);
  button.props.onPointerDown(event(100, 100));
  button.props.onPointerUp(event(101, 100));
  button.props.onClick(event(101, 100)); h.component.render();
  assert.equal(h.changes.length, 1); assertPoint(dotOf(h.changes.at(-1)[0]), dotOf(label));
  button.props.onPointerDown(event(100, 100));
  button.props.onPointerMove(event(110, 100));
  button.props.onPointerCancel(event(110, 100));
  button.props.onClick(event(0, 0, {detail: 0})); h.component.render();
  assert.equal(h.changes.length, 2, "keyboard activation is independent of a cancelled pointer gesture");
  assertPoint(dotOf(h.changes.at(-1)[0]), dotOf(label));
  assert.equal(h.changes.at(-1)[0].labelDirection, label.labelDirection);
  assert.equal(h.changes.at(-1)[0].text, label.text);
  h.component.unmount();
});

function annotation(initialDocument, options = {}) {
  const exports = [], frames = [], creations = [], ref = {current: null}, changes = [];
  function canvas() {
    const value = {width: 640, height: 360, drawCalls: [],
      getBoundingClientRect: () => options.boundingRect?.() ?? ({left: 0, top: 0, width: initialDocument.canvas.width, height: initialDocument.canvas.height}),
      setPointerCapture() {}, releasePointerCapture() {},
      toBlob(callback) { callback(new Blob([new Uint8Array([1, 2, 3])], {type: "image/png"})); },
    };
    const context = {font: "", setTransform() {}, save() {}, restore() {},
      measureText(text) { const size = Number(this.font.match(/ ([\d.]+)px/)?.[1] ?? 18); return {width: options.measureText?.(text, size) ?? text.length * size * .4}; },
      drawImage: (...args) => value.drawCalls.push(args)};
    value.getContext = () => context;
    creations.push(value);
    return value;
  }
  const live = canvas();
  const image = {complete: true, naturalWidth: initialDocument.sourcePixels.width, naturalHeight: initialDocument.sourcePixels.height};
  const harness = createLibraryHarness({}, source, {
    modules: {
      "./geom": geom, "./model": model, "./project.js": project, "./crop.js": crop,
      "./text-layout.js": layout, "./text-composition.js": composition,
      "./callout-layout.js": calloutLayout,
      "./watermark-geometry.js": watermarkGeometry,
      "./render": {textFont: size => `600 ${size}px sans-serif`, renderAll(r, marks, options) {
        if (!r.exporting) frames.push({marks: structuredClone(marks), options: structuredClone(options)});
        if (r.exporting) exports.push({source: r.sourceImage, sourceWidth: r.sourceWidth, sourceHeight: r.sourceHeight,
          sourceOffset: r.sourceOffset, regionSize: r.regionSize, scaleX: r.scaleX, scaleY: r.scaleY,
          marks: structuredClone(marks), canvasWidth: r.ctx.canvas?.width});
      }},
    },
    document: {createElement: type => { assert.equal(type, "canvas"); return canvas(); }},
    globals: {devicePixelRatio: 1},
    attachRef(node) { if (node.type === "canvas") node.props.ref.current = live; },
  });
  const props = {ref, image, region: {x: 0, y: 0, ...initialDocument.canvas}, initialDocument,
    appearance, tool: "select", onHistoryChange() {}, onCancel() {},
    onDocumentChange: marks => changes.push(structuredClone(marks)), ...options};
  const component = harness.mount("default", props);
  component.render();
  return {component, props, ref, changes, exports, frames, creations, live,
    pointer(name, x, y, event = {}) {
      const node = nodes(component.render()).find(node => node?.type === "canvas");
      node.props[name]({clientX: x, clientY: y, button: 0, pointerId: 1, detail: 1,
        currentTarget: live, preventDefault() {}, stopPropagation() {}, ...event});
      component.render();
    },
    mouse(name, x, y, detail) {
      const node = nodes(component.render()).find(node => node?.type === "canvas");
      node.props[name]({clientX: x, clientY: y, detail, nativeEvent: {clientX: x, clientY: y},
        preventDefault() {}, stopPropagation() {}});
      component.render();
    },
  };
}

const documentWith = marks => ({schemaVersion: 1, canvas: {width: 640, height: 360}, sourcePixels: {width: 640, height: 360}, marks});

const watermark = {kind: "watermark", id: 93, text: "中文 sample", rect: {x: 100, y: 100, width: 100, height: 80},
  color: "black", fontSize: 28, opacity: .2, rotation: -30, mode: "tiled", spacing: 80};
const watermarkEditor = h => nodes(h.component.render()).find(node => node?.props?.editing?.watermark);

test("the watermark entry opens native inline input, previews all styling and commits one editable mark", async () => {
  const selections = [];
  const h = annotation(documentWith([]), {onSelectionInfo: mark => selections.push(mark)});
  h.ref.current.editWatermark(); h.component.render({...h.props, tool: "watermark"});
  const input = watermarkEditor(h);
  assert.ok(input, "the tool-change effect must not close an editor opened by the entry button");
  assert.equal(input.props.editing.background, "transparent");
  assert.equal(input.props.onFinish, undefined, "Return commits the watermark without completing capture");
  const created = input.props.editing.watermark;
  assert.equal(created.rect.x + created.rect.width / 2, 320);
  assert.equal(created.rect.y + created.rect.height / 2, 180);
  assert.deepEqual(h.changes, [], "the empty draft has no undo entry");
  input.props.onTextChange("中文 input\nEnglish"); h.component.render();
  h.ref.current.updateSelectionAppearance({watermarkOpacity: 45, watermarkRotation: 15,
    watermarkMode: "single", watermarkSpacing: 100, watermarkColor: "white"}, true);
  h.component.render();
  const preview = h.frames.at(-1).options.draft;
  assert.equal(preview.kind, "watermark");
  assert.equal(preview.text, "中文 input\nEnglish");
  assert.equal(preview.opacity, .45); assert.equal(preview.rotation, 15);
  assert.equal(preview.mode, "tiled", "explicit watermark edits use the tiled-only workflow");
  assert.equal(preview.spacing, 100); assert.equal(preview.color, "white");
  assert.deepEqual(selections.at(-1), preview, "the HUD receives a watermark, not a plain-text surrogate");
  const saved = await h.ref.current.exportResult();
  assert.deepEqual(saved.document.marks, [preview]);
  assert.equal(h.changes.length, 1);
  const reopened = annotation(saved.document);
  reopened.ref.current.editWatermark(); reopened.component.render();
  assert.equal(watermarkEditor(reopened).props.editing.id, preview.id);
  assert.deepEqual((await reopened.ref.current.exportResult()).document.marks, [preview]);
  assert.deepEqual(reopened.changes, [], "an unchanged second edit is a no-op");
  h.ref.current.undo(); h.component.render(); assert.deepEqual(h.changes.at(-1), []);
  h.ref.current.redo(); h.component.render(); assert.deepEqual(h.changes.at(-1), [preview]);
});

test("watermark entry edits the last cropped-out anchor without changing its tile phase", async () => {
  const outside = {...watermark, id: 94, rect: {x: -450, y: -130, width: 120, height: 40}};
  const h = annotation(documentWith([watermark, outside]));
  h.ref.current.editWatermark(); h.component.render();
  const editor = watermarkEditor(h);
  assert.equal(editor.props.editing.id, outside.id);
  assert.ok(editor.props.editing.rect.x >= 0 && editor.props.editing.rect.y >= 0);
  assert.deepEqual(editor.props.editing.watermark.rect, outside.rect);
  assert.deepEqual((await h.ref.current.exportResult()).document.marks, [watermark, outside]);
  assert.deepEqual(h.changes, []);
  h.ref.current.editWatermark(); h.component.render();
  watermarkEditor(h).props.onTextChange("updated 中文"); h.component.render();
  const frame = watermarkEditor(h).props.editing.rect;
  watermarkEditor(h).props.onRectChange({...frame, width: 160, height: 60}); h.component.render();
  const updated = watermarkEditor(h).props.editing.watermark.rect;
  assert.equal(updated.x + updated.width / 2, outside.rect.x + outside.rect.width / 2);
  assert.equal(updated.y + updated.height / 2, outside.rect.y + outside.rect.height / 2);
  const saved = await h.ref.current.exportResult();
  assert.equal(saved.document.marks.at(-1).text, "updated 中文");
  assert.equal(saved.document.marks.at(-1).opacity, outside.opacity);
  assert.equal(saved.document.marks.at(-1).rotation, outside.rotation);
  h.ref.current.undo(); h.component.render(); assert.deepEqual(h.changes.at(-1), [watermark, outside]);
});

test("native double click edits an existing watermark and tiled copies do not intercept another annotation", () => {
  const h = annotation(documentWith([watermark, text]));
  h.pointer("onPointerDown", 150, 140, {detail: 0}); h.pointer("onPointerUp", 150, 140, {detail: 0});
  h.mouse("onDoubleClick", 150, 140, 2);
  assert.equal(watermarkEditor(h).props.editing.id, watermark.id);
  h.ref.current.cancelTextEditing(); h.component.render();
  assert.equal(model.markIndexAt([text, {...watermark, rect: {x: 500, y: 250, width: 100, height: 80}}],
    {x: 80, y: 60}), 0, "only the watermark's primary anchor participates in hit testing");
});

test("the watermark tool reopens saved text from any tiled copy instead of adding an empty mark", async () => {
  const h = annotation(documentWith([watermark]), {tool: "watermark"});
  // This point is far outside the editable primary anchor, as are most tiles.
  h.pointer("onPointerDown", 540, 300); h.pointer("onPointerUp", 540, 300);
  const input = watermarkEditor(h);
  assert.equal(input.props.editing.id, watermark.id);
  assert.equal(input.props.editing.text, watermark.text);
  input.props.onTextChange("二次编辑\nWatermark content"); h.component.render();
  h.ref.current.updateSelectionAppearance({watermarkOpacity: 47, watermarkSpacing: 120}, true); h.component.render();
  assert.equal(watermarkEditor(h).props.editing.text, "二次编辑\nWatermark content");
  watermarkEditor(h).props.onCommit(); h.component.render();
  assert.equal(watermarkEditor(h), undefined, "Return's commit callback closes only this editor");
  const saved = await h.ref.current.exportResult();
  assert.equal(saved.document.marks.length, 1);
  assert.equal(saved.document.marks[0].id, watermark.id);
  assert.equal(saved.document.marks[0].text, "二次编辑\nWatermark content");
  assert.equal(saved.document.marks[0].opacity, .47);
  const reopened = annotation(project.parseAnnotationDocument(JSON.parse(JSON.stringify(saved.document))), {tool: "watermark"});
  reopened.ref.current.editWatermark(); reopened.component.render();
  assert.equal(watermarkEditor(reopened).props.editing.text, saved.document.marks[0].text);
  assert.deepEqual((await reopened.ref.current.exportResult()).document.marks, saved.document.marks);
  h.component.unmount(); reopened.component.unmount();
});

test("repeated watermark actions retain the same native draft and explicitly selected watermark", async () => {
  const last = {...watermark, id: 94, text: "last watermark", rect: {...watermark.rect, x: 340}};
  const h = annotation(documentWith([watermark, last]), {selectedMarkId: watermark.id, tool: "watermark"});
  let focused = 0, mounted = false;
  h.live.parentElement = {querySelector: selector => {assert.equal(selector, "textarea"); return mounted ? {focus() {focused++;}} : null;}};
  h.ref.current.editWatermark(); h.component.render();
  mounted = true;
  assert.equal(watermarkEditor(h).props.editing.id, watermark.id, "the inspector edits its selected object before the last watermark");
  watermarkEditor(h).props.onTextChange("selected 中文 watermark"); h.component.render();
  for (let i = 0; i < 3; i++) {
    h.ref.current.editWatermark(); h.component.render();
    h.pointer("onPointerDown", 560, 300); h.pointer("onPointerUp", 560, 300);
    assert.equal(watermarkEditor(h).props.editing.id, watermark.id);
    assert.equal(watermarkEditor(h).props.editing.text, "selected 中文 watermark");
    assert.deepEqual(h.changes, [], "repeated entries neither commit the draft nor add empty marks");
  }
  assert.equal(focused, 6, "the already-mounted native input regains focus without remounting");
  const saved = await h.ref.current.exportResult();
  assert.equal(saved.document.marks.length, 2);
  assert.equal(saved.document.marks[0].text, "selected 中文 watermark");
  assert.deepEqual(saved.document.marks[1], last);
  h.component.unmount();

  const empty = annotation(documentWith([]), {appearance: {...appearance, watermarkMode: "single"}, tool: "watermark"});
  empty.ref.current.editWatermark(); empty.component.render();
  const id = watermarkEditor(empty).props.editing.id;
  for (let i = 0; i < 3; i++) {
    empty.ref.current.editWatermark(); empty.component.render();
    empty.pointer("onPointerDown", 520, 280); empty.pointer("onPointerUp", 520, 280);
    assert.equal(watermarkEditor(empty).props.editing.id, id);
    assert.equal(watermarkEditor(empty).props.editing.watermark.mode, "tiled");
  }
  assert.deepEqual((await empty.ref.current.exportResult()).document.marks, []);
  assert.deepEqual(empty.changes, []);
  empty.component.unmount();
});

test("a batched commit and reentry focuses the existing watermark input without clearing its text", async () => {
  const h = annotation(documentWith([watermark]), {tool: "watermark"});
  h.ref.current.editWatermark(); h.component.render();
  watermarkEditor(h).props.onTextChange("batched 中文 draft"); h.component.render();
  let focused = 0;
  h.live.parentElement = {querySelector: () => ({focus() {focused++;}})};
  // Mirror the old window-level tool callback, with no React render in between.
  h.ref.current.commitTextEditing(); h.ref.current.clearSelection(); h.ref.current.editWatermark(); h.component.render();
  assert.equal(focused, 1);
  assert.equal(watermarkEditor(h).props.editing.id, watermark.id);
  assert.equal(watermarkEditor(h).props.editing.text, "batched 中文 draft");
  assert.equal((await h.ref.current.exportResult()).document.marks.length, 1);
  h.component.unmount();
});

test("old single watermarks remain readable and unchanged until a real text or appearance edit", async () => {
  const legacy = {...watermark, mode: "single"};
  const h = annotation(documentWith([legacy]), {selectedMarkId: legacy.id});
  h.ref.current.updateSelectionAppearance({colorPreset: "white"}); h.component.render();
  assert.deepEqual((await h.ref.current.exportResult()).document.marks, [legacy]);
  h.ref.current.editWatermark(); h.component.render();
  h.ref.current.updateSelectionAppearance({textFontSize: 30}); h.component.render();
  assert.equal(watermarkEditor(h).props.editing.text, legacy.text);
  assert.deepEqual((await h.ref.current.exportResult()).document.marks, [legacy]);
  assert.deepEqual(h.changes, [], "opening and saving unchanged legacy data must not migrate it");
  h.ref.current.editWatermark(); h.component.render();
  watermarkEditor(h).props.onTextChange("modified single 中文"); h.component.render();
  assert.equal(h.frames.at(-1).marks[0].mode, "tiled");
  h.ref.current.cancelTextEditing(); h.component.render();
  assert.deepEqual((await h.ref.current.exportResult()).document.marks, [legacy]);
  h.ref.current.updateSelectionAppearance({watermarkOpacity: 35}, true); h.component.render();
  h.ref.current.finishAppearanceAdjustment(); h.component.render();
  const saved = (await h.ref.current.exportResult()).document.marks[0];
  assert.equal(saved.mode, "tiled"); assert.equal(saved.text, legacy.text); assert.equal(saved.opacity, .35);
  h.ref.current.undo(); h.component.render();
  assert.deepEqual((await h.ref.current.exportResult()).document.marks, [legacy]);
  h.ref.current.editWatermark(); h.component.render();
  watermarkEditor(h).props.onTextChange("modified single 中文"); h.component.render();
  watermarkEditor(h).props.onCommit(); h.component.render();
  assert.equal((await h.ref.current.exportResult()).document.marks[0].mode, "tiled");
  assert.equal((await h.ref.current.exportResult()).document.marks[0].text, "modified single 中文");
  h.component.unmount();
});

test("switching to another existing watermark after deleting a draft resolves its stable id", async () => {
  const last = {...watermark, id: 94, text: "retained watermark", rect: {...watermark.rect, x: 340}};
  const h = annotation(documentWith([watermark, last]), {selectedMarkId: watermark.id, tool: "watermark"});
  h.ref.current.editWatermark(); h.component.render();
  watermarkEditor(h).props.onTextChange(""); h.component.render();
  h.pointer("onPointerDown", 390, 140); h.pointer("onPointerUp", 390, 140);
  assert.equal(watermarkEditor(h).props.editing.id, last.id);
  assert.equal(watermarkEditor(h).props.editing.text, last.text);
  assert.deepEqual((await h.ref.current.exportResult()).document.marks, [last]);
  h.component.unmount();
});

test("dense watermark updates retain the valid saved and inline states and report the reason", async () => {
  const original = {...watermark, mode: "single", rotation: 0, spacing: 16, rect: {x: 100, y: 100, width: 2, height: 2}};
  const document = {...documentWith([original]), canvas: {width: 10000, height: 10000}, sourcePixels: {width: 10000, height: 10000}};
  const errors = [];
  const h = annotation(document, {selectedMarkId: original.id, onError: message => errors.push(message)});
  h.ref.current.updateSelectionAppearance({watermarkMode: "tiled"}, true); h.component.render();
  h.ref.current.finishAppearanceAdjustment(); h.component.render();
  assert.deepEqual(h.changes, []);
  assert.equal(errors.at(-1), "Watermark is too dense. Increase its size or spacing.");
  h.ref.current.updateSelectionAppearance({watermarkOpacity: 200, watermarkSpacing: 512}); h.component.render();
  assert.match(errors.at(-1), /opacity must be between 0 and 1/);
  assert.deepEqual(h.changes, [], "strict mark validation also rejects invalid API style values");
  h.ref.current.editWatermark(); h.component.render();
  h.ref.current.updateSelectionAppearance({watermarkMode: "tiled"}, true); h.component.render();
  assert.equal(watermarkEditor(h).props.editing.watermark.mode, "single");
  watermarkEditor(h).props.onTextChange("x".repeat(513)); h.component.render();
  assert.equal(watermarkEditor(h).props.editing.text, original.text);
  assert.equal(errors.at(-1), "Watermark text must be 512 characters or fewer.");
  assert.deepEqual((await h.ref.current.exportResult()).document.marks, [original]);
});

test("an empty or cancelled watermark draft creates no persisted mark", async () => {
  const h = annotation(documentWith([]));
  h.ref.current.editWatermark(); h.component.render();
  h.ref.current.cancelTextEditing(); h.component.render();
  assert.deepEqual((await h.ref.current.exportResult()).document.marks, []);
  h.ref.current.editWatermark(); h.component.render();
  assert.deepEqual((await h.ref.current.exportResult()).document.marks, []);
  assert.deepEqual(h.changes, []);
});

test("mosaic hover size redraws immediately and a stroke keeps its starting shape/style through cross-window preferences", async () => {
  const h = annotation(documentWith([]), {tool: "mosaic", mosaicShape: "brush"});
  h.pointer("onPointerMove", 50, 50);
  const count = h.frames.length;
  h.component.render({...h.props, appearance: {...appearance, mosaicBrushDiameter: 60}});
  assert.ok(h.frames.length > count);
  assert.equal(h.frames.at(-1).options.brushDiameter, 60);
  h.pointer("onPointerDown", 50, 50);
  h.component.render({...h.props, mosaicShape: "rectangle", appearance: {...appearance,
    mosaicBrushDiameter: 12, mosaicIntensity: "soft", mosaicStyle: "blur"}});
  h.pointer("onPointerMove", 100, 80); h.pointer("onPointerUp", 100, 80);
  assert.equal(h.frames.at(-1).options.brushCursor, null, "an area tool never retains a stale circular brush cursor");
  const mark = (await h.ref.current.exportResult()).document.marks[0];
  assert.equal(mark.shape, "brush"); assert.equal(mark.brushDiameter, 60);
  assert.equal(mark.style, "pixel"); assert.equal(mark.intensity, "standard");
  h.ref.current.undo(); h.component.render(); assert.deepEqual(h.changes.at(-1), []);
});

for (const shape of ["brush", "rectangle", "ellipse"]) {
  test(`${shape} mosaic creation, cancelled preview and selected styling preserve a single undo per gesture`, async () => {
    const h = annotation(documentWith([]), {tool: "mosaic", mosaicShape: shape});
    h.pointer("onPointerDown", 30, 40); h.pointer("onPointerMove", 100, 90);
    h.pointer("onPointerCancel", 100, 90);
    assert.equal(h.frames.at(-1).options.draft, null); assert.deepEqual(h.changes, []);
    h.pointer("onPointerDown", 30, 40); h.pointer("onPointerMove", 100, 90); h.pointer("onPointerUp", 100, 90);
    const created = h.changes.at(-1)[0];
    assert.equal(created.shape, shape); assert.equal(h.changes.length, 1);
    h.ref.current.updateSelectionAppearance({mosaicBrushDiameter: 50}, true);
    h.ref.current.updateSelectionAppearance({mosaicBrushDiameter: 40, mosaicIntensity: "strong", mosaicStyle: "blur"}, true);
    h.ref.current.finishAppearanceAdjustment(); h.component.render();
    assert.equal(h.changes.length, 2);
    const saved = await h.ref.current.exportResult();
    assert.equal(saved.document.marks[0].brushDiameter, 40);
    assert.equal(saved.document.marks[0].intensity, "strong");
    assert.equal(saved.document.marks[0].style, "blur");
    h.ref.current.undo(); h.component.render(); assert.deepEqual(h.changes.at(-1), [created]);
    h.ref.current.undo(); h.component.render(); assert.deepEqual(h.changes.at(-1), []);
  });
}

test("callout text edits directly on canvas, saves in the same mark and cancels without deleting the badge", async () => {
  const h = annotation(documentWith([]), {tool: "callout"});
  h.pointer("onPointerDown", 80, 90); h.pointer("onPointerUp", 80, 90);
  const editor = () => nodes(h.component.render()).find(node => node?.props?.editing?.callout);
  let input = editor();
  assert.ok(input, "creation opens the canvas editor");
  assert.ok(input.props.editing.rect.x - 80 - 18 >= 32);
  const placed = h.changes.at(-1)[0];
  input.props.onTextChange("中文\nabcdef"); h.component.render();
  assert.equal(h.frames.at(-1).marks[0].text, "中文\nabcdef", "preview includes the draft's badge and text geometry");
  const result = await h.ref.current.exportResult();
  assert.equal(result.document.marks[0].text, "中文\nabcdef");
  assert.equal(result.document.marks[0].id, placed.id);
  h.ref.current.editSelectedText(); h.component.render();
  editor().props.onTextChange("discarded draft"); h.component.render();
  h.ref.current.cancelTextEditing(); h.component.render();
  assert.equal(h.changes.at(-1)[0].text, "中文\nabcdef");
  h.ref.current.undo(); h.component.render();
  assert.deepEqual(h.changes.at(-1), [placed], "one canvas Undo restores the empty numbered badge");
});

const reflowCallout = {kind: "callout", id: 81, center: {x: 80, y: 80}, number: 1, text: "explanation",
  labelRect: {x: 200, y: 60, width: 100, height: 40}, size: 36, fontSize: 18, color: "cherry", style: "filled"};
const initialFrame = {left: 0, top: 0, width: 640, height: 360};
const inspectorFrame = {left: 80, top: 60, width: 480, height: 270};
const calloutEditor = h => nodes(h.component.render()).find(node => node?.props?.editing?.callout);

const devCalloutText = "Dev drag abcdef\n中文保存测试";
const devCalloutMeasure = value => [...value].reduce((width, character) => width + (/\p{Script=Han}/u.test(character) ? 14 : 7.4), 0);
const calloutLines = (mark, measureText) => layout.layoutTextLines(mark.text,
  mark.labelRect.width - 2 * Math.max(4, mark.fontSize * .5), measureText);

test("callout save and reopen tolerate fractional text metrics without an extra wrapped line", async () => {
  const doc = {schemaVersion: 1, canvas: {width: 1440, height: 900}, sourcePixels: {width: 1440, height: 900},
    marks: [{...reflowCallout, text: "", fontSize: 14, labelRect: {x: 200, y: 180, width: 160, height: 40}}]};
  const h = annotation(doc, {selectedMarkId: reflowCallout.id, viewSize: {width: 730, height: 456}, measureText: devCalloutMeasure});
  h.ref.current.editSelectedText(); h.component.render();
  h.ref.current.updateSelectedCallout({text: devCalloutText}); h.component.render();
  const saved = await h.ref.current.exportResult(), mark = saved.document.marks[0];
  const fractionalMetrics = value => devCalloutMeasure(value) + (value === "Dev drag abcdef" ? .02 : 0);
  assert.deepEqual(calloutLines(mark, fractionalMetrics), ["Dev drag abcdef", "中文保存测试"],
    "fitting exactly at the longest line cannot survive even fractional measurement differences");
  assert.ok(mark.labelRect.height >= calloutLines(mark, fractionalMetrics).length * 14 * 1.25 + 14);
  const reopened = annotation(saved.document, {selectedMarkId: mark.id,
    viewSize: {width: 730, height: 456}, measureText: fractionalMetrics});
  reopened.ref.current.editSelectedText(); reopened.component.render();
  assert.deepEqual(calloutEditor(reopened).props.editing.rect, mark.labelRect);
  assert.deepEqual((await reopened.ref.current.exportResult()).document.marks, [mark]);
  assert.deepEqual(reopened.changes, [], "reopening a fitting saved frame adds no invisible history entry");
});

test("editing a legacy short callout repairs only missing height, with cancellable and undoable persistence", async () => {
  const mark = {...reflowCallout, text: devCalloutText, fontSize: 14,
    labelRect: {x: 200, y: 180, width: 118, height: 49}};
  const measureText = value => devCalloutMeasure(value) + (value === "Dev drag abcdef" ? .02 : 0);
  const h = annotation(documentWith([mark]), {selectedMarkId: mark.id, measureText});
  const neededHeight = Math.ceil(calloutLines(mark, measureText).length * mark.fontSize * 1.25 + 14);
  assert.ok(neededHeight > mark.labelRect.height);
  h.ref.current.editSelectedText(); h.component.render();
  assert.deepEqual(calloutEditor(h).props.editing.rect, {...mark.labelRect, height: neededHeight});
  assert.deepEqual(h.changes, [], "repair remains an inline draft until committed");
  h.ref.current.cancelTextEditing(); h.component.render();
  assert.deepEqual((await h.ref.current.exportResult()).document.marks, [mark]);
  h.ref.current.editSelectedText(); h.component.render();
  const repaired = (await h.ref.current.exportResult()).document.marks[0];
  assert.deepEqual(repaired, {...mark, labelRect: {...mark.labelRect, height: neededHeight}});
  assert.equal(h.changes.length, 1);
  h.ref.current.undo(); h.component.render(); assert.deepEqual(h.changes.at(-1), [mark]);
  h.ref.current.redo(); h.component.render(); assert.deepEqual(h.changes.at(-1), [repaired]);
  const reopened = annotation(documentWith([repaired]), {selectedMarkId: mark.id, measureText});
  reopened.ref.current.editSelectedText(); reopened.component.render();
  assert.deepEqual((await reopened.ref.current.exportResult()).document.marks, [repaired]);
  assert.deepEqual(reopened.changes, []);
});

test("a native editor height repair retains the saved position even above its badge", async () => {
  const mark = {...reflowCallout, text: devCalloutText, fontSize: 14, center: {x: 230, y: 180},
    labelRect: {x: 200, y: 80, width: 118, height: 49}};
  const h = annotation(documentWith([mark]), {selectedMarkId: mark.id, measureText: devCalloutMeasure});
  h.ref.current.editSelectedText(); h.component.render();
  const frame = calloutEditor(h).props.editing.rect;
  calloutEditor(h).props.onRectChange({...frame, height: frame.height + 18}); h.component.render();
  assert.deepEqual(calloutEditor(h).props.editing.rect, {...frame, height: frame.height + 18},
    "repairing the saved height must not run the grow-away-from-badge placement algorithm");
  const result = (await h.ref.current.exportResult()).document.marks[0];
  assert.equal(result.labelRect.x, mark.labelRect.x); assert.equal(result.labelRect.y, mark.labelRect.y);
  assert.equal(result.labelRect.width, mark.labelRect.width);
});

test("a legacy callout height repair moves upward only as needed at the canvas bottom", async () => {
  const mark = {...reflowCallout, text: devCalloutText, fontSize: 14,
    labelRect: {x: 200, y: 320, width: 118, height: 30}};
  const h = annotation(documentWith([mark]), {selectedMarkId: mark.id, measureText: devCalloutMeasure});
  const height = Math.ceil(calloutLines(mark, devCalloutMeasure).length * 14 * 1.25 + 14);
  h.ref.current.editSelectedText(); h.component.render();
  const rect = {...mark.labelRect, y: 360 - height, height};
  assert.deepEqual(calloutEditor(h).props.editing.rect, rect);
  assert.equal(rect.y + rect.height, 360);
  h.ref.current.cancelTextEditing(); h.component.render();
  assert.deepEqual((await h.ref.current.exportResult()).document.marks, [mark]);
  h.ref.current.editSelectedText(); h.component.render();
  // The native textarea may measure a taller frame; this remains a height
  // repair, not a request to separate the description from its saved badge.
  calloutEditor(h).props.onRectChange({...rect, y: rect.y - 18, height: rect.height + 18}); h.component.render();
  assert.deepEqual(calloutEditor(h).props.editing.rect, {...rect, y: rect.y - 18, height: rect.height + 18});
  const saved = (await h.ref.current.exportResult()).document.marks[0];
  assert.equal(saved.labelRect.x, mark.labelRect.x); assert.equal(saved.labelRect.width, mark.labelRect.width);
  assert.ok(saved.labelRect.y + saved.labelRect.height <= 360);
  assert.ok(saved.labelRect.height >= height); assert.equal(h.changes.length, 1);
  h.ref.current.undo(); h.component.render(); assert.deepEqual(h.changes.at(-1), [mark]);
  h.ref.current.redo(); h.component.render(); assert.deepEqual(h.changes.at(-1), [saved]);
});

for (const part of ["badge", "description"]) {
  test(`the ${part} body drags directly without selecting a handle, with one undo`, () => {
    const h = annotation(documentWith([reflowCallout]));
    const start = part === "badge" ? reflowCallout.center : {x: 250, y: 80};
    h.pointer("onPointerDown", start.x, start.y); h.pointer("onPointerMove", start.x + 40, start.y + 30);
    h.pointer("onPointerUp", start.x + 40, start.y + 30);
    const moved = h.changes.at(-1)[0];
    if (part === "badge") {
      assert.deepEqual(moved.center, {x: start.x + 40, y: start.y + 30});
      assert.deepEqual(moved.labelRect, reflowCallout.labelRect);
    } else {
      assert.deepEqual(moved.center, reflowCallout.center);
      assert.deepEqual(moved.labelRect, {...reflowCallout.labelRect, x: 240, y: 90});
    }
    assert.equal(h.changes.length, 1);
    h.ref.current.undo(); h.component.render(); assert.deepEqual(h.changes.at(-1), [reflowCallout]);
    h.ref.current.redo(); h.component.render(); assert.deepEqual(h.changes.at(-1), [moved]);
  });
}

test("a callout click below the drag threshold edits; its connector still moves the whole callout", () => {
  const h = annotation(documentWith([reflowCallout]));
  h.pointer("onPointerDown", 250, 80); h.pointer("onPointerMove", 252, 81); h.pointer("onPointerUp", 252, 81);
  assert.deepEqual(h.changes, []); assert.equal(calloutEditor(h).props.editing.id, reflowCallout.id);
  h.ref.current.commitTextEditing(); h.component.render();
  h.pointer("onPointerDown", 140, 80); h.pointer("onPointerMove", 180, 110); h.pointer("onPointerUp", 180, 110);
  const moved = h.changes.at(-1)[0];
  assert.deepEqual(moved.center, {x: 120, y: 110});
  assert.deepEqual(moved.labelRect, {...reflowCallout.labelRect, x: 240, y: 90});
});

test("shared callout controls update the inline draft live and save one complete undoable edit", async () => {
  const selections = [];
  const h = annotation(documentWith([reflowCallout]), {selectedMarkId: reflowCallout.id,
    onSelectionInfo: mark => selections.push(structuredClone(mark))});
  h.ref.current.editSelectedText(); h.component.render();
  calloutEditor(h).props.onTextChange("中文\nabcdef"); h.component.render();
  const updates = [
    {calloutSize: 48, calloutStyle: "outline", textFontSize: 24, colorPreset: "white"},
    {calloutSize: 60, calloutStyle: "filled"},
    {calloutSize: 40, calloutStyle: "outline"},
  ];
  const live = [];
  for (const patch of updates) {
    h.ref.current.updateSelectionAppearance(patch, true); h.component.render();
    const editor = calloutEditor(h).props.editing, selection = selections.at(-1);
    assert.equal(editor.id, reflowCallout.id, "live properties keep the same native text editor");
    assert.equal(editor.text, "中文\nabcdef");
    assert.equal(selection.fontSize, 24); assert.equal(selection.color, "white");
    live.push([selection.size, selection.style]);
  }
  assert.deepEqual(live, [[48, "outline"], [60, "filled"], [40, "outline"]],
    "the HUD must receive each size/style preview rather than snapping back to the saved mark");
  h.ref.current.finishAppearanceAdjustment(); h.component.render();
  assert.deepEqual(h.changes, [], "inline changes remain one text draft until commit");
  const result = await h.ref.current.exportResult(); h.component.render();
  assert.equal(h.changes.length, 1);
  const saved = result.document.marks[0];
  assert.equal(saved.size, 40); assert.equal(saved.style, "outline");
  assert.equal(saved.fontSize, 24); assert.equal(saved.color, "white");
  assert.equal(saved.text, "中文\nabcdef"); assert.equal(saved.number, reflowCallout.number);
  assert.deepEqual((await annotation(result.document).ref.current.exportResult()).document.marks, [saved]);
  h.ref.current.undo(); h.component.render(); assert.deepEqual(h.changes.at(-1), [reflowCallout]);
  h.ref.current.redo(); h.component.render(); assert.deepEqual(h.changes.at(-1), [saved]);
});

function reflowClick(extraMarks = []) {
  let frame = initialFrame;
  const h = annotation(documentWith([reflowCallout, ...extraMarks]), {boundingRect: () => frame});
  const resize = next => {
    frame = next;
    h.component.render({...h.props, viewSize: {width: next.width, height: next.height}});
  };
  // PointerEvent.detail is deliberately zero: recovery must depend on the
  // native dblclick event, not a mouse click count on pointerdown.
  h.pointer("onPointerDown", 250, 80, {detail: 0});
  resize(inspectorFrame);
  h.pointer("onPointerUp", 250, 80, {detail: 0});
  h.mouse("onClick", 250, 80, 1);
  assert.equal(calloutEditor(h)?.props.editing.id, reflowCallout.id);
  return {...h, resize};
}

test("a native double click restores the same callout after its inspector reflows the stage", async () => {
  const h = reflowClick();
  h.pointer("onPointerDown", 250, 80, {detail: 0});
  // Clearing selection hides the inspector before dblclick is dispatched.
  h.resize(initialFrame);
  h.pointer("onPointerUp", 250, 80, {detail: 0});
  h.mouse("onClick", 250, 80, 2);
  assert.equal(calloutEditor(h), undefined, "the second canvas click initially commits the editor");
  h.mouse("onDoubleClick", 250, 80, 2);
  assert.equal(calloutEditor(h)?.props.editing.id, reflowCallout.id);
  assert.deepEqual(h.changes, [], "recovery does not create an undo step or change saved placement");
  assert.deepEqual((await h.ref.current.exportResult()).document.marks, [reflowCallout]);
});

test("an ordinary single click on reflowed blank canvas still commits the callout draft", async () => {
  const h = reflowClick();
  calloutEditor(h).props.onTextChange("updated explanation"); h.component.render();
  h.pointer("onPointerDown", 250, 80, {detail: 0});
  h.resize(initialFrame);
  h.pointer("onPointerUp", 250, 80, {detail: 0});
  h.mouse("onClick", 250, 80, 1);
  assert.equal(calloutEditor(h), undefined);
  assert.equal(h.changes.at(-1)[0].text, "updated explanation");
  assert.equal(h.changes.length, 1);
  assert.equal((await h.ref.current.exportResult()).document.marks[0].text, "updated explanation");
});

test("double clicking another mark after reflow edits that mark without restoring the old callout", () => {
  const other = {...text, id: 82, text: "another mark", rect: {x: 210, y: 15, width: 100, height: 35}};
  const h = reflowClick([other]);
  h.pointer("onPointerDown", 250, 80, {detail: 0});
  h.resize(initialFrame);
  h.pointer("onPointerUp", 250, 80, {detail: 0});
  h.mouse("onClick", 250, 80, 2);
  h.mouse("onDoubleClick", 250, 80, 2);
  const editor = nodes(h.component.render()).find(node => node?.type?.name === "TextEditor");
  assert.equal(editor?.props.editing.id, other.id);
  assert.equal(editor.props.editing.callout, undefined);
  assert.deepEqual(h.changes, []);
});

for (const mark of [
  {...text, id: 83, text: "plain text", rect: {x: 200, y: 60, width: 100, height: 40}},
  {...text, id: 84, text: "direction label", labelDirection: "left", rect: {x: 200, y: 60, width: 100, height: 40}},
]) {
  test(`${mark.labelDirection ? "direction label" : "plain text"} first double click retains live-frame editing after inspector reflow`, async () => {
    let frame = initialFrame;
    const selections = [];
    const h = annotation(documentWith([mark]), {boundingRect: () => frame,
      onSelectionInfo: selected => selections.push(selected?.id ?? null)});
    const resize = next => {
      frame = next;
      h.component.render({...h.props, viewSize: {width: next.width, height: next.height}});
    };
    h.pointer("onPointerDown", 250, 80, {detail: 0});
    assert.equal(selections.at(-1), mark.id);
    resize(inspectorFrame);
    h.pointer("onPointerUp", 250, 80, {detail: 0});
    h.mouse("onClick", 250, 80, 1);
    h.pointer("onPointerDown", 250, 80, {detail: 0});
    assert.equal(selections.at(-1), null, "the second click misses the mark in the inspector frame");
    resize(initialFrame);
    h.pointer("onPointerUp", 250, 80, {detail: 0});
    h.mouse("onClick", 250, 80, 2);
    h.mouse("onDoubleClick", 250, 80, 2);
    const editor = nodes(h.component.render()).find(node => node?.type?.name === "TextEditor");
    assert.equal(editor?.props.editing.id, mark.id);
    assert.equal(editor.props.editing.callout, undefined);
    assert.equal(editor.props.editing.labelDirection, mark.labelDirection);
    assert.deepEqual(h.changes, []);
    assert.deepEqual((await h.ref.current.exportResult()).document.marks, [mark]);
  });
}

test("a dragged second click never restores the previous callout after stage reflow", () => {
  const h = reflowClick();
  h.pointer("onPointerDown", 250, 80, {detail: 0});
  h.resize(initialFrame);
  h.pointer("onPointerMove", 270, 90, {detail: 0});
  h.pointer("onPointerUp", 270, 90, {detail: 0});
  h.mouse("onDoubleClick", 250, 80, 2);
  assert.equal(calloutEditor(h), undefined);
  assert.deepEqual(h.changes, []);
});

test("double clicking blank canvas without a stage reflow never reopens a callout", () => {
  const h = annotation(documentWith([reflowCallout]));
  h.pointer("onPointerDown", 250, 80, {detail: 0}); h.pointer("onPointerUp", 250, 80, {detail: 0});
  h.mouse("onClick", 250, 80, 1);
  assert.ok(calloutEditor(h));
  h.pointer("onPointerDown", 310, 80, {detail: 0}); h.pointer("onPointerUp", 310, 80, {detail: 0});
  h.mouse("onClick", 310, 80, 2); h.mouse("onDoubleClick", 310, 80, 2);
  assert.equal(calloutEditor(h), undefined);
  assert.deepEqual(h.changes, []);
});

test("saved callout placement survives reopening and text growth stays away from its number", async () => {
  const mark = {kind: "callout", id: 1, center: {x: 580, y: 90}, number: 1, text: "saved",
    labelRect: {x: 370, y: 70, width: 160, height: 40}, size: 36, fontSize: 18, color: "cherry", style: "filled"};
  const h = annotation(documentWith([mark]), {selectedMarkId: 1});
  h.ref.current.editSelectedText(); h.component.render();
  let editor = nodes(h.component.render()).find(node => node?.props?.editing?.callout);
  assert.deepEqual(editor.props.editing.rect, mark.labelRect);
  assert.deepEqual((await h.ref.current.exportResult()).document.marks, [mark]);
  h.ref.current.editSelectedText(); h.component.render();
  editor = nodes(h.component.render()).find(node => node?.props?.editing?.callout);
  editor.props.onTextChange("a longer explanation"); h.component.render();
  editor.props.onRectChange({...mark.labelRect, width: 280}); h.component.render();
  editor = nodes(h.component.render()).find(node => node?.props?.editing?.callout);
  assert.equal(editor.props.editing.rect.x, 250);
  assert.equal(editor.props.editing.rect.x + editor.props.editing.rect.width, 530);
  h.ref.current.cancelTextEditing(); h.component.render();
  assert.deepEqual((await h.ref.current.exportResult()).document.marks, [mark]);
});

test("a newly created inline callout frame moves only its label and preserves typed text", () => {
  const h = annotation(documentWith([]), {tool: "callout"});
  h.pointer("onPointerDown", 80, 90); h.pointer("onPointerUp", 80, 90);
  const editor = nodes(h.component.render()).find(node => node?.props?.editing?.callout);
  editor.props.onTextChange("explanation"); h.component.render();
  const frame = editor.props.editing.rect;
  editor.props.onMoveCallout({clientX: frame.x + frame.width + 4, clientY: frame.y + 3, pointerId: 1, preventDefault() {}, stopPropagation() {}},
    {x: frame.x + frame.width, y: frame.y});
  h.component.render();
  const before = h.changes.at(-1)[0];
  assert.deepEqual(h.frames.at(-1).marks[0].labelRect, {...before.labelRect,
    x: before.labelRect.x + 4, y: before.labelRect.y + 3}, "the threshold-crossing movement is not dropped");
  h.pointer("onPointerMove", frame.x + frame.width + 40, frame.y + 30);
  h.pointer("onPointerUp", frame.x + frame.width + 40, frame.y + 30);
  const after = h.changes.at(-1)[0];
  assert.deepEqual(after.center, before.center);
  assert.equal(after.text, "explanation");
  assert.deepEqual(after.labelRect, {...before.labelRect, x: before.labelRect.x + 40, y: before.labelRect.y + 30});
  h.ref.current.undo(); h.component.render(); assert.deepEqual(h.changes.at(-1)[0], before);
});

test("numbered notes create on click or drag and retain descriptions through export and reopen", async () => {
  const h = annotation(documentWith([]), {tool: "callout", calloutNumber: 7});
  h.pointer("onPointerDown", 80, 90); h.pointer("onPointerUp", 80, 90);
  const placed = h.changes.at(-1)[0];
  assert.equal(placed.kind, "callout"); assert.equal(placed.number, 7);
  assert.equal(placed.style, "filled"); assert.equal(model.nextCalloutNumber([placed]), 8);
  h.ref.current.updateSelectedCallout({text: "标题\nA short explanation", number: 12}, true);
  h.component.render();
  h.ref.current.updateSelectedCallout({size: 48, style: "outline"}, true);
  h.component.render();
  h.ref.current.finishAppearanceAdjustment(); h.component.render();
  h.ref.current.commitTextEditing(); h.component.render();
  const saved = h.changes.at(-1)[0];
  assert.equal(saved.text, "标题\nA short explanation"); assert.equal(saved.number, 12);
  assert.ok(saved.labelRect.height > saved.fontSize * 2);
  const exported = await h.ref.current.exportResult();
  assert.deepEqual(exported.document.marks, [saved]);
  const reopened = annotation(exported.document);
  assert.deepEqual(reopened.frames.at(-1).marks, [saved]);
  h.ref.current.undo(); h.component.render();
  assert.deepEqual(h.changes.at(-1), [placed], "one undo restores the complete inspector edit");
  h.ref.current.redo(); h.component.render();
  assert.deepEqual(h.changes.at(-1), [saved]);
  h.component.render({...h.props, calloutNumber: 13});
  h.pointer("onPointerDown", 90, 260); h.pointer("onPointerUp", 350, 280);
  assert.equal(h.changes.at(-1)[1].number, 13);
  assert.ok(h.changes.at(-1)[1].labelRect.x > 300);
});

test("number and description handles move independently, stay in bounds, and undo exactly", () => {
  const mark = {kind: "callout", id: 1, center: {x: 80, y: 80}, number: 1, text: "説明",
    labelRect: {x: 200, y: 60, width: 100, height: 40}, size: 36, fontSize: 18, color: "cherry", style: "filled"};
  const h = annotation(documentWith([mark]), {selectedMarkId: mark.id});
  h.pointer("onPointerDown", 80, 62); h.pointer("onPointerMove", 120, 102); h.pointer("onPointerUp", 120, 102);
  assert.deepEqual(h.changes.at(-1)[0].labelRect, mark.labelRect);
  assert.deepEqual(h.changes.at(-1)[0].center, {x: 120, y: 120});
  h.ref.current.undo(); h.component.render();
  h.pointer("onPointerDown", 250, 80); h.pointer("onPointerMove", 620, 350); h.pointer("onPointerUp", 620, 350);
  // Undo clears selection; dragging the description body still moves it independently.
  const current = h.changes.at(-1)[0];
  assert.deepEqual(current.center,mark.center);
  assert.ok(current.labelRect.x + current.labelRect.width <= 640);
  assert.ok(current.labelRect.y + current.labelRect.height <= 360);
  h.ref.current.undo(); h.component.render();
  h.pointer("onPointerDown", 250, 80); h.pointer("onPointerUp", 250, 80);
  h.pointer("onPointerDown", 300, 60); h.pointer("onPointerUp", 340, 90);
  const movedLabel = h.changes.at(-1)[0];
  assert.deepEqual(movedLabel.center, mark.center);
  assert.deepEqual(movedLabel.labelRect, {...mark.labelRect, x: 240, y: 90});
  h.ref.current.undo(); h.component.render();
  assert.deepEqual(h.changes.at(-1), [mark]);
});

test("opening the callout inspector during selection cannot move a mark or distort a drag", () => {
  const mark = {kind: "callout", id: 1, center: {x: 80, y: 80}, number: 1, text: "説明",
    labelRect: {x: 200, y: 60, width: 100, height: 40}, size: 36, fontSize: 18, color: "cherry", style: "filled"};
  let rect = {left: 0, top: 0, width: 640, height: 360};
  const h = annotation(documentWith([mark]), {boundingRect: () => rect});
  h.pointer("onPointerDown", 80, 80);
  rect = {left: 80, top: 100, width: 480, height: 270};
  h.pointer("onPointerMove", 80, 80); h.pointer("onPointerUp", 80, 80);
  assert.deepEqual(h.changes, [], "a stationary click stays a selection, with no undo entry");
  assert.deepEqual(h.frames.at(-1).marks, [mark]);
  rect = {left: 0, top: 0, width: 640, height: 360};
  h.pointer("onPointerDown", 80, 80);
  rect = {left: 80, top: 100, width: 480, height: 270};
  h.pointer("onPointerMove", 100, 90); h.pointer("onPointerUp", 100, 90);
  assert.deepEqual(h.changes.at(-1)[0].center, {x: 100, y: 90});
  assert.deepEqual(h.changes.at(-1)[0].labelRect, mark.labelRect);
});

test("keyboard-style live font changes update the selected mark and commit one undoable edit", () => {
  const h = annotation(documentWith([text]), {selectedMarkId: text.id});
  h.ref.current.setTextFontSizeLive(32); // Keyboard input has no pointerdown.
  h.component.render();
  h.ref.current.setTextFontSizeLive(48);
  h.component.render();
  h.ref.current.endTextFontSizeAdjustment();
  h.component.render();
  assert.equal(h.changes.length, 1);
  const larger = h.changes[0][0];
  assert.equal(larger.fontSize, 48);
  assert.equal(larger.rect.width, text.rect.width * 48 / 18);
  assert.equal(larger.rect.height, text.rect.height * 48 / 18);
  h.ref.current.undo();
  h.component.render();
  assert.deepEqual(h.changes.at(-1), [text]);
});

test("font changes repair a previously saved short text box using explicit and wrapped lines", () => {
  const legacy = {...text, text: "long words wrap here\nsecond paragraph", fontSize: 48,
    rect: {x: 40, y: 40, width: 180, height: 45}};
  const h = annotation(documentWith([legacy]), {selectedMarkId: legacy.id});
  h.ref.current.setTextFontSizeLive(36);
  h.component.render();
  h.ref.current.endTextFontSizeAdjustment();
  h.component.render();
  const repaired = h.changes.at(-1)[0];
  const lines = layout.layoutTextLines(repaired.text, repaired.rect.width, value => value.length * 36 * .4);
  assert.ok(lines.length > 2, "the two paragraphs also need automatic wrapping");
  assert.equal(repaired.rect.height, Math.ceil(lines.length * 36 * 1.25));
  assert.ok(repaired.rect.height > legacy.rect.height);
  const lastLine = {x: repaired.rect.x + 10, y: repaired.rect.y + (lines.length - 1) * 36 * 1.25 + 10};
  assert.equal(model.markIndexAt([repaired], lastLine), 0);
  h.ref.current.undo();
  h.component.render();
  assert.deepEqual(h.changes.at(-1), [legacy]);
});

test("repairing bounds at the same font size is still one undoable change", () => {
  const legacy = {...text, rect: {...text.rect, height: 10}};
  const h = annotation(documentWith([legacy]), {selectedMarkId: legacy.id});
  h.ref.current.setTextFontSizeLive(18);
  h.ref.current.endTextFontSizeAdjustment();
  h.component.render();
  assert.equal(h.changes.at(-1)[0].rect.height, 45);
  h.ref.current.undo();
  h.component.render();
  assert.deepEqual(h.changes.at(-1), [legacy]);
});

test("clicking an off-center resize handle does not insert an invisible undo step", () => {
  const h = annotation(documentWith([rectangle]), {selectedMarkId: rectangle.id});
  h.pointer("onPointerDown", 140, 140);
  h.pointer("onPointerMove", 170, 150);
  h.pointer("onPointerUp", 170, 150);
  const moved = h.changes.at(-1)[0];
  assert.equal(moved.rect.x, 130);
  assert.equal(moved.rect.y, 110);
  h.pointer("onPointerDown", 183, 114); // Inside the 9px handle target, off its center.
  h.pointer("onPointerUp", 183, 114);
  assert.equal(h.changes.length, 1);
  h.ref.current.undo();
  h.component.render();
  assert.deepEqual(h.changes.at(-1), [rectangle]);
});

test("a cropped mosaic is exported using the same source bounds and document as reopening", async () => {
  const mosaic = {kind: "mosaic", id: 3, points: [{x: 200, y: 180}, {x: 460, y: 180}],
    brushDiameter: 20, intensity: "standard", style: "pixel"};
  const h = annotation(documentWith([mosaic]));
  const first = await h.ref.current.exportResult({x: 250, y: 0, width: 390, height: 360});
  assert.deepEqual(first.cropPixels, {x: 250, y: 0, width: 390, height: 360});
  assert.deepEqual(first.document.marks[0].points, [{x: -50, y: 180}, {x: 210, y: 180}]);
  const firstRender = h.exports[0];
  assert.equal(firstRender.sourceWidth, 390);
  assert.equal(firstRender.sourceHeight, 360);
  assert.deepEqual(firstRender.sourceOffset, {x: 0, y: 0});
  assert.deepEqual(firstRender.source.drawCalls[0].slice(1), [250, 0, 390, 360, 0, 0, 390, 360]);
  const reopened = annotation(first.document);
  const second = await reopened.ref.current.exportResult();
  assert.equal(second.cropPixels, null);
  assert.deepEqual(second.document, first.document);
  const reopenedRender = reopened.exports[0];
  for (const key of ["sourceWidth", "sourceHeight", "sourceOffset", "regionSize", "scaleX", "scaleY", "marks"]) {
    assert.deepEqual(reopenedRender[key], firstRender[key], key);
  }
});


for (const handle of ["right", "left", "top", "bottom", "topLeft", "topRight", "bottomLeft", "bottomRight"]) {
  test(`text ${handle} resize remeasures wrapped preview and saves one fully hittable edit`, async () => {
    const original = {...text, text: "ALPHA BETA GAMMA DELTA\nONE TWO THREE FOUR FIVE", fontSize: 14,
      rect: {x: 180, y: 110, width: 130, height: 35}};
    // Real font hinting is not perfectly proportional. Simulate the smaller
    // font crossing a wrap threshold to exercise the native regression.
    const measureText = (value, size) => value.length * size * (size < 14 ? .6 : .4);
    const h = annotation(documentWith([original]), {selectedMarkId: original.id, measureText});
    const start = geom.handlePoint(handle, original.rect);
    const end = {x: start.x + (handle.toLowerCase().includes("left") ? 40 : -40),
      y: start.y + (handle.startsWith("top") ? 12 : -12)};
    h.pointer("onPointerDown", start.x, start.y);
    h.pointer("onPointerMove", end.x, end.y);
    assert.equal(h.changes.length, 0, "preview must not commit history");
    const preview = h.frames.at(-1).marks[0];
    h.pointer("onPointerUp", end.x, end.y);
    assert.equal(h.changes.length, 1);
    const resized = h.changes[0][0];
    const lines = layout.layoutTextLines(resized.text, resized.rect.width,
      value => measureText(value, resized.fontSize));
    assert.ok(lines.length > 2, "the simulated font needs additional wrapped lines");
    assert.equal(resized.rect.height, Math.ceil(lines.length * resized.fontSize * 1.25));
    assert.deepEqual(preview, resized, "drag preview and committed bounds agree");
    const close = (actual, expected) => assert.ok(Math.abs(actual - expected) < 1e-8);
    if (handle.startsWith("top")) close(resized.rect.y + resized.rect.height, original.rect.y + original.rect.height);
    else if (handle.startsWith("bottom")) close(resized.rect.y, original.rect.y);
    else close(resized.rect.y + resized.rect.height / 2, original.rect.y + original.rect.height / 2);
    const lastLine = {x: resized.rect.x + 10,
      y: resized.rect.y + (lines.length - .5) * resized.fontSize * 1.25};
    assert.equal(model.markIndexAt([resized], lastLine), 0);
    const saved = await h.ref.current.exportResult();
    const reopened = annotation(saved.document, {measureText});
    const savedAgain = await reopened.ref.current.exportResult();
    assert.deepEqual(savedAgain.document.marks, [resized]);
    assert.equal(model.markIndexAt(savedAgain.document.marks, lastLine), 0);
    h.ref.current.undo(); h.component.render();
    assert.deepEqual(h.changes.at(-1), [original]);
    h.ref.current.redo(); h.component.render();
    assert.deepEqual(h.changes.at(-1), [resized]);
  });
}

test("a text handle click without motion does not repair bounds or create history", () => {
  const legacy = {...text, rect: {...text.rect, height: 10}};
  const h = annotation(documentWith([legacy]), {selectedMarkId: legacy.id});
  const point = geom.handlePoint("right", legacy.rect);
  h.pointer("onPointerDown", point.x, point.y);
  h.pointer("onPointerMove", point.x + .2, point.y);
  assert.deepEqual(h.frames.at(-1).marks[0], legacy);
  h.pointer("onPointerUp", point.x + .2, point.y);
  assert.equal(h.changes.length, 0);
});


test("resizing an older malformed saved text box repairs all wrapped-line hit bounds", () => {
  const legacy = {...text, text: "ALPHA BETA GAMMA DELTA\nONE TWO THREE FOUR FIVE", fontSize: 14,
    rect: {x: 180, y: 110, width: 90, height: 35}};
  const h = annotation(documentWith([legacy]), {selectedMarkId: legacy.id});
  const point = geom.handlePoint("right", legacy.rect);
  h.pointer("onPointerDown", point.x, point.y);
  h.pointer("onPointerUp", point.x + 20, point.y);
  const repaired = h.changes.at(-1)[0];
  const lines = layout.layoutTextLines(repaired.text, repaired.rect.width,
    value => value.length * repaired.fontSize * .4);
  assert.ok(lines.length > 2);
  assert.equal(repaired.rect.height, Math.ceil(lines.length * repaired.fontSize * 1.25));
  assert.equal(model.markIndexAt([repaired], {x: repaired.rect.x + 5,
    y: repaired.rect.y + (lines.length - .5) * repaired.fontSize * 1.25}), 0);
});


for (const sample of [
  {name: "full-height side", handle: "right", fontSize: 120, rect: {x: 180, y: 30, width: 160, height: 300}, dx: -16, dy: 0},
  {name: "near-top anchored", handle: "top", fontSize: 14, rect: {x: 180, y: 5, width: 20, height: 35}, dx: 0, dy: 4},
  {name: "near-bottom anchored", handle: "bottom", fontSize: 14, rect: {x: 180, y: 320, width: 20, height: 35}, dx: 0, dy: -4},
]) {
  test(`${sample.name} text resize fits measured lines without moving its anchor`, () => {
    const original = {...text, text: "A B\nC D", fontSize: sample.fontSize, rect: sample.rect};
    const measureText = (value, size) => value.length * size * (size < sample.fontSize ? .6 : .4);
    const h = annotation(documentWith([original]), {selectedMarkId: original.id, measureText});
    const start = geom.handlePoint(sample.handle, original.rect);
    h.pointer("onPointerDown", start.x, start.y);
    h.pointer("onPointerMove", start.x + sample.dx, start.y + sample.dy);
    const preview = h.frames.at(-1).marks[0];
    h.pointer("onPointerUp", start.x + sample.dx, start.y + sample.dy);
    const resized = h.changes.at(-1)[0];
    assert.deepEqual(preview, resized);
    const lines = layout.layoutTextLines(resized.text, resized.rect.width,
      value => measureText(value, resized.fontSize));
    assert.equal(resized.rect.height, Math.ceil(lines.length * resized.fontSize * 1.25));
    assert.ok(resized.rect.y >= 0);
    assert.ok(resized.rect.y + resized.rect.height <= 360);
    const actual = sample.handle === "top" ? resized.rect.y + resized.rect.height :
      sample.handle === "bottom" ? resized.rect.y : resized.rect.y + resized.rect.height / 2;
    const expected = sample.handle === "top" ? original.rect.y + original.rect.height :
      sample.handle === "bottom" ? original.rect.y : original.rect.y + original.rect.height / 2;
    assert.ok(Math.abs(actual - expected) < 1e-8);
  });
}

const movableMarks = [
  watermark,
  {...watermark, mode: "single"},
  rectangle,
  label,
  {...text, rect: {x: 100, y: 100, width: 100, height: 80}},
  {kind: "pen", id: 3, points: [{x:100,y:100},{x:200,y:180}], color:"white", width:3},
  {kind: "line", id: 4, start:{x:100,y:100}, end:{x:200,y:180}, color:"white", width:3},
  {kind: "arrow", id: 5, start:{x:100,y:100}, end:{x:200,y:180}, color:"white", width:3},
  ...["brush", "rectangle", "ellipse"].map((shape,index) => ({kind:"mosaic",id:6+index,
    points:[{x:100,y:100},{x:200,y:180}],shape,brushDiameter:20,intensity:"standard",style:"pixel"})),
];

for (const mark of movableMarks) {
  test(`${mark.kind} ${mark.shape ?? ""} moves as one live mark, preserving layers, cancel and undo`, () => {
    const under = {...rectangle, id:90, rect:{x:400,y:100,width:30,height:30}};
    const over = {...rectangle, id:91, rect:{x:450,y:100,width:30,height:30}};
    const liveFrames = [];
    const h = annotation(documentWith([under,mark,over]), {selectedMarkId:mark.id,
      onLiveMarks: (marks,draft) => liveFrames.push({marks:structuredClone(marks),draft})});
    h.pointer("onPointerDown",150,140);
    h.pointer("onPointerMove",180,160);
    const frame = h.frames.at(-1);
    const moved = model.translateMark(mark,{x:30,y:20},{x:0,y:0,width:640,height:360});
    assert.deepEqual(frame.marks,[under,moved,over],"replace the old mark in its layer before releasing the pointer");
    assert.equal(frame.options.draft,null,"never draw a duplicate above the original");
    assert.equal(frame.options.selectedIndex,1,"handles follow the replacement");
    assert.deepEqual(liveFrames.at(-1),{marks:[under,moved,over],draft:null},"video uses the same live geometry");
    assert.equal(h.changes.length,0,"preview does not mutate the saved document");
    h.pointer("onPointerCancel",180,160);
    assert.deepEqual(h.frames.at(-1).marks,[under,mark,over]);
    assert.equal(h.changes.length,0);
    h.pointer("onPointerDown",150,140);
    h.pointer("onPointerMove",180,160);
    h.pointer("onPointerUp",180,160);
    assert.deepEqual(h.changes.at(-1),[under,moved,over]);
    h.ref.current.undo(); h.component.render();
    assert.deepEqual(h.changes.at(-1),[under,mark,over],"one undo restores the whole gesture");
    h.component.unmount();
  });
}

for (const mark of movableMarks) {
  test(`${mark.kind} ${mark.shape ?? ""} resize preview removes the old geometry and follows the handle`, () => {
    const h = annotation(documentWith([mark]), {selectedMarkId:mark.id});
    const bounds = model.selectionBounds(mark);
    const linear = mark.kind === "line" || mark.kind === "arrow";
    const point = linear ? mark.end : {x:bounds.x+bounds.width,y:bounds.y+bounds.height};
    h.pointer("onPointerDown",point.x,point.y);
    h.pointer("onPointerMove",point.x+30,point.y+20);
    const frame = h.frames.at(-1);
    assert.equal(frame.marks.length,1);
    assert.notDeepEqual(frame.marks[0],mark);
    assert.equal(frame.options.draft,null);
    assert.equal(frame.options.selectedIndex,0);
    assert.equal(h.changes.length,0);
    h.pointer("onPointerUp",point.x+30,point.y+20);
    assert.deepEqual(h.changes.at(-1),frame.marks);
    h.ref.current.undo();h.component.render();
    assert.deepEqual(h.changes.at(-1),[mark]);
    h.component.unmount();
  });
}

test("a newly drawn rectangle moves immediately and can be reopened without a ghost", () => {
  const h = annotation(documentWith([]), {tool:"rectangle"});
  h.pointer("onPointerDown",100,100);h.pointer("onPointerMove",200,180);h.pointer("onPointerUp",200,180);
  const mark = h.changes.at(-1)[0];
  h.component.render({...h.props,tool:"select"});
  h.pointer("onPointerDown",150,140);h.pointer("onPointerMove",180,160);
  assert.equal(h.frames.at(-1).marks.length,1);
  assert.equal(h.frames.at(-1).marks[0].rect.x,130);
  assert.equal(h.frames.at(-1).options.draft,null);
  h.pointer("onPointerUp",180,160);
  const reopened = annotation(documentWith(h.changes.at(-1)),{selectedMarkId:mark.id});
  reopened.pointer("onPointerDown",180,160);reopened.pointer("onPointerMove",190,170);
  assert.equal(reopened.frames.at(-1).marks.length,1);
  assert.equal(reopened.frames.at(-1).marks[0].rect.x,140);
  assert.equal(reopened.frames.at(-1).options.draft,null);
  h.component.unmount();reopened.component.unmount();
});

test("selecting a label stays stationary when its inspector changes the stage mid-click",()=>{
  const h=annotation(documentWith([label]));
  h.pointer("onPointerDown",200,120);
  h.live.getBoundingClientRect=()=>({left:20,top:30,width:512,height:288});
  h.pointer("onPointerUp",200,120);
  assert.equal(h.changes.length,0);
  assert.deepEqual(h.frames.at(-1).marks,[label]);
  const button=dotButton(h);
  button.props.onClick({stopPropagation(){}});h.component.render();
  assertPoint(dotOf(h.changes.at(-1)[0]),dotOf(label));
  assert.equal(h.changes.at(-1)[0].labelDirection,"right");
  assert.equal(h.changes.at(-1)[0].text,label.text);
});
