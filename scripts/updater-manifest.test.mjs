import assert from "node:assert/strict";
import test from "node:test";

import { buildUpdaterManifest } from "./updater-manifest.mjs";

const signature = Buffer.alloc(96, 7).toString("base64");

test("builds one signed static manifest for Universal macOS and Windows NSIS", () => {
  const manifest = buildUpdaterManifest({
    version: "1.4.9",
    tag: "v1.4.9",
    notes: "Native media and signed updates.",
    pubDate: "2026-09-02T10:00:00Z",
    macAsset: "kiri.app.tar.gz",
    macSignature: signature,
    windowsAsset: "Kiri_1.4.9_x64-setup.exe",
    windowsSignature: signature,
  });

  assert.equal(manifest.version, "1.4.9");
  assert.equal(manifest.pub_date, "2026-09-02T10:00:00.000Z");
  assert.deepEqual(
    Object.keys(manifest.platforms),
    ["darwin-aarch64", "darwin-x86_64", "windows-x86_64", "windows-x86_64-nsis"],
  );
  assert.strictEqual(manifest.platforms["darwin-aarch64"], manifest.platforms["darwin-x86_64"]);
  assert.strictEqual(manifest.platforms["windows-x86_64"], manifest.platforms["windows-x86_64-nsis"]);
  assert.match(manifest.platforms["darwin-aarch64"].url, /\/v1.4.9\/kiri.app.tar.gz$/);
  assert.match(manifest.platforms["windows-x86_64"].url, /Kiri_1.4.9_x64-setup.exe$/);
});

test("rejects mismatched tags, malformed signatures, and nested asset paths", () => {
  const valid = {
    version: "1.4.9",
    tag: "v1.4.9",
    notes: "notes",
    pubDate: "2026-09-02T10:00:00Z",
    macAsset: "kiri.app.tar.gz",
    macSignature: signature,
    windowsAsset: "Kiri.exe",
    windowsSignature: signature,
  };
  assert.throws(() => buildUpdaterManifest({ ...valid, tag: "v1.4.8" }), /does not match/);
  assert.throws(() => buildUpdaterManifest({ ...valid, macSignature: "short" }), /signature/);
  assert.throws(() => buildUpdaterManifest({ ...valid, windowsAsset: "nested/Kiri.exe" }), /asset name/);
});
