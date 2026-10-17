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
    /// Parlato : figee sur un commit du depot (pas "main"), pour que le
    /// fichier ne puisse pas changer sous nos pieds.
    pub url: &'static str,
    /// Parlato : SHA-256 du fichier (oid LFS Hugging Face), verifie apres
    /// le telechargement.
    pub sha256: &'static str,
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
/// Parlato : URLs figees sur le commit 5359861 de ggerganov/whisper.cpp
/// (2024-10-29, dernier commit au 2026-10-16), empreintes relevees via
/// l'API Hugging Face (`/api/models/<depot>/tree/<commit>`, `lfs.oid`).
pub const WHISPER_MODELS: &[WhisperModelInfo] = &[
    WhisperModelInfo {
        id: "ggml-tiny",
        display_name: "Tiny (multilingue)",
        size_bytes: 75_000_000,
        multilingual: true,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/5359861c739e955e79d9a303bcbc70fb988958b1/ggml-tiny.bin",
        sha256: "be07e048e1e599ad46341c8d2a135645097a538221678b7acdd1b1919c6e1b21",
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
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/5359861c739e955e79d9a303bcbc70fb988958b1/ggml-tiny.en.bin",
        sha256: "921e4cf8686fdd993dcd081a5da5b6c365bfde1162e72b08d75ac75289920b1f",
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
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/5359861c739e955e79d9a303bcbc70fb988958b1/ggml-base.bin",
        sha256: "60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe",
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
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/5359861c739e955e79d9a303bcbc70fb988958b1/ggml-base.en.bin",
        sha256: "a03779c86df3323075f5e796cb2ce5029f00ec8869eee3fdfb897afe36c6d002",
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
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/5359861c739e955e79d9a303bcbc70fb988958b1/ggml-small.bin",
        sha256: "1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b",
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
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/5359861c739e955e79d9a303bcbc70fb988958b1/ggml-small.en.bin",
        sha256: "c6138d6d58ecc8322097e0f987c32f1be8bb0a18532a3f88f734d1bbf9c41e5d",
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
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/5359861c739e955e79d9a303bcbc70fb988958b1/ggml-medium.bin",
        sha256: "6c14d5adee5f86394037b4e4e8b59f1673b6cee10e3cf0b11bbdbee79c156208",
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
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/5359861c739e955e79d9a303bcbc70fb988958b1/ggml-medium.en.bin",
        sha256: "cc37e93478338ec7700281a7ac30a10128929eb8f427dda2e865faa8f6da4356",
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
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/5359861c739e955e79d9a303bcbc70fb988958b1/ggml-large-v2.bin",
        sha256: "9a423fe4d40c82774b6af34115b8b935f34152246eb19e80e376071d3f999487",
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
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/5359861c739e955e79d9a303bcbc70fb988958b1/ggml-large-v3.bin",
        sha256: "64d182b440b98d5203c4f9bd541544d84c605196c4f7b845dfa11fb23594d1e2",
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
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/5359861c739e955e79d9a303bcbc70fb988958b1/ggml-large-v3-q5_0.bin",
        sha256: "d75795ecff3f83b5faa89d1900604ad8c780abd5739fae406de19f23ecd98ad1",
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
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/5359861c739e955e79d9a303bcbc70fb988958b1/ggml-large-v3-turbo.bin",
        sha256: "1fc70f774d38eb169993ac391eea357ef47c88757ef72ee5943879b7e8e2bc69",
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
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/5359861c739e955e79d9a303bcbc70fb988958b1/ggml-large-v3-turbo-q5_0.bin",
        sha256: "394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2",
        notes: "Quantise, tres bon rapport qualite/taille pour CPU.",
        speed: 0.75,
        accuracy: 0.95,
        language_codes: WHISPER_FULL_LANGS,
    },
];

pub fn find_model(id: &str) -> Option<&'static WhisperModelInfo> {
    WHISPER_MODELS.iter().find(|m| m.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_is_pinned_with_checksums() {
        for m in WHISPER_MODELS {
            crate::download::assert_pinned(m.url, m.sha256);
            assert!(m.url.ends_with(&format!("/{}.bin", m.id)), "{}", m.id);
        }
    }
}
