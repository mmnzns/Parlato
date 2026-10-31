// Language codes offered per model family. The engine catalog only carries a
// plain "99 languages" text, the language pickers need the codes ("auto"
// first = detect). Moved here from the old Whisper and Parakeet catalogs.

/// 99 langues supportees par whisper.cpp + "auto" en tete pour la detection
/// automatique. Source : whisper.cpp `whisper_lang_id` table, trie alpha.
pub const WHISPER_FULL_LANGS: &[&str] = &[
    "auto", "af", "am", "ar", "as", "az", "ba", "be", "bg", "bn", "bo", "br",
    "bs", "ca", "cs", "cy", "da", "de", "el", "en", "es", "et", "eu", "fa",
    "fi", "fo", "fr", "gl", "gu", "ha", "haw", "he", "hi", "hr", "ht", "hu",
    "hy", "id", "is", "it", "ja", "jw", "ka", "kk", "km", "kn", "ko", "la",
    "lb", "ln", "lo", "lt", "lv", "mg", "mi", "mk", "ml", "mn", "mr", "ms",
    "mt", "my", "ne", "nl", "nn", "no", "oc", "pa", "pl", "ps", "pt", "ro",
    "ru", "sa", "sd", "si", "sk", "sl", "sn", "so", "sq", "sr", "su", "sv",
    "sw", "ta", "te", "tg", "th", "tk", "tl", "tr", "tt", "uk", "ur", "uz",
    "vi", "yi", "yo", "yue", "zh",
];

/// 25 langues europeennes supportees par Parakeet TDT v3 + "auto" en tete.
/// Reference VoiceInk LanguageDictionary.forProvider(.fluidAudio).
pub const PARAKEET_V3_LANGS: &[&str] = &[
    "auto", "bg", "cs", "da", "de", "el", "en", "es", "et", "fi", "fr", "hr",
    "hu", "it", "lt", "lv", "mt", "nl", "pl", "pt", "ro", "ru", "sk", "sl",
    "sv", "uk",
];

/// Langues de la fiche NVIDIA de Nemotron 3.5 ASR, "auto" en tete.
pub const NEMOTRON_LANGS: &[&str] = &[
    "auto", "ar", "bg", "cs", "da", "de", "el", "en", "es", "et", "fi", "fr",
    "he", "hi", "hr", "hu", "it", "ja", "ko", "lt", "lv", "mt", "nl", "no",
    "pl", "pt", "ro", "ru", "sk", "sl", "sv", "th", "tr", "uk", "vi", "zh",
];
