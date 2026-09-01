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

test("capture confirmation finalizes only after its owner IPC returns", () => {
  const commands = readFileSync(join(rustRoot, "commands.rs"), "utf8").replace(/\r\n?/g, "\n");
  const overlay = readFileSync(
    join(repositoryRoot, "src", "windows", "OverlayWindow.tsx"),
    "utf8",
  );

  const confirmation = commands.match(
    /fn confirm_capture_inner\([\s\S]*?(?=\npub\(crate\) fn finalize_confirmed_capture_after_overlay_destroyed)/,
  )?.[0];
  assert.ok(confirmation, "confirm_capture_inner must remain present");
  assert.doesNotMatch(
    confirmation,
    /defer_capture_overlay_close|window\.close\(\)|show_completion_preview/,
    "the synchronous backend callback must not destroy its owner or create feedback WebViews",
  );
  assert.match(
    commands,
    /pending_capture_completion[\s\S]*PendingCaptureCompletion/,
    "successful capture feedback must wait for overlay destruction",
  );
  assert.match(
    commands,
    /fn finalize_confirmed_capture_after_overlay_destroyed[\s\S]*show_completion_preview/,
    "completion preview creation must run from the overlay destruction path",
  );
  const confirmationAwait = overlay.indexOf("await api.confirmCapture");
  const ownerCloseAwait = overlay.indexOf(
    "await getCurrentWindow().close()",
    confirmationAwait,
  );
  assert.ok(confirmationAwait >= 0, "the overlay must await capture confirmation");
  assert.ok(
    ownerCloseAwait > confirmationAwait,
    "the overlay must wait for the confirmation response before closing itself",
  );
  assert.doesNotMatch(
    overlay.slice(confirmationAwait, ownerCloseAwait),
    /\bcatch\b|\bfinally\b/,
    "the successful confirmation path must close before leaving its try block",
  );
});
