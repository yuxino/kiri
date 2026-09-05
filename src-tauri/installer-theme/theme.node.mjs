import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { buildTheme, decodeArtwork, loadArtwork, themeDirectory, toBitmap } from './build.mjs';

const source = loadArtwork;
test('both assets decode to the native NSIS dimensions', () => {
  const assets = decodeArtwork(source());
  assert.deepEqual(Object.keys(assets), ['sidebar', 'header']);
  for (const [name, size] of [['sidebar', [164, 314]], ['header', [150, 57]]]) {
    const bitmap = toBitmap(assets[name]);
    assert.equal(bitmap.subarray(0, 2).toString(), 'BM');
    assert.equal(bitmap.readUInt32LE(2), bitmap.length);
    assert.equal(bitmap.readUInt32LE(10), 54);
    assert.equal(bitmap.readInt32LE(18), size[0]);
    assert.equal(bitmap.readInt32LE(22), size[1]);
    assert.equal(bitmap.readUInt16LE(28), 24);
    assert.equal(bitmap.readUInt32LE(30), 0);
  }
});
test('BMP rows are bottom-up, BGR and padded to four bytes', () => {
  const bitmap = toBitmap({ width: 1, height: 2,
    palette: Buffer.from([255, 0, 0, 0, 0, 255]), pixels: Buffer.from([0, 1]) });
  assert.deepEqual([...bitmap.subarray(54)], [255, 0, 0, 0, 0, 0, 255, 0]);
});
test('corrupt art, wrong dimensions and unexpected assets fail closed', () => {
  const corrupt = source(); corrupt.assets.sidebar.sha256 = '0'.repeat(64);
  assert.throws(() => decodeArtwork(corrupt), /Corrupt/);
  const oversized = source(); oversized.assets.sidebar.width = 999999;
  assert.throws(() => decodeArtwork(oversized), /metadata/);
  const extra = source(); extra.assets.extra = extra.assets.header;
  assert.throws(() => decodeArtwork(extra), /incomplete/);
});
test('generation is deterministic, detects drift and can repair its own output', () => {
  const directory = mkdtempSync(join(tmpdir(), 'yuxino-theme-test-'));
  try {
    buildTheme(directory); buildTheme(directory, true);
    const first = readFileSync(join(directory, 'sidebar.bmp'));
    buildTheme(directory);
    assert.deepEqual(readFileSync(join(directory, 'sidebar.bmp')), first);
    writeFileSync(join(directory, 'sidebar.bmp'), 'corrupt');
    assert.throws(() => buildTheme(directory, true), /Stale/);
    buildTheme(directory); buildTheme(directory, true);
  } finally { rmSync(directory, { recursive: true, force: true }); }
});
test('the NSIS hook is presentation-only and keeps runtime controls native', () => {
  const hook = readFileSync(join(themeDirectory, 'theme.nsh'), 'utf8');
  assert.doesNotMatch(hook, /^\s*(?:Function|Section|Page|Exec\w*|Delete|RMDir|WriteReg\w*|RequestExecutionLevel|InstallDir)\b/m);
  assert.doesNotMatch(hook, /^\s*!define\s+MUI_PAGE_CUSTOMFUNCTION_PRE\b/m);
  for (const id of [1033, 2052, 1041]) {
    assert.match(hook, new RegExp(`LangString YuxinoWelcomeTitle ${id} `));
    assert.match(hook, new RegExp(`LangString YuxinoFinishText ${id} `));
  }
  assert.match(hook, /\$\(\^Name\)/);
});

test('pinned data can be provisioned once, then reused without network', async () => {
  const { ensureArtwork } = await import('./ensure-artwork.mjs');
  const directory = mkdtempSync(join(tmpdir(), 'yuxino-theme-fetch-'));
  try {
    writeFileSync(join(directory, 'artwork.lock.json'), readFileSync(join(themeDirectory, 'artwork.lock.json')));
    let requests = 0;
    await ensureArtwork({ directory, fetcher: async (url, options) => {
      assert.match(url, /^https:\/\/raw\.githubusercontent\.com\/yuxino\/kiri\/[a-f0-9]{40}\/src-tauri\/installer-theme\/artwork\//);
      assert.equal(options.redirect, 'error'); requests++;
      return new Response(readFileSync(join(themeDirectory, 'artwork', url.split('/').at(-1))));
    }});
    assert.equal(requests, 6);
    await ensureArtwork({ directory, offline: true, fetcher: () => { throw new Error('unexpected network'); }});
    writeFileSync(join(directory, 'artwork', 'header-0.b64'), 'bad');
    await assert.rejects(ensureArtwork({ directory, offline: true }), /checksum mismatch/);
  } finally { rmSync(directory, { recursive: true, force: true }); }
});
test('missing, failed and oversized downloads never produce trusted assets', async () => {
  const { ensureArtwork } = await import('./ensure-artwork.mjs');
  const directory = mkdtempSync(join(tmpdir(), 'yuxino-theme-fail-'));
  try {
    writeFileSync(join(directory, 'artwork.lock.json'), readFileSync(join(themeDirectory, 'artwork.lock.json')));
    await assert.rejects(ensureArtwork({ directory, offline: true }), /Missing artwork/);
    await assert.rejects(ensureArtwork({ directory, fetcher: async () => new Response('', { status: 503 }) }), /download failed/);
    await assert.rejects(ensureArtwork({ directory, fetcher: async () => new Response('x'.repeat(20000)) }), /Oversized/);
    await assert.rejects(ensureArtwork({ directory, fetcher: async () => new Response('incorrect') }), /checksum mismatch/);
  } finally { rmSync(directory, { recursive: true, force: true }); }
});
