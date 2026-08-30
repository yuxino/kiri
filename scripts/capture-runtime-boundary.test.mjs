import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const rustRoot = join(repositoryRoot, "src-tauri", "src");
const removedFixtureSwitch = ["KIRI", "CAPTURE", "FIXTURE"].join("_");

function rustSources(directory) {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) return rustSources(path);
    return entry.isFile() && entry.name.endsWith(".rs") ? [path] : [];
  });
}

test("the interactive runtime cannot replace the desktop with a capture fixture", () => {
  const offenders = rustSources(rustRoot)
    .filter((path) => readFileSync(path, "utf8").includes(removedFixtureSwitch))
    .map((path) => path.slice(repositoryRoot.length + 1));

  assert.deepEqual(offenders, []);
});

test("the click ripple stays passive so recording hotkeys keep focus", () => {
  const commands = readFileSync(join(rustRoot, "commands.rs"), "utf8");
  const rippleBuilder = commands.match(
    /fn create_ripple_window\([\s\S]*?(?=\nstruct StartedRecorder)/,
  )?.[0];

  assert.ok(rippleBuilder, "create_ripple_window must remain present");
  assert.match(rippleBuilder, /\.focused\(false\)/);
  assert.match(rippleBuilder, /set_window_click_through\(app, "ripple"\)/);
});

test("capture confirmation defers closing its owner WebView until IPC returns", () => {
  const commands = readFileSync(join(rustRoot, "commands.rs"), "utf8");
  const confirmation = commands.match(
    /fn confirm_capture_inner\([\s\S]*?\n}\n\n(?=\/\/\/ Closing)/,
  )?.[0];

  assert.ok(confirmation, "confirm_capture_inner must remain present");
  assert.match(confirmation, /defer_capture_overlay_close\(app, &session\.overlay_labels\)/);
  assert.doesNotMatch(
    confirmation,
    /get_webview_window\(label\)[\s\S]{0,120}window\.close\(\)/,
    "the synchronous IPC callback must not directly destroy its owner WebView",
  );
});
