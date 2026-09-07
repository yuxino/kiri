import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { compactTranslations, compactTranslationsPlugin } from "./compact-translations.mjs";

test("compacted dictionaries preserve every translation and English alias", () => {
  let canonicalKeys;
  for (const language of ["en", "zh-Hans", "ja"]) {
    const dictionary = JSON.parse(readFileSync(new URL(`../src/i18n/${language}.json`, import.meta.url)));
    const keys = Object.keys(dictionary).sort();
    canonicalKeys ??= keys;
    assert.deepEqual(keys, canonicalKeys);
    const compact = compactTranslations(dictionary);
    for (const [key, value] of Object.entries(dictionary)) {
      assert.equal(compact[key] ?? key, value, `${language}: ${key}`);
    }
    assert.ok(JSON.stringify(compact).length < JSON.stringify(dictionary).length);
  }
  assert.deepEqual(compactTranslations({ Same: "Same", Alias: "Different", Empty: "" }), {
    Alias: "Different", Empty: "",
  });
});

test("build compaction touches only the three canonical dictionaries", () => {
  const root = path.resolve("/project");
  const plugin = compactTranslationsPlugin(root);
  assert.equal(plugin.apply, "build");
  assert.equal(plugin.transform('{"Same":"Same"}', path.join(root, "src/i18n/en.json")).code, "{}");
  for (const id of [path.join(root, "other/en.json"), path.join(root, "src/i18n/en.json?raw")]) {
    assert.equal(plugin.transform("not JSON", id), null);
  }
});
