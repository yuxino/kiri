/** yuxino installer theme v1.0.0 — build-time renderer; no application runtime changes or npm dependencies. */
import { ensureArtwork } from './ensure-artwork.mjs';
import { createHash } from 'node:crypto';
import { readFileSync, mkdirSync, writeFileSync, renameSync } from 'node:fs';
import { resolve, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { brotliDecompressSync } from 'node:zlib';

export const themeDirectory = fileURLToPath(new URL('.', import.meta.url));
const dimensions = { sidebar: [164, 314], header: [150, 57] };

export function loadArtwork() {
  const folder = join(themeDirectory, 'artwork');
  const pack = JSON.parse(readFileSync(join(folder, 'manifest.json'), 'utf8'));
  for (const [name, entry] of Object.entries(pack.assets ?? {})) {
    const count = name === 'sidebar' ? 4 : name === 'header' ? 1 : 0;
    if (!count || !Array.isArray(entry.parts) || entry.parts.length !== count ||
        entry.parts.some((file, i) => file !== `${name}-${i}.b64`)) throw new Error('Invalid artwork parts');
    entry.data = entry.parts.map(file => readFileSync(join(folder, file), 'utf8')).join('');
  }
  return pack;
}

export function decodeArtwork(pack) {
  if (pack?.format !== 'yuxino-indexed-brotli-v1' ||
      JSON.stringify(Object.keys(pack.assets ?? {}).sort()) !== '["header","sidebar"]') {
    throw new Error('Unsupported or incomplete installer artwork');
  }
  return Object.fromEntries(Object.entries(dimensions).map(([name, [width, height]]) => {
    const entry = pack.assets[name];
    if (entry.width !== width || entry.height !== height || entry.paletteSize !== 32 ||
        typeof entry.data !== 'string' || entry.data.length > 100_000 ||
        !/^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/.test(entry.data)) {
      throw new Error(`Invalid ${name} artwork metadata`);
    }
    const expectedSize = 32 * 3 + width * height;
    const data = brotliDecompressSync(Buffer.from(entry.data, 'base64'), { maxOutputLength: expectedSize });
    if (data.length !== expectedSize || createHash('sha256').update(data).digest('hex') !== entry.sha256) {
      throw new Error(`Corrupt ${name} artwork`);
    }
    const palette = data.subarray(0, 96);
    const pixels = data.subarray(96);
    if (pixels.some(index => index >= 32)) throw new Error(`Invalid ${name} palette index`);
    return [name, { width, height, palette, pixels }];
  }));
}

/** Emit ordinary, opaque 24-bit BI_RGB BMPs accepted by native NSIS controls. */
export function toBitmap({ width, height, palette, pixels }) {
  const stride = (width * 3 + 3) & ~3;
  const bitmap = Buffer.alloc(54 + stride * height);
  bitmap.write('BM', 0, 'ascii');
  bitmap.writeUInt32LE(bitmap.length, 2);
  bitmap.writeUInt32LE(54, 10);
  bitmap.writeUInt32LE(40, 14);
  bitmap.writeInt32LE(width, 18);
  bitmap.writeInt32LE(height, 22);
  bitmap.writeUInt16LE(1, 26);
  bitmap.writeUInt16LE(24, 28);
  bitmap.writeUInt32LE(stride * height, 34);
  bitmap.writeInt32LE(3780, 38);
  bitmap.writeInt32LE(3780, 42);
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const source = pixels[y * width + x] * 3;
      const target = 54 + (height - 1 - y) * stride + x * 3;
      bitmap[target] = palette[source + 2];
      bitmap[target + 1] = palette[source + 1];
      bitmap[target + 2] = palette[source];
    }
  }
  return bitmap;
}

export function buildTheme(outputDirectory = join(themeDirectory, 'generated'), check = false) {
  const pack = loadArtwork();
  const artwork = decodeArtwork(pack);
  if (!check) mkdirSync(outputDirectory, { recursive: true });
  for (const [name, image] of Object.entries(artwork)) {
    const destination = join(outputDirectory, `${name}.bmp`);
    const bitmap = toBitmap(image);
    if (check) {
      if (!readFileSync(destination).equals(bitmap)) throw new Error(`Stale installer artwork: ${name}`);
    } else {
      const temporary = `${destination}.${process.pid}.tmp`;
      writeFileSync(temporary, bitmap);
      renameSync(temporary, destination);
    }
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const arguments_ = process.argv.slice(2);
    if (arguments_.some(value => value !== '--check') || arguments_.length > 1) {
      throw new Error('Usage: node src-tauri/installer-theme/build.mjs [--check]');
    }
    await ensureArtwork({ offline: arguments_.includes('--check') });
    buildTheme(undefined, arguments_.includes('--check'));
    console.log('yuxino installer theme v1.0.0: artwork verified');
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  }
}
