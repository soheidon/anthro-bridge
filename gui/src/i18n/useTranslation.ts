import { useContext } from "react";
import { LanguageContext } from "./LanguageContext";
import type { TranslationKey } from "./lang/en";

// Vite glob import — auto-discovers all .ts files in lang/
// Adding a new language file here makes it available automatically on next build.
const modules = import.meta.glob<{
  nativeName: string;
  translations: Record<string, string>;
}>("./lang/*.ts", { eager: true });

const ALL_TRANSLATIONS: Record<string, Record<string, string>> = {};
const LANGS_INTERNAL: { code: string; nativeName: string }[] = [];

for (const [path, mod] of Object.entries(modules)) {
  const code = (path.match(/lang[\/\\](.+)\.ts$/) ?? [])[1] ?? "";
  if (code && mod.translations) {
    ALL_TRANSLATIONS[code] = mod.translations;
    LANGS_INTERNAL.push({ code, nativeName: mod.nativeName ?? code });
  }
}

// Sort: EN first, JA second, then alphabetical by nativeName
LANGS_INTERNAL.sort((a, b) => {
  if (a.code === "en") return -1;
  if (b.code === "en") return 1;
  if (a.code === "ja") return -1;
  if (b.code === "ja") return 1;
  return a.nativeName.localeCompare(b.nativeName);
});

export const AVAILABLE_LANGS: ReadonlyArray<{ readonly code: string; readonly nativeName: string }> =
  LANGS_INTERNAL;

export type Lang = (typeof LANGS_INTERNAL)[number]["code"];

export function useTranslation() {
  const { lang } = useContext(LanguageContext);

  function t(key: TranslationKey, params?: Record<string, string | number>): string {
    // Try current language, fall back to English
    const langTranslations = ALL_TRANSLATIONS[lang];
    let text: string = langTranslations?.[key] ?? ALL_TRANSLATIONS["en"]?.[key] ?? key;
    if (params) {
      for (const [k, v] of Object.entries(params)) {
        text = text.replace(`{${k}}`, String(v));
      }
    }
    return text;
  }

  return { t, lang };
}
