#!/usr/bin/env node

import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { basename, dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const currentFile = fileURLToPath(import.meta.url);

function requireVersion(version) {
  if (!/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(version)) {
    throw new Error(`Invalid updater version: ${version}`);
  }
  return version;
}

function requireSignature(signature, label) {
  const normalized = signature.trim();
  if (normalized.length < 80 || !/^[A-Za-z0-9+/=]+$/.test(normalized)) {
    throw new Error(`${label} is not a valid updater signature`);
  }
  return normalized;
}

function assetUrl(tag, filename) {
  if (basename(filename) !== filename || /[\r\n]/.test(filename)) {
    throw new Error(`Invalid release asset name: ${filename}`);
  }
  return `https://github.com/yuxino/kiri/releases/download/${encodeURIComponent(tag)}/${encodeURIComponent(filename)}`;
}

export function buildUpdaterManifest({
  version,
  tag,
  notes,
  pubDate,
  macAsset,
  macSignature,
  windowsAsset,
  windowsSignature,
}) {
  requireVersion(version);
  if (tag !== `v${version}`) throw new Error(`Release tag ${tag} does not match v${version}`);
  if (!Number.isFinite(Date.parse(pubDate))) throw new Error(`Invalid publication date: ${pubDate}`);

  const mac = {
    signature: requireSignature(macSignature, "macOS signature"),
    url: assetUrl(tag, macAsset),
  };
  const windows = {
    signature: requireSignature(windowsSignature, "Windows signature"),
    url: assetUrl(tag, windowsAsset),
  };

  return {
    version,
    notes,
    pub_date: new Date(pubDate).toISOString(),
    platforms: {
      "darwin-aarch64": mac,
      "darwin-x86_64": mac,
      "windows-x86_64": windows,
      "windows-x86_64-nsis": windows,
    },
  };
}

function parseArguments(arguments_) {
  const values = new Map();
  for (let index = 0; index < arguments_.length; index += 2) {
    const key = arguments_[index];
    const value = arguments_[index + 1];
    if (!key?.startsWith("--") || value === undefined) {
      throw new Error("Updater manifest arguments must be --name value pairs");
    }
    values.set(key.slice(2), value);
  }
  return values;
}

function required(values, key) {
  const value = values.get(key);
  if (!value) throw new Error(`Missing --${key}`);
  return value;
}

export function run(arguments_) {
  const values = parseArguments(arguments_);
  const macAssetPath = resolve(required(values, "mac-asset"));
  const windowsAssetPath = resolve(required(values, "windows-asset"));
  const macSignaturePath = resolve(required(values, "mac-signature"));
  const windowsSignaturePath = resolve(required(values, "windows-signature"));
  const notesPath = resolve(required(values, "notes-file"));
  const outputPath = resolve(required(values, "output"));

  for (const path of [
    macAssetPath,
    windowsAssetPath,
    macSignaturePath,
    windowsSignaturePath,
    notesPath,
  ]) {
    if (!existsSync(path)) throw new Error(`Required updater input is missing: ${path}`);
  }

  const manifest = buildUpdaterManifest({
    version: required(values, "version"),
    tag: required(values, "tag"),
    notes: readFileSync(notesPath, "utf8").trim(),
    pubDate: values.get("pub-date") ?? new Date().toISOString(),
    macAsset: basename(macAssetPath),
    macSignature: readFileSync(macSignaturePath, "utf8"),
    windowsAsset: basename(windowsAssetPath),
    windowsSignature: readFileSync(windowsSignaturePath, "utf8"),
  });

  writeFileSync(outputPath, `${JSON.stringify(manifest, null, 2)}\n`, { mode: 0o644 });
  return outputPath;
}

if (process.argv[1] && resolve(process.argv[1]) === currentFile) {
  try {
    console.log(`Wrote signed updater manifest: ${run(process.argv.slice(2))}`);
  } catch (error) {
    console.error(error instanceof Error ? error.message : error);
    process.exitCode = 1;
  }
}
