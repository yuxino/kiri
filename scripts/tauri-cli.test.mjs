import assert from "node:assert/strict";
import test from "node:test";
import path from "node:path";

import {
  cargoRunnerEnvironmentKey,
  macosDevEnvironment,
  parseRustcHost,
  tauriCommand,
  tauriTarget,
} from "./tauri-cli.mjs";

test("finds the Tauri subcommand after global flags", () => {
  assert.equal(tauriCommand(["dev", "--no-watch"]), "dev");
  assert.equal(tauriCommand(["--verbose", "dev"]), "dev");
  assert.equal(tauriCommand(["build", "--debug"]), "build");
});

test("turns a Rust host triple into Cargo's runner environment key", () => {
  assert.equal(
    cargoRunnerEnvironmentKey("aarch64-apple-darwin"),
    "CARGO_TARGET_AARCH64_APPLE_DARWIN_RUNNER",
  );
});

test("uses an explicit Tauri target when cross-compiling", () => {
  assert.equal(
    tauriTarget(["dev", "--target", "x86_64-apple-darwin"]),
    "x86_64-apple-darwin",
  );
  assert.equal(
    tauriTarget(["dev", "--target=aarch64-apple-darwin"]),
    "aarch64-apple-darwin",
  );
  assert.equal(
    tauriTarget(["dev", "--", "--target", "runner-argument"]),
    undefined,
  );
  assert.equal(tauriTarget(["dev", "--no-watch"]), undefined);
});

test("extracts the host triple from rustc verbose version output", () => {
  assert.equal(
    parseRustcHost("rustc 1.95.0\nhost: aarch64-apple-darwin\nLLVM version: 21.1.8\n"),
    "aarch64-apple-darwin",
  );
  assert.throws(() => parseRustcHost("rustc 1.95.0\n"), /determine the Rust host/);
});

test("adds only the stable macOS dev runner to a copied environment", () => {
  const original = { PATH: "/usr/bin", KEEP: "yes" };
  const result = macosDevEnvironment(
    original,
    "x86_64-apple-darwin",
    "/repo/scripts",
  );

  assert.deepEqual(original, { PATH: "/usr/bin", KEEP: "yes" });
  assert.equal(result.KEEP, "yes");
  assert.equal(result.PATH, ["/repo/scripts", "/usr/bin"].join(path.delimiter));
  assert.equal(
    result.CARGO_TARGET_X86_64_APPLE_DARWIN_RUNNER,
    "kiri-macos-dev-runner",
  );
});
