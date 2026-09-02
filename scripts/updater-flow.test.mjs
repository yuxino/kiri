import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (path) => readFileSync(resolve(root, path), "utf8");

test("the app uses Tauri's signed updater as a manual staged flow", () => {
  const settings = read("src/settings/SettingsView.tsx");
  const ipc = read("src/lib/ipc.ts");
  const rust = read("src-tauri/src/updates.rs");
  const lib = read("src-tauri/src/lib.rs");

  assert.match(settings, /from "@tauri-apps\/plugin-updater"/);
  assert.match(settings, /await check\(/);
  assert.match(settings, /await update\.download\(/);
  assert.match(settings, /event\.event === "Progress"/);
  assert.match(settings, /await update\.install\(\{ restartAfterInstall: false \}\)/);
  assert.match(settings, /await relaunch\(\)/);
  assert.match(settings, /update\.body/);
  assert.doesNotMatch(settings, /downloadAndInstall/);
  assert.doesNotMatch(ipc, /check_for_updates|checkForUpdates/);
  assert.doesNotMatch(rust, /api\.github\.com|check_for_updates/);
  assert.match(rust, /github\.com\/yuxino\/kiri\/releases\/latest/);
  assert.match(lib, /tauri_plugin_updater::Builder::new\(\)\.build\(\)/);
  assert.match(lib, /tauri_plugin_process::init\(\)/);
});

test("updater configuration is fixed, HTTPS-only, and least-privileged", () => {
  const config = JSON.parse(read("src-tauri/tauri.conf.json"));
  const defaultCapability = JSON.parse(read("src-tauri/capabilities/default.json"));
  const capability = JSON.parse(read("src-tauri/capabilities/updater.json"));
  const packageJson = JSON.parse(read("package.json"));
  const decodedPublicKey = Buffer.from(config.plugins.updater.pubkey, "base64").toString("utf8");

  assert.equal(config.bundle.createUpdaterArtifacts, true);
  assert.deepEqual(config.plugins.updater.endpoints, [
    "https://github.com/yuxino/kiri/releases/latest/download/latest.json",
  ]);
  assert.match(decodedPublicKey, /^untrusted comment: minisign public key:/);
  assert.equal(decodedPublicKey.includes("PRIVATE KEY"), false);
  assert.deepEqual(defaultCapability.permissions.filter((permission) => /^(?:updater|process):/.test(permission)), []);
  assert.deepEqual(capability.windows, ["library"]);
  assert.deepEqual(
    capability.permissions.filter((permission) => permission.startsWith("updater:")),
    ["updater:allow-check", "updater:allow-download", "updater:allow-install"],
  );
  assert.equal(capability.permissions.includes("process:allow-restart"), true);
  assert.equal(packageJson.dependencies["@tauri-apps/plugin-updater"], "^2.11.0");
  assert.equal(packageJson.dependencies["@tauri-apps/plugin-process"], "^2.3.1");
});

test("CI signs updater artifacts without exposing a private key", () => {
  const build = read(".github/workflows/build.yml");
  const release = read(".github/workflows/release.yml");
  const packager = read("scripts/package-macos-release.sh");

  for (const source of [build, release]) {
    assert.match(source, /secrets\.TAURI_SIGNING_PRIVATE_KEY/);
    assert.match(source, /secrets\.TAURI_SIGNING_PRIVATE_KEY_PASSWORD/);
    assert.doesNotMatch(source, /BEGIN (?:OPENSSH |RSA )?PRIVATE KEY/);
  }
  assert.match(release, /updaterJsonPreferNsis:\s*true/);
  assert.match(release, /uploadUpdaterJson:\s*true/);
  assert.match(release, /uploadUpdaterSignatures:\s*true/);
  assert.match(packager, /security find-generic-password/);
  assert.match(packager, /kiri\.app\.tar\.gz/);
  assert.match(packager, /TAURI_SIGNING_PRIVATE_KEY_PASSWORD/);
});
