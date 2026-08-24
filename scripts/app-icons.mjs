#!/usr/bin/env node

import {
  copyFileSync,
  existsSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";
import { inflateSync } from "node:zlib";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const iconDir = join(repoRoot, "src-tauri", "icons");
const sourceIcon = join(iconDir, "app-icon-source.png");
const desktopPngs = [
  ["32x32.png", 32],
  ["128x128.png", 128],
  ["128x128@2x.png", 256],
];
const desktopPngNames = desktopPngs.map(([name]) => name);
const generatedNames = [...desktopPngNames, "icon.icns", "icon.ico"];
const pngSignature = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);

function fail(message) {
  throw new Error(message);
}

function readUInt32BE(buffer, offset, label) {
  if (offset + 4 > buffer.length) fail(`${label}: truncated 32-bit value`);
  return buffer.readUInt32BE(offset);
}

function paethPredictor(left, above, upperLeft) {
  const estimate = left + above - upperLeft;
  const leftDistance = Math.abs(estimate - left);
  const aboveDistance = Math.abs(estimate - above);
  const upperLeftDistance = Math.abs(estimate - upperLeft);
  if (leftDistance <= aboveDistance && leftDistance <= upperLeftDistance) return left;
  return aboveDistance <= upperLeftDistance ? above : upperLeft;
}

function decodePngAlpha(buffer, label) {
  if (!buffer.subarray(0, 8).equals(pngSignature)) fail(`${label}: not a PNG`);

  let offset = 8;
  let header;
  const compressedParts = [];
  while (offset + 12 <= buffer.length) {
    const length = readUInt32BE(buffer, offset, label);
    const type = buffer.toString("ascii", offset + 4, offset + 8);
    const dataStart = offset + 8;
    const dataEnd = dataStart + length;
    if (dataEnd + 4 > buffer.length) fail(`${label}: truncated ${type} chunk`);

    if (type === "IHDR") {
      header = {
        width: readUInt32BE(buffer, dataStart, label),
        height: readUInt32BE(buffer, dataStart + 4, label),
        bitDepth: buffer[dataStart + 8],
        colorType: buffer[dataStart + 9],
        interlace: buffer[dataStart + 12],
      };
    } else if (type === "IDAT") {
      compressedParts.push(buffer.subarray(dataStart, dataEnd));
    } else if (type === "IEND") {
      break;
    }
    offset = dataEnd + 4;
  }

  if (!header) fail(`${label}: missing IHDR`);
  if (header.width !== header.height || header.width === 0) {
    fail(`${label}: app icons must be non-empty squares`);
  }
  if (header.bitDepth !== 8 || ![4, 6].includes(header.colorType) || header.interlace !== 0) {
    fail(`${label}: expected a non-interlaced 8-bit PNG with a real alpha channel`);
  }
  if (compressedParts.length === 0) fail(`${label}: missing image data`);

  const bytesPerPixel = header.colorType === 6 ? 4 : 2;
  const alphaOffset = bytesPerPixel - 1;
  const stride = header.width * bytesPerPixel;
  const filtered = inflateSync(Buffer.concat(compressedParts));
  if (filtered.length !== (stride + 1) * header.height) {
    fail(`${label}: unexpected decompressed image size`);
  }

  const alpha = new Uint8Array(header.width * header.height);
  let inputOffset = 0;
  let previous = new Uint8Array(stride);
  for (let y = 0; y < header.height; y += 1) {
    const filter = filtered[inputOffset];
    inputOffset += 1;
    if (filter > 4) fail(`${label}: unsupported PNG filter ${filter}`);
    const row = new Uint8Array(stride);
    for (let x = 0; x < stride; x += 1) {
      const encoded = filtered[inputOffset];
      inputOffset += 1;
      const left = x >= bytesPerPixel ? row[x - bytesPerPixel] : 0;
      const above = previous[x];
      const upperLeft = x >= bytesPerPixel ? previous[x - bytesPerPixel] : 0;
      let prediction = 0;
      if (filter === 1) prediction = left;
      else if (filter === 2) prediction = above;
      else if (filter === 3) prediction = Math.floor((left + above) / 2);
      else if (filter === 4) prediction = paethPredictor(left, above, upperLeft);
      row[x] = (encoded + prediction) & 0xff;
    }
    for (let x = 0; x < header.width; x += 1) {
      alpha[y * header.width + x] = row[x * bytesPerPixel + alphaOffset];
    }
    previous = row;
  }

  return { width: header.width, height: header.height, alpha };
}

