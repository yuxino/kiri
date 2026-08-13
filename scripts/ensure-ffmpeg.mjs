#!/usr/bin/env node
// Downloads the ffmpeg binary for the host platform into
// src-tauri/binaries/ffmpeg-<target-triple>/ffmpeg so releases bundle it.
// Run from the repository root: node scripts/ensure-ffmpeg.mjs

import { execSync } from "node:child_process";
import { createWriteStream, existsSync, mkdirSync, rmSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { Readable } from "node:stream";
import { pipeline } from "node:stream/promises";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");

function targetTriple() {
  const out = execSync("rustc -vV", { encoding: "utf8" });
  const match = out.match(/host: (\S+)/);
  if (!match) throw new Error("could not detect rustc host");
  return match[1];
}

function downloadUrl(triple) {
  if (triple.startsWith("x86_64-pc-windows") || triple.startsWith("aarch64-pc-windows")) {
    return "https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip";
  }
  if (triple.startsWith("aarch64-apple-darwin")) {
    return "https://www.osxexperts.net/ffmpeg71arm.zip";
  }
  return "https://www.osxexperts.net/ffmpeg71intel.zip";
}

const triple = targetTriple();
const outDir = join(root, "src-tauri", "binaries", `ffmpeg-${triple}`);
if (existsSync(join(outDir, "ffmpeg")) || existsSync(join(outDir, "ffmpeg.exe"))) {
  console.log(`ffmpeg already present for ${triple}`);
  process.exit(0);
}

mkdirSync(outDir, { recursive: true });
const url = downloadUrl(triple);
console.log(`downloading ${url}`);
const response = await fetch(url);
if (!response.ok) throw new Error(`download failed: ${response.status}`);
const buffer = Buffer.from(await response.arrayBuffer());

const zipPath = join(outDir, "ffmpeg.zip");
const { writeFileSync } = await import("node:fs");
writeFileSync(zipPath, buffer);
console.log("unzipping…");
try {
  execSync(`cd "${outDir}" && unzip -o -q ffmpeg.zip`);
} catch {
  execSync(`powershell -NoProfile -Command "Expand-Archive -Path '${outDir}\\ffmpeg.zip' -DestinationPath '${outDir}' -Force"`);
}
rmSync(zipPath, { force: true });

// Locate the extracted binary and place it directly in outDir.
const { readdirSync, renameSync, statSync } = await import("node:fs");
function findBinary(dir) {
  for (const entry of readdirSync(dir)) {
    const path = join(dir, entry);
    if (statSync(path).isDirectory()) {
      const found = findBinary(path);
      if (found) return found;
    } else if (entry === "ffmpeg" || entry === "ffmpeg.exe") {
      return path;
    }
  }
  return null;
}
const binary = findBinary(outDir);
if (!binary) {
  console.error("could not locate ffmpeg binary in the archive");
  process.exit(1);
}
const finalPath = join(outDir, triple.startsWith("x86_64-pc-windows") || triple.startsWith("aarch64-pc-windows") ? "ffmpeg.exe" : "ffmpeg");
if (binary !== finalPath) renameSync(binary, finalPath);

// Mirror into a stable path used by tauri bundle resources.
const currentDir = join(root, "src-tauri", "binaries", "ffmpeg-current");
mkdirSync(currentDir, { recursive: true });
const currentPath = join(currentDir, triple.startsWith("x86_64-pc-windows") || triple.startsWith("aarch64-pc-windows") ? "ffmpeg.exe" : "ffmpeg");
const { copyFileSync } = await import("node:fs");
copyFileSync(finalPath, currentPath);
console.log(`ffmpeg ready at ${finalPath}`);
