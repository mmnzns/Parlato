// Table code ISO -> nom de langue affichable (en anglais).
//
// Reference VoiceInk : VoiceInk/Models/LanguageDictionary.swift `all`. On
// garde les memes ids et les memes libelles. Le code "auto" reste a part :
// il est traduit cote UI via i18n (`languageSelector.auto`) plutot qu'embarque
// ici, parce que la traduction depend de la langue de l'app.

export const LANGUAGE_NAMES: Record<string, string> = {
  af: "Afrikaans",
  am: "Amharic",
  ar: "Arabic",
  as: "Assamese",
  az: "Azerbaijani",
  ba: "Bashkir",
  be: "Belarusian",
  bg: "Bulgarian",
  bn: "Bengali",
  bo: "Tibetan",
  br: "Breton",
  bs: "Bosnian",
  ca: "Catalan",
  cs: "Czech",
  cy: "Welsh",
  da: "Danish",
  de: "German",
  el: "Greek",
  en: "English",
  en_au: "Australian English",
  en_uk: "British English",
  en_us: "US English",
  es: "Spanish",
  et: "Estonian",
  eu: "Basque",
  fa: "Persian",
  fi: "Finnish",
  fil: "Filipino",
  fo: "Faroese",
  fr: "French",
  ga: "Irish",
  gl: "Galician",
  gu: "Gujarati",
  ha: "Hausa",
  haw: "Hawaiian",
  he: "Hebrew",
  hi: "Hindi",
  hr: "Croatian",
  ht: "Haitian Creole",
  hu: "Hungarian",
  hy: "Armenian",
  id: "Indonesian",
  ig: "Igbo",
  is: "Icelandic",
  it: "Italian",
  ja: "Japanese",
  jw: "Javanese",
  ka: "Georgian",
  kk: "Kazakh",
  km: "Khmer",
  kn: "Kannada",
  ko: "Korean",
  ku: "Kurdish",
  ky: "Kyrgyz",
  la: "Latin",
  lb: "Luxembourgish",
  ln: "Lingala",
  lo: "Lao",
  lt: "Lithuanian",
  lv: "Latvian",
  mg: "Malagasy",
  mi: "Maori",
  mk: "Macedonian",
  ml: "Malayalam",
  mn: "Mongolian",
  mr: "Marathi",
  ms: "Malay",
  mt: "Maltese",
  my: "Myanmar",
  ne: "Nepali",
  nl: "Dutch",
  nn: "Norwegian Nynorsk",
  no: "Norwegian",
  oc: "Occitan",
  or: "Odia",
  pa: "Punjabi",
  pl: "Polish",
  ps: "Pashto",
  pt: "Portuguese",
  ro: "Romanian",
  ru: "Russian",
  sa: "Sanskrit",
  sd: "Sindhi",
  si: "Sinhala",
  sk: "Slovak",
  sl: "Slovenian",
  sn: "Shona",
  so: "Somali",
  sq: "Albanian",
  sr: "Serbian",
  su: "Sundanese",
  sv: "Swedish",
  sw: "Swahili",
  ta: "Tamil",
  te: "Telugu",
  tg: "Tajik",
  th: "Thai",
  tk: "Turkmen",
  tl: "Tagalog",
  tr: "Turkish",
  tt: "Tatar",
  uk: "Ukrainian",
  ur: "Urdu",
  uz: "Uzbek",
  vi: "Vietnamese",
  wo: "Wolof",
  xh: "Xhosa",
  yi: "Yiddish",
  yo: "Yoruba",
  yue: "Cantonese",
  zh: "Chinese",
  zu: "Zulu",
};

// Parlato : codes VoiceInk qui ne sont pas des balises BCP 47.
const BCP47: Record<string, string> = { en_au: "en-AU", en_uk: "en-GB", en_us: "en-US" };

/// Resolve un code ISO en libelle affichable. Renvoie le code en upper-case
/// si inconnu (defensive : permet d'afficher un code exotique sans casser l'UI).
/// Parlato : avec `uiLanguage`, le nom est donne dans la langue de l'app
/// ("anglais" dans l'app en francais) via Intl.DisplayNames, avec repli sur
/// la table anglaise.
export function languageLabel(code: string, uiLanguage?: string): string {
  const english = LANGUAGE_NAMES[code] ?? code.toUpperCase();
  if (!uiLanguage || uiLanguage.startsWith("en") || !LANGUAGE_NAMES[code]) return english;
  try {
    const names = new Intl.DisplayNames([uiLanguage], { type: "language", fallback: "none" });
    const name = names.of(BCP47[code] ?? code);
    if (!name) return english;
    return name.charAt(0).toLocaleUpperCase(uiLanguage) + name.slice(1);
  } catch {
    return english;
  }
}