function validateAlpha({ width, height, alpha }, label) {
  const corners = [0, width - 1, (height - 1) * width, width * height - 1];
  if (corners.some((index) => alpha[index] > 8)) {
    fail(`${label}: all four corner pixels must be transparent`);
  }

  let transparent = 0;
  let translucent = 0;
  let opaque = 0;
  let visibleMinX = width;
  let visibleMinY = height;
  let visibleMaxX = -1;
  let visibleMaxY = -1;
  for (let y = 0; y < height; y += 1) {
    for (let x = 0; x < width; x += 1) {
      const value = alpha[y * width + x];
      if (value <= 8) {
        transparent += 1;
        continue;
      }

      visibleMinX = Math.min(visibleMinX, x);
      visibleMinY = Math.min(visibleMinY, y);
      visibleMaxX = Math.max(visibleMaxX, x);
      visibleMaxY = Math.max(visibleMaxY, y);
      if (value === 255) opaque += 1;
      else translucent += 1;
    }
  }
  const pixels = alpha.length;
  if (transparent / pixels < 0.05) {
    fail(`${label}: transparent padding is too small to prevent a square icon`);
  }
  if (opaque / pixels < 0.4) fail(`${label}: visible icon content is unexpectedly sparse`);
  if (translucent === 0) fail(`${label}: rounded edge is missing anti-aliasing`);

  const visibleWidth = visibleMaxX - visibleMinX + 1;
  const visibleHeight = visibleMaxY - visibleMinY + 1;
  const [minimumVisibleRatio, maximumVisibleRatio] =
    width >= 64 ? [0.78, 0.84] : width >= 32 ? [0.75, 0.86] : [0.7, 0.9];
  for (const [axis, visibleSize, canvasSize] of [
    ["width", visibleWidth, width],
    ["height", visibleHeight, height],
  ]) {
    const ratio = visibleSize / canvasSize;
    if (ratio < minimumVisibleRatio || ratio > maximumVisibleRatio) {
      fail(
        `${label}: visible ${axis} must occupy ${Math.round(minimumVisibleRatio * 100)}-` +
          `${Math.round(maximumVisibleRatio * 100)}% of the canvas, got ${Math.round(ratio * 100)}%`,
      );
    }
  }

  const center = Math.floor(height / 2) * width + Math.floor(width / 2);
  if (alpha[center] < 250) fail(`${label}: center must remain visible`);

  const cornerSize = Math.max(1, Math.floor(width * 0.03));
  let cornerPixels = 0;
  let clearCornerPixels = 0;
  for (let y = 0; y < cornerSize; y += 1) {
    for (let x = 0; x < cornerSize; x += 1) {
      for (const index of [
        y * width + x,
        y * width + (width - 1 - x),
        (height - 1 - y) * width + x,
        (height - 1 - y) * width + (width - 1 - x),
      ]) {
        cornerPixels += 1;
        if (alpha[index] <= 8) clearCornerPixels += 1;
      }
    }
  }
  if (clearCornerPixels / cornerPixels < 0.95) {
    fail(`${label}: outer corner areas must remain transparent`);
  }

  const sample = (x, y) => {
    const pixelX = Math.round((width - 1) * x);
    const pixelY = Math.round((height - 1) * y);
    return alpha[pixelY * width + pixelX];
  };
  const outerEdges = [
    [0.5, 0.03],
    [0.97, 0.5],
    [0.5, 0.97],
    [0.03, 0.5],
  ];
  const innerEdges = [
    [0.5, 0.12],
    [0.88, 0.5],
    [0.5, 0.88],
    [0.12, 0.5],
  ];
  const roundedCorners = [
    [0.08, 0.08],
    [0.92, 0.08],
    [0.92, 0.92],
    [0.08, 0.92],
  ];
  const innerCorners = [
    [0.18, 0.18],
    [0.82, 0.18],
    [0.82, 0.82],
    [0.18, 0.82],
  ];
  const outerEdgeLimit = width < 32 ? 224 : 128;
  if (outerEdges.some(([x, y]) => sample(x, y) > outerEdgeLimit)) {
    fail(`${label}: transparent padding is missing around the icon`);
  }
  if (roundedCorners.some(([x, y]) => sample(x, y) > 64)) {
    fail(`${label}: expected a transparent rounded-square outline, not a hard square`);
  }
  if ([...innerEdges, ...innerCorners].some(([x, y]) => sample(x, y) < 200)) {
    fail(`${label}: rounded-square silhouette is clipped or too small`);
  }
}

