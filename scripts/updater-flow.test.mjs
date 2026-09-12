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
  assert.match(settings, /await update\.install\(\{ restartAfterInstall: true \}\)/);
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
  assert.equal(config.plugins.updater.windows.installMode, "passive");
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

// Run the actual update component's handlers with isolated IPC and hook state.
// No network, installer or user library is touched by these regressions.
const { createLibraryHarness, nodes, deferred, settleRequests } = await import('./helpers/library-render-harness.mjs');
function updaterHarness(windows = true) {
  const pending = deferred();
  const installs = [];
  let progress;
  let restarts = 0;
  const update = { version: '9.9.9', currentVersion: '1.5.0', body: 'Release notes',
    close: async () => {},
    download: callback => { progress = callback; return pending.promise; },
    install: async options => { installs.push(options); },
  };
  const settings = read('src/settings/SettingsView.tsx');
  const source = `import React, { useState, useRef, useEffect } from 'react';
    import { api } from '../lib/ipc';
    const navigator = { userAgent: '${windows ? 'Windows' : 'Macintosh'}' };
    const check = api.check, getVersion = api.getVersion, relaunch = api.relaunch;
    const t = value => value, fmt = (value, arg) => value.replace('%@', arg);
    ${settings.slice(settings.indexOf('type UpdateDetails ='), settings.indexOf('function GeneralSettingsSection'))}
    export { AboutSettingsSection };`;
  const harness = createLibraryHarness({ check: async () => update, getVersion: async () => '1.5.0',
    relaunch: async () => { restarts++; } }, source);
  const component = harness.mount('AboutSettingsSection');
  const render = () => component.render();
  const button = () => nodes(render()).find(node => node?.type === 'button');
  return { pending, installs, render, button, progress: event => progress(event), restarts: () => restarts };
}

test('Windows download reports bytes and cannot install until verification succeeds', async () => {
  const h = updaterHarness();
  h.button().props.onClick(); await settleRequests();
  h.button().props.onClick();
  h.progress({ event: 'Started', data: { contentLength: 4 * 1024 * 1024 } });
  h.progress({ event: 'Progress', data: { chunkLength: 1024 * 1024 } });
  assert.ok(nodes(h.render()).includes('Downloading… 25% · 1.0 MB / 4.0 MB'));
  const progress = nodes(h.render()).find(node => node?.type === 'progress');
  assert.equal(progress.props.value, 1024 * 1024);
  h.progress({ event: 'Finished' });
  assert.ok(nodes(h.render()).includes('Verifying update signature…'));
  assert.equal(h.button().props.disabled, true);
  h.button().props.onClick(); assert.deepEqual(h.installs, []);
  h.pending.resolve(); await settleRequests();
  assert.ok(nodes(h.button()).includes('Install and Restart'));
  h.button().props.onClick(); await settleRequests();
  assert.deepEqual(h.installs, [{ restartAfterInstall: true }]);
  assert.equal(h.restarts(), 0, 'Windows installer owns the restart');
});

test('unknown download size stays indeterminate and failed verification never enables installation', async () => {
  const h = updaterHarness();
  h.button().props.onClick(); await settleRequests(); h.button().props.onClick();
  h.progress({ event: 'Started', data: {} });
  h.progress({ event: 'Progress', data: { chunkLength: 1024 * 1024 } });
  assert.ok(nodes(h.render()).includes('Downloading… 1.0 MB'));
  assert.equal(nodes(h.render()).find(node => node?.type === 'progress').props.value, undefined);
  h.progress({ event: 'Finished' });
  h.pending.reject(new Error('invalid signature')); await settleRequests();
  assert.ok(nodes(h.render()).includes("Couldn't download the update. Try again."));
  assert.deepEqual(h.installs, []);
});

test('macOS installation still waits for its explicit restart action', async () => {
  const h = updaterHarness(false);
  h.button().props.onClick(); await settleRequests(); h.button().props.onClick();
  h.pending.resolve(); await settleRequests();
  assert.ok(nodes(h.button()).includes('Install Update'));
  h.button().props.onClick(); await settleRequests();
  assert.equal(h.restarts(), 0);
  assert.ok(nodes(h.button()).includes('Restart and Finish Update'));
  h.button().props.onClick(); await settleRequests();
  assert.equal(h.restarts(), 1);
});
