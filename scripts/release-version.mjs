#!/usr/bin/env node

import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const currentFile = fileURLToPath(import.meta.url);
const repositoryRoot = resolve(dirname(currentFile), "..");

export function cargoPackageVersion(contents) {
  const lines = contents.split(/\r?\n/);
  const start = lines.findIndex((line) => line.trim() === "[package]");
  const end = lines.findIndex((line, index) => index > start && /^\s*\[/.test(line));
  const packageBlock = start >= 0
    ? lines.slice(start + 1, end < 0 ? undefined : end).join("\n")
    : "";
  const version = packageBlock.match(/^version\s*=\s*"([^"]+)"\s*$/m)?.[1];
  if (!version) throw new Error("Could not read the package version from src-tauri/Cargo.toml.");
  return version;
}

export function cargoLockVersion(contents) {
  const packageBlock = contents
    .replaceAll("\r\n", "\n")
    .split(/\n(?=\[\[package\]\]\n)/)
    .find((block) => /^\[\[package\]\]\s*$[\s\S]*?^name\s*=\s*"kiri"\s*$/m.test(block));
  const version = packageBlock?.match(/^version\s*=\s*"([^"]+)"\s*$/m)?.[1];
  if (!version) throw new Error("Could not read the kiri version from src-tauri/Cargo.lock.");
  return version;
}

export function readReleaseVersions(root = repositoryRoot) {
  const packageJson = JSON.parse(readFileSync(resolve(root, "package.json"), "utf8"));
  const tauriConfig = JSON.parse(
    readFileSync(resolve(root, "src-tauri", "tauri.conf.json"), "utf8"),
  );
  return {
    packageJson: packageJson.version,
    cargoToml: cargoPackageVersion(
      readFileSync(resolve(root, "src-tauri", "Cargo.toml"), "utf8"),
    ),
    cargoLock: cargoLockVersion(
      readFileSync(resolve(root, "src-tauri", "Cargo.lock"), "utf8"),
    ),
    tauriConfig: tauriConfig.version,
  };
}

export function assertReleaseVersions(versions, tag) {
  const entries = Object.entries(versions);
  const expected = entries[0]?.[1];
  if (!expected || entries.some(([, version]) => version !== expected)) {
    const details = entries.map(([source, version]) => `${source}=${version}`).join(", ");
    throw new Error(`Release versions do not match: ${details}`);
  }
  if (tag && tag !== `v${expected}`) {
    throw new Error(`Release tag ${tag} does not match source version v${expected}.`);
  }
  return expected;
}

if (process.argv[1] && resolve(process.argv[1]) === currentFile) {
  try {
    const version = assertReleaseVersions(readReleaseVersions(), process.argv[2]);
    console.log(`Release version check passed: v${version}`);
  } catch (error) {
    console.error(error instanceof Error ? error.message : error);
    process.exitCode = 1;
  }
}