function validatePng(buffer, label, expectedSize) {
  const decoded = decodePngAlpha(buffer, label);
  if (expectedSize && decoded.width !== expectedSize) {
    fail(`${label}: expected ${expectedSize}x${expectedSize}, got ${decoded.width}x${decoded.height}`);
  }
  validateAlpha(decoded, label);
  return decoded.width;
}

function validateIcns(path) {
  const buffer = readFileSync(path);
  if (buffer.toString("ascii", 0, 4) !== "icns") fail("icon.icns: invalid header");
  if (readUInt32BE(buffer, 4, "icon.icns") !== buffer.length) {
    fail("icon.icns: declared size does not match file size");
  }

  const sizes = new Set();
  let offset = 8;
  while (offset < buffer.length) {
    if (offset + 8 > buffer.length) fail("icon.icns: truncated entry header");
    const type = buffer.toString("ascii", offset, offset + 4);
    const length = readUInt32BE(buffer, offset + 4, "icon.icns");
    if (length < 8 || offset + length > buffer.length) fail(`icon.icns: invalid ${type} entry`);
    const payload = buffer.subarray(offset + 8, offset + length);
    if (payload.subarray(0, 8).equals(pngSignature)) {
      sizes.add(validatePng(payload, `icon.icns:${type}`));
    } else if (["s8mk", "l8mk", "h8mk", "t8mk"].includes(type)) {
      const size = Math.sqrt(payload.length);
      if (!Number.isInteger(size)) fail(`icon.icns:${type}: invalid alpha mask`);
      validateAlpha({ width: size, height: size, alpha: payload }, `icon.icns:${type}`);
      sizes.add(size);
    }
    offset += length;
  }

  for (const size of [16, 32, 64, 128, 256, 512, 1024]) {
    if (!sizes.has(size)) fail(`icon.icns: missing ${size}x${size} representation`);
  }
}

function canonicalizeIcns(buffer) {
  if (buffer.toString("ascii", 0, 4) !== "icns") fail("generated icon.icns: invalid header");
  const entries = [];
  let offset = 8;
  while (offset < buffer.length) {
    const length = readUInt32BE(buffer, offset + 4, "generated icon.icns");
    if (length < 8 || offset + length > buffer.length) fail("generated icon.icns: invalid entry");
    entries.push(buffer.subarray(offset, offset + length));
    offset += length;
  }
  entries.sort((left, right) => Buffer.compare(left, right));
  const header = Buffer.alloc(8);
  header.write("icns", 0, "ascii");
  header.writeUInt32BE(8 + entries.reduce((total, entry) => total + entry.length, 0), 4);
  return Buffer.concat([header, ...entries]);
}

