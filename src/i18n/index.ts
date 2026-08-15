/**
 * Kiri i18n — mirrors the Swift L10n behavior:
 *  - keys are the English strings themselves (English fallback)
 *  - follows the OS preferred language, only en + zh-Hans exist
 *  - the user's manual choice is persisted by the backend (language.json)
 *    and wins over the system locale
 *  - `%@` / `%d` placeholders like String(format:)
 */
import en from "./en.json";
import zhHans from "./zh-Hans.json";

export type KiriLanguage = "en" | "zh-Hans";

const DICTIONARIES: Record<KiriLanguage, Record<string, string>> = {
  en,
  "zh-Hans": zhHans,
};

function detectLanguage(): KiriLanguage {
  const locale = (navigator.language || "en").toLowerCase();
  // Match macOS zh-Hans preference and Windows zh-CN/zh-SG locales.
  if (/^zh-(hans|cn|sg)|zh-(hans|cn|sg)-/i.test(locale) || locale === "zh-hans") {
    return "zh-Hans";
  }
  return "en";
}

let language: KiriLanguage = detectLanguage();
const listeners = new Set<() => void>();

export function setLanguage(next: KiriLanguage) {
  if (language === next) return;
  language = next;
  // Persistence is handled by the backend (language.json) so the choice is
  // shared across windows and survives relaunches; this just updates the
  // in-memory state for the current window.
  listeners.forEach((listener) => listener());
}

/** Subscribe to language changes (returns an unsubscribe function). */
export function onLanguageChange(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/** Current language, for UI to render a switcher. */
export function getLanguage(): KiriLanguage {
  return language;
}

/** Look up a key; missing translations fall back to the key itself. */
export function t(key: string): string {
  return DICTIONARIES[language][key] ?? key;
}

/**
 * Swift-style formatting. `%@` consumes the next argument as a string,
 * `%d` formats the next argument as an integer.
 */
export function fmt(key: string, ...args: (string | number)[]): string {
  const template = t(key);
  let index = 0;
  return template.replace(/%[@d]/g, (token) => {
    const value = args[index++];
    if (value === undefined) return token;
    return token === "%d" ? String(Math.trunc(Number(value))) : String(value);
  });
}
