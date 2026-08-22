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