function validateIco(path) {
  const buffer = readFileSync(path);
  if (buffer.length < 6 || buffer.readUInt16LE(0) !== 0 || buffer.readUInt16LE(2) !== 1) {
    fail("icon.ico: invalid header");
  }
  const count = buffer.readUInt16LE(4);
  if (count === 0 || 6 + count * 16 > buffer.length) fail("icon.ico: invalid directory");

  const sizes = new Set();
  for (let index = 0; index < count; index += 1) {
    const entry = 6 + index * 16;
    const width = buffer[entry] || 256;
    const height = buffer[entry + 1] || 256;
    const length = buffer.readUInt32LE(entry + 8);
    const offset = buffer.readUInt32LE(entry + 12);
    if (width !== height || offset + length > buffer.length) fail("icon.ico: invalid image entry");
    const payload = buffer.subarray(offset, offset + length);
    const decodedSize = validatePng(payload, `icon.ico:${width}x${height}`, width);
    sizes.add(decodedSize);
  }

  for (const size of [16, 24, 32, 48, 64, 256]) {
    if (!sizes.has(size)) fail(`icon.ico: missing ${size}x${size} representation`);
  }
}

function verifyIcons() {
  const sourceSize = validatePng(readFileSync(sourceIcon), "app-icon-source.png");
  if (sourceSize < 1024) fail("app-icon-source.png: master icon must be at least 1024x1024");
  for (const [name, expectedSize] of desktopPngs) {
    const path = join(iconDir, name);
    if (!existsSync(path)) fail(`${name}: missing generated icon`);
    validatePng(readFileSync(path), name, expectedSize);
  }
  validateIcns(join(iconDir, "icon.icns"));
  validateIco(join(iconDir, "icon.ico"));

  const config = JSON.parse(readFileSync(join(repoRoot, "src-tauri", "tauri.conf.json"), "utf8"));
  const configured = config.bundle?.icon;
  const expectedConfigured = [
    "icons/32x32.png",
    "icons/128x128.png",
    "icons/128x128@2x.png",
    "icons/icon.icns",
    "icons/icon.ico",
  ];
  if (!Array.isArray(configured) || JSON.stringify(configured) !== JSON.stringify(expectedConfigured)) {
    fail("tauri.conf.json: bundle.icon must use the verified desktop icon set in canonical order");
  }
  for (const relativePath of configured) {
    if (!existsSync(join(repoRoot, "src-tauri", relativePath))) {
      fail(`tauri.conf.json: missing configured icon ${relativePath}`);
    }
  }

  console.log(
    "App icon verification passed: transparent corners and visual-safe sizing are present in PNG, ICNS, and ICO assets.",
  );
}

function generateIcons() {
  const temporaryOutput = mkdtempSync(join(tmpdir(), "kiri-app-icons-"));
  try {
    const cli = join(repoRoot, "node_modules", "@tauri-apps", "cli", "tauri.js");
    if (!existsSync(cli)) fail("Tauri CLI is not installed; run pnpm install first");
    const result = spawnSync(process.execPath, [cli, "icon", sourceIcon, "--output", temporaryOutput], {
      cwd: repoRoot,
      stdio: "inherit",
    });
    if (result.error) throw result.error;
    if (result.status !== 0) fail(`Tauri icon generator exited with status ${result.status}`);

    for (const name of generatedNames) {
      const generated = join(temporaryOutput, name);
      if (!existsSync(generated)) fail(`Tauri icon generator did not create ${name}`);
      const target = join(iconDir, name);
      if (name === "icon.icns") writeFileSync(target, canonicalizeIcns(readFileSync(generated)));
      else copyFileSync(generated, target);
    }
  } finally {
    rmSync(temporaryOutput, { recursive: true, force: true });
  }
  verifyIcons();
}

const command = process.argv[2] ?? "verify";
try {
  if (command === "generate") generateIcons();
  else if (command === "verify") verifyIcons();
  else fail(`unknown command ${command}; use generate or verify`);
} catch (error) {
  console.error(`App icon check failed: ${error instanceof Error ? error.message : error}`);
  process.exitCode = 1;
}
