import assert from "node:assert/strict";
import test from "node:test";

import {
  assertReleaseVersions,
  cargoLockVersion,
  cargoPackageVersion,
  readReleaseVersions,
} from "./release-version.mjs";

test("repository release versions stay aligned", () => {
  const versions = readReleaseVersions();
  assert.equal(assertReleaseVersions(versions), versions.packageJson);
});

test("reads Cargo versions with LF or CRLF line endings", () => {
  const manifest = '[package]\nname = "kiri"\nversion = "1.4.0"\n\n[dependencies]\n';
  const lock = '[[package]]\nname = "kiri"\nversion = "1.4.0"\n\n[[package]]\nname = "serde"\nversion = "1.0.0"\n';
  for (const newline of ["\n", "\r\n"]) {
    assert.equal(cargoPackageVersion(manifest.replaceAll("\n", newline)), "1.4.0");
    assert.equal(cargoLockVersion(lock.replaceAll("\n", newline)), "1.4.0");
  }
});

test("accepts the matching release tag", () => {
  const versions = {
    packageJson: "1.4.0",
    cargoToml: "1.4.0",
    cargoLock: "1.4.0",
    tauriConfig: "1.4.0",
  };
  assert.equal(assertReleaseVersions(versions, "v1.4.0"), "1.4.0");
});

test("rejects mismatched source versions and tags", () => {
  const versions = {
    packageJson: "1.4.0",
    cargoToml: "1.4.0",
    cargoLock: "1.3.0",
    tauriConfig: "1.4.0",
  };
  assert.throws(() => assertReleaseVersions(versions), /do not match/);
  assert.throws(
    () => assertReleaseVersions({ ...versions, cargoLock: "1.4.0" }, "v1.3.0"),
    /does not match source version/,
  );
});
