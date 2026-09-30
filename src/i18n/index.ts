// i18n setup - react-i18next with browser language detection.
// Supported languages: English (fallback), French, Spanish.
//
// The user language is detected in this order: manual override in localStorage,
// browser navigator, app default (en). It's persisted in localStorage under
// the key `parla_language`.

import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import LanguageDetector from "i18next-browser-languagedetector";

import en from "./locales/en.json";
import fr from "./locales/fr.json";
import es from "./locales/es.json";
import { isMac } from "@/lib/platform";

// Parlato: each locale file has a `macOverrides` block holding the strings
// that name Windows keys (Right Alt, Ctrl, Win). On macOS they replace the
// base strings, so screens keep calling the same keys everywhere.
type Tree = { [key: string]: string | Tree };

function withMacOverrides(base: Tree): Tree {
  const { macOverrides, ...rest } = base;
  if (!isMac || !macOverrides || typeof macOverrides === "string") return rest;
  const merge = (target: Tree, over: Tree): Tree => {
    const out: Tree = { ...target };
    for (const [k, v] of Object.entries(over)) {
      const cur = out[k];
      out[k] = typeof v === "string" || typeof cur !== "object" ? v : merge(cur, v);
    }
    return out;
  };
  return merge(rest, macOverrides);
}

export const SUPPORTED_LANGUAGES = ["en", "fr", "es"] as const;
export type SupportedLanguage = (typeof SUPPORTED_LANGUAGES)[number];

export const LANGUAGE_LABELS: Record<SupportedLanguage, string> = {
  en: "English",
  fr: "Francais",
  es: "Espanol",
};

i18n
  .use(LanguageDetector)
  .use(initReactI18next)
  .init({
    resources: {
      en: { translation: withMacOverrides(en) },
      fr: { translation: withMacOverrides(fr) },
      es: { translation: withMacOverrides(es) },
    },
    fallbackLng: "en",
    supportedLngs: SUPPORTED_LANGUAGES,
    interpolation: {
      escapeValue: false,
    },
    detection: {
      order: ["localStorage", "navigator"],
      lookupLocalStorage: "parla_language",
      caches: ["localStorage"],
    },
  });

export default i18n;
