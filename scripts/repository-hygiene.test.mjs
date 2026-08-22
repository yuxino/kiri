import assert from "node:assert/strict";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");

function filesUnder(directory, extensions) {
  if (!existsSync(directory)) return [];
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const candidate = join(directory, entry.name);
    if (entry.isDirectory()) return filesUnder(candidate, extensions);
    return entry.isFile() && extensions.some((extension) => entry.name.endsWith(extension))
      ? [candidate]
      : [];
  });
}

test("the repository has one canonical Tauri and Cargo project", () => {
  for (const obsoletePath of ["Cargo.toml", "Cargo.lock", "crates", "tauri-app"]) {
    assert.equal(
      existsSync(join(repositoryRoot, obsoletePath)),
      false,
      `${obsoletePath} would shadow or duplicate the canonical src-tauri project`,
    );
  }
});

test("completed migration material stays in Git history", () => {
  for (const obsoletePath of ["docs/plans", "docs/spec"]) {
    assert.equal(
      existsSync(join(repositoryRoot, obsoletePath)),
      false,
      `${obsoletePath} must not act as a competing source of truth`,
    );
  }
});

test("current Markdown links resolve inside the repository", () => {
  const topLevelMarkdown = readdirSync(repositoryRoot, { withFileTypes: true })
    .filter((entry) => entry.isFile() && entry.name.endsWith(".md"))
    .map((entry) => join(repositoryRoot, entry.name));
  const markdownFiles = [
    ...topLevelMarkdown,
    ...filesUnder(join(repositoryRoot, "docs"), [".md"]),
  ];
  const broken = [];
  for (const markdownPath of markdownFiles) {
    const markdown = readFileSync(markdownPath, "utf8");
    for (const match of markdown.matchAll(/\[[^\]]*\]\(([^)]+)\)/g)) {
      const destination = match[1].replace(/^<|>$/g, "").split("#", 1)[0];
      if (!destination || /^(?:[a-z]+:|\/)/i.test(destination)) continue;
      const resolved = resolve(dirname(markdownPath), decodeURIComponent(destination));
      if (!existsSync(resolved)) {
        broken.push(`${relative(repositoryRoot, markdownPath)} -> ${destination}`);
      }
    }
  }
  assert.deepEqual(broken, []);
});

test("translation dictionaries stay aligned and contain no orphaned keys", () => {
  const languages = ["en", "zh-Hans", "ja"];
  const dictionaries = languages.map((language) =>
    JSON.parse(readFileSync(join(repositoryRoot, "src", "i18n", `${language}.json`), "utf8")),
  );
  const englishKeys = Object.keys(dictionaries[0]);
  for (let index = 1; index < dictionaries.length; index += 1) {
    assert.deepEqual(Object.keys(dictionaries[index]), englishKeys, `${languages[index]} keys differ`);
  }

  const sourceFiles = [
    ...filesUnder(join(repositoryRoot, "src"), [".ts", ".tsx"]),
    ...filesUnder(join(repositoryRoot, "src-tauri", "src"), [".rs"]),
  ];
  const source = sourceFiles.map((path) => readFileSync(path, "utf8")).join("\n");
  const orphaned = englishKeys.filter((key) => !source.includes(key));
  assert.deepEqual(orphaned, [], `orphaned translation keys: ${orphaned.join(", ")}`);

  assert.ok(sourceFiles.every((path) => !relative(repositoryRoot, path).startsWith("..")));
});

test("CSS custom properties are defined once they are used and are not orphaned", () => {
  const cssFiles = filesUnder(join(repositoryRoot, "src"), [".css"]);
  const css = cssFiles.map((path) => readFileSync(path, "utf8")).join("\n");
  const declared = new Set(
    [...css.matchAll(/^\s*--([\w-]+)\s*:/gm)].map((match) => match[1]),
  );
  const used = new Set([...css.matchAll(/var\(--([\w-]+)/g)].map((match) => match[1]));
  assert.deepEqual([...used].filter((name) => !declared.has(name)), [], "undefined CSS variables");
  assert.deepEqual([...declared].filter((name) => !used.has(name)), [], "orphaned CSS variables");
});
