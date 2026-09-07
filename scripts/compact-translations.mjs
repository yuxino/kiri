import path from "node:path";

// t() falls back to the English key. Keep the complete dictionaries in source
// for translators, but omit identical values from the shipped lookup tables.
export function compactTranslations(dictionary) {
  return Object.fromEntries(
    Object.entries(dictionary).filter(([key, value]) => key !== value),
  );
}

export function compactTranslationsPlugin(root) {
  const dictionaries = new Set(
    ["en", "zh-Hans", "ja"].map((language) =>
      path.resolve(root, "src/i18n", `${language}.json`).replaceAll("\\", "/"),
    ),
  );
  return {
    name: "kiri-compact-translations",
    apply: "build",
    enforce: "pre",
    transform(source, id) {
      if (!dictionaries.has(id.replaceAll("\\", "/"))) return null;
      // Return JSON so Vite's normal JSON loader still owns module generation.
      return { code: JSON.stringify(compactTranslations(JSON.parse(source))), map: null };
    },
  };
}
