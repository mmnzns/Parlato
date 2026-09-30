// Catalogue des modeles Whisper supportes.
//
// Reference VoiceInk : VoiceInk/Models/PredefinedModels.swift
// - Memes noms, memes tailles, meme source HuggingFace.

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct WhisperModelInfo {
    /// Identifiant stable (ex: "ggml-large-v3-turbo").
    pub id: &'static str,
    /// Libelle affiche a l'utilisateur.
    pub display_name: &'static str,
    /// Taille approximative en octets (pour barre de progression).
    pub size_bytes: u64,
    /// Vrai si le modele est multilingue, faux si anglais-seul (.en).
    pub multilingual: bool,
    /// URL HuggingFace de telechargement.
    pub url: &'static str,
    /// Commentaire indicatif sur les performances / cas d'usage.
    pub notes: &'static str,
    /// Note de vitesse de 0 a 1 (alignee VoiceInk TranscriptionModelRegistry.swift).
    pub speed: f32,
    /// Note de precision de 0 a 1 (idem VoiceInk).
    pub accuracy: f32,
    /// Codes ISO supportes par le modele. "auto" en tete = auto-detect.
    /// Multilingue -> WHISPER_FULL_LANGS, .en -> &["en"].
    pub language_codes: &'static [&'static str],
}

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

/// Catalogue strictement aligne sur VoiceInk TranscriptionModelRegistry.swift.
pub const WHISPER_MODELS: &[WhisperModelInfo] = &[
    WhisperModelInfo {
        id: "ggml-tiny",
        display_name: "Tiny (multilingue)",
        size_bytes: 75_000_000,
        multilingual: true,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.bin",
        notes: "Tres rapide, precision limitee. Bon pour tests.",
        speed: 0.95,
        accuracy: 0.6,
        language_codes: WHISPER_FULL_LANGS,
    },
    WhisperModelInfo {
        id: "ggml-tiny.en",
        display_name: "Tiny (anglais)",
        size_bytes: 75_000_000,
        multilingual: false,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.en.bin",
        notes: "Anglais uniquement, un peu meilleur que tiny multilingue sur EN.",
        speed: 0.95,
        accuracy: 0.65,
        language_codes: &["en"],
    },
    WhisperModelInfo {
        id: "ggml-base",
        display_name: "Base (multilingue)",
        size_bytes: 142_000_000,
        multilingual: true,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin",
        notes: "Bon compromis taille/precision pour CPU.",
        speed: 0.85,
        accuracy: 0.72,
        language_codes: WHISPER_FULL_LANGS,
    },
    WhisperModelInfo {
        id: "ggml-base.en",
        display_name: "Base (anglais)",
        size_bytes: 142_000_000,
        multilingual: false,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin",
        notes: "Anglais uniquement, meilleur que base multilingue sur EN.",
        speed: 0.85,
        accuracy: 0.75,
        language_codes: &["en"],
    },
    // Parlato : tailles Small et Medium, presentes dans le depot
    // ggerganov/whisper.cpp mais absentes du catalogue VoiceInk. Tailles
    // exactes relevees le 2026-09-29 (x-linked-size HuggingFace).
    WhisperModelInfo {
        id: "ggml-small",
        display_name: "Small (multilingue)",
        size_bytes: 487_601_967,
        multilingual: true,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin",
        notes: "Bon equilibre precision / vitesse en CPU.",
        speed: 0.7,
        accuracy: 0.84,
        language_codes: WHISPER_FULL_LANGS,
    },
    WhisperModelInfo {
        id: "ggml-small.en",
        display_name: "Small (anglais)",
        size_bytes: 487_614_201,
        multilingual: false,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.en.bin",
        notes: "Anglais uniquement, meilleur que small multilingue sur EN.",
        speed: 0.7,
        accuracy: 0.86,
        language_codes: &["en"],
    },
    WhisperModelInfo {
        id: "ggml-medium",
        display_name: "Medium (multilingue)",
        size_bytes: 1_533_763_059,
        multilingual: true,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium.bin",
        notes: "Tres bonne precision, plus lent que Large v3 Turbo.",
        speed: 0.45,
        accuracy: 0.9,
        language_codes: WHISPER_FULL_LANGS,
    },
    WhisperModelInfo {
        id: "ggml-medium.en",
        display_name: "Medium (anglais)",
        size_bytes: 1_533_774_781,
        multilingual: false,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium.en.bin",
        notes: "Anglais uniquement, tres bonne precision sur EN.",
        speed: 0.45,
        accuracy: 0.92,
        language_codes: &["en"],
    },
    WhisperModelInfo {
        id: "ggml-large-v2",
        display_name: "Large v2 (multilingue)",
        size_bytes: 2_900_000_000,
        multilingual: true,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v2.bin",
        notes: "Tres bonne precision, necessite beaucoup de RAM.",
        speed: 0.3,
        accuracy: 0.96,
        language_codes: WHISPER_FULL_LANGS,
    },
    WhisperModelInfo {
        id: "ggml-large-v3",
        display_name: "Large v3 (multilingue)",
        size_bytes: 2_900_000_000,
        multilingual: true,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3.bin",
        notes: "Derniere generation, precision maximale en CPU lourd.",
        speed: 0.3,
        accuracy: 0.98,
        language_codes: WHISPER_FULL_LANGS,
    },
    // Parlato : Large v3 quantise Q5_0 (depot ggerganov/whisper.cpp).
    WhisperModelInfo {
        id: "ggml-large-v3-q5_0",
        display_name: "Large v3 (Q5_0)",
        size_bytes: 1_081_140_203,
        multilingual: true,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-q5_0.bin",
        notes: "Large v3 quantise, precision proche pour un tiers de la taille.",
        speed: 0.35,
        accuracy: 0.97,
        language_codes: WHISPER_FULL_LANGS,
    },
    WhisperModelInfo {
        id: "ggml-large-v3-turbo",
        display_name: "Large v3 Turbo",
        size_bytes: 1_500_000_000,
        multilingual: true,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo.bin",
        notes: "Large v3 optimise pour la vitesse, recommande avec CUDA.",
        speed: 0.75,
        accuracy: 0.97,
        language_codes: WHISPER_FULL_LANGS,
    },
    WhisperModelInfo {
        id: "ggml-large-v3-turbo-q5_0",
        display_name: "Large v3 Turbo (Q5_0)",
        size_bytes: 547_000_000,
        multilingual: true,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin",
        notes: "Quantise, tres bon rapport qualite/taille pour CPU.",
        speed: 0.75,
        accuracy: 0.95,
        language_codes: WHISPER_FULL_LANGS,
    },
];

pub fn find_model(id: &str) -> Option<&'static WhisperModelInfo> {
    WHISPER_MODELS.iter().find(|m| m.id == id)
}
