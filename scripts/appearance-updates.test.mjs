import assert from 'node:assert/strict';
import test from 'node:test';
import {readFileSync} from 'node:fs';
import { AppearanceUpdates } from '../src/annotation/appearance-updates.js';
import {createLibraryHarness, settleRequests} from './helpers/library-render-harness.mjs';
const initial = { colorPreset: 'cherry', penWidth: 3, textFontSize: 18 };

test('stale editor color changes preserve another editor width', () => {
  const a = new AppearanceUpdates(initial), b = new AppearanceUpdates(initial);
  a.update({ ...a.current, penWidth: 24 });
  const first = a.beginSave();
  const saved = { ...initial, ...first };
  a.receive(saved); a.finishSave(saved);
  // B has not received the update; its write still contains only color.
  b.update({ ...b.current, colorPreset: 'blue' });
  const patch = b.beginSave();
  assert.deepEqual(patch, { colorPreset: 'blue' });
  const combined = { ...saved, ...patch };
  b.receive(combined); a.receive(combined); b.finishSave(combined);
  assert.equal(b.current.penWidth, 24);
  assert.deepEqual(a.current, b.current);
});
test('in-flight writes preserve newer local edits and external fields', () => {
  const window = new AppearanceUpdates(initial);
  window.update({ ...window.current, penWidth: 24 });
  window.beginSave();
  window.update({ ...window.current, penWidth: 13 });
  window.receive({ ...initial, penWidth: 24, colorPreset: 'orange' });
  window.finishSave({ ...initial, penWidth: 24 });
  assert.deepEqual(window.current, { ...initial, penWidth: 13, colorPreset: 'orange' });
  assert.deepEqual(window.beginSave(), { penWidth: 13 });
});
test('failed writes retry their fields without losing edits made during the request', () => {
  const window = new AppearanceUpdates(initial);
  window.update({ ...window.current, penWidth: 24 }); window.beginSave();
  window.update({ ...window.current, colorPreset: 'blue' });
  window.failSave();
  assert.deepEqual(window.beginSave(), { penWidth: 24, colorPreset: 'blue' });
});

test('a stale rendered callback changes only its intended field after a native event', () => {
  const window = new AppearanceUpdates(initial);
  const rendered = {...window.current};
  window.receive({...initial, penWidth: 24});
  window.update({...rendered, colorPreset: 'blue'}, rendered);
  assert.equal(window.current.penWidth, 24);
  assert.equal(window.current.colorPreset, 'blue');
  assert.deepEqual(window.beginSave(), {colorPreset: 'blue'});
});

test('several callbacks from one render preserve both unsaved changes', () => {
  const window = new AppearanceUpdates(initial);
  const rendered = {...window.current};
  window.update({...rendered, penWidth: 24}, rendered);
  window.update({...rendered, colorPreset: 'blue'}, rendered);
  assert.equal(window.current.penWidth, 24);
  assert.deepEqual(window.beginSave(), {penWidth: 24, colorPreset: 'blue'});
});

test('watermark and mosaic preferences from independent windows merge field by field',()=>{
  const initial={watermarkColor:'black',watermarkFontSize:28,watermarkOpacity:20,watermarkRotation:-30,
    watermarkMode:'tiled',watermarkSpacing:80,mosaicShape:'brush'};
  const overlay=new AppearanceUpdates(initial),editor=new AppearanceUpdates(initial);
  const editorRender={...editor.current};
  overlay.update({...overlay.current,watermarkSpacing:160,watermarkRotation:45});
  const first=overlay.beginSave(),saved={...initial,...first};
  overlay.finishSave(saved);editor.receive(saved);
  editor.update({...editorRender,watermarkOpacity:55,mosaicShape:'ellipse'},editorRender);
  assert.deepEqual(editor.beginSave(),{watermarkOpacity:55,mosaicShape:'ellipse'});
  const combined={...saved,watermarkOpacity:55,mosaicShape:'ellipse'};
  editor.finishSave(combined);overlay.receive(combined);
  assert.deepEqual(overlay.current,editor.current);
  assert.equal(editor.current.watermarkSpacing,160);assert.equal(editor.current.watermarkRotation,45);
});

function appearanceHook(saved) {
  const writes = [], timers = new Map();
  let changed, sequence = 0;
  const harness = createLibraryHarness({}, readFileSync(new URL('../src/annotation/useAnnotationAppearance.ts', import.meta.url), 'utf8'), {
    modules: {
      './model': {DEFAULT_APPEARANCE: {...saved, watermarkMode: 'tiled'}},
      './appearance-updates.js': {AppearanceUpdates},
      '../lib/ipc': {
        api: {
          getAnnotationAppearance: async () => saved,
          setAnnotationAppearance: async patch => {writes.push(patch); saved = {...saved, ...patch}; return saved;},
        },
        onAnnotationAppearanceChanged: async callback => {changed = callback; return () => {changed = undefined;};},
      },
    },
  });
  harness.window.setTimeout = callback => {const id = ++sequence; timers.set(id, callback); return id;};
  harness.window.clearTimeout = id => timers.delete(id);
  const component = harness.mount('useAnnotationAppearance', {});
  component.render();
  return {component, writes, get saved() {return saved;}, receive(value) {saved = value; changed(value);},
    async load() {await settleRequests(); return component.render();},
    async save() {for (const [id, callback] of timers) {timers.delete(id); callback();} await settleRequests(); return component.render();},
  };
}

test('legacy single appearance loads as tiled without a write or loss of other saved preferences', async () => {
  const saved = {...initial, watermarkMode: 'single', watermarkFontSize: 28, watermarkOpacity: 20, watermarkSpacing: 80};
  const h = appearanceHook(saved);
  assert.deepEqual((await h.load())[0], {...saved, watermarkMode: 'tiled'});
  assert.deepEqual(h.writes, []);
  h.receive({...saved, watermarkSpacing: 160, penWidth: 24});
  assert.deepEqual(h.component.render()[0], {...saved, watermarkMode: 'tiled', watermarkSpacing: 160, penWidth: 24});
  await h.save();
  assert.deepEqual(h.writes, [], 'reading another window preference does not migrate old content or write defaults');
  h.component.unmount();
});

test('the tiled-only hook saves just explicit fields and preserves a simultaneous independent update', async () => {
  const saved = {...initial, watermarkMode: 'single', watermarkFontSize: 28, watermarkOpacity: 20, watermarkSpacing: 80};
  const h = appearanceHook(saved);
  const [rendered, update] = await h.load();
  h.receive({...saved, watermarkSpacing: 160, penWidth: 24});
  update({...rendered, watermarkMode: 'single', watermarkOpacity: 47});
  assert.equal(h.component.render()[0].watermarkMode, 'tiled');
  const [appearance] = await h.save();
  assert.deepEqual(h.writes, [{watermarkOpacity: 47}]);
  assert.equal(appearance.watermarkMode, 'tiled');
  assert.equal(appearance.watermarkSpacing, 160); assert.equal(appearance.penWidth, 24);
  assert.equal(h.saved.watermarkMode, 'single', 'UI normalization alone is not a background preference migration');
  h.component.unmount();
});
