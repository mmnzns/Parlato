// Model ids: Parlato's pre-0.15 ids and the engine catalog's ids.
//
// From voice-engine v0.2.2 on, every model is known by the engine catalog's
// id ("whisper-turbo", "parakeet-ultra", "granite-4.2-3b"). Settings and
// Power Mode rows saved by older versions hold Parlato's own ids; this table
// rewrites them once at startup (plan Decision 2). It is written from the ids
// themselves, not imported from the engine's internals. Imported Whisper
// files keep their `imported:<stem>` ids.

/// `(old Parlato id, engine catalog id)`.
pub const LEGACY_IDS: &[(&str, &str)] = &[
    // Whisper (13).
    ("ggml-tiny", "whisper-tiny"),
    ("ggml-tiny.en", "whisper-tiny-en"),
    ("ggml-base", "whisper-base"),
    ("ggml-base.en", "whisper-base-en"),
    ("ggml-small", "whisper-small"),
    ("ggml-small.en", "whisper-small-en"),
    ("ggml-medium", "whisper-medium"),
    ("ggml-medium.en", "whisper-medium-en"),
    ("ggml-large-v2", "whisper-large-v2-full"),
    ("ggml-large-v3", "whisper-large-v3-full"),
    ("ggml-large-v3-q5_0", "whisper-large-v3"),
    ("ggml-large-v3-turbo", "whisper-turbo-full"),
    ("ggml-large-v3-turbo-q5_0", "whisper-turbo"),
    // Parakeet family (8).
    ("parakeet-tdt-0.6b-v2", "parakeet-v2-full"),
    ("parakeet-tdt-0.6b-v2-int8", "parakeet-v2"),
    ("parakeet-tdt-0.6b-v3", "parakeet-v3-full"),
    ("parakeet-tdt-0.6b-v3-int8", "parakeet-v3"),
    ("parakeet-ultra-0.6b-int8", "parakeet-ultra"),
    ("parakeet-unified-en-0.6b", "parakeet-unified-en-full"),
    ("parakeet-unified-en-0.6b-int8", "parakeet-unified-en"),
    ("nemotron-3.5-asr-streaming-0.6b", "nemotron-3.5"),
    // Local AI: the one catalog model Parlato already had.
    ("granite-4.2-3b-q4", "granite-4.2-3b"),
];

/// Local AI ids that left the catalog. A saved choice of one of them becomes
/// the import id of its file, when the file is still there.
pub const DROPPED_GGUF_IDS: &[&str] = &[
    "qwen2.5-3b-instruct-q4",
    "llama-3.2-3b-instruct-q4",
    "gemma-2-2b-it-q4",
    "phi-3.5-mini-instruct-q4",
];

/// The catalog id for an old Parlato id, or None when `id` is not an old id
/// (already a catalog id, an `imported:` id, empty...).
pub fn migrate_id(id: &str) -> Option<&'static str> {
    LEGACY_IDS
        .iter()
        .find(|(old, _)| *old == id)
        .map(|(_, new)| *new)
}

/// The old Parlato id of a catalog model, for the copy an older Parlato left.
pub fn legacy_id(new_id: &str) -> Option<&'static str> {
    LEGACY_IDS
        .iter()
        .find(|(_, new)| *new == new_id)
        .map(|(old, _)| *old)
}

/// The history label of a dictation ("local / <id>", "parakeet / <id>") with
/// an old Parlato id replaced by the catalog id, so rows saved before the
/// move and rows saved after it read and add up as one model. The stored row
/// is never changed; this is for display and for the model performance page.
pub fn display_label(label: &str) -> String {
    for prefix in ["local / ", "parakeet / "] {
        if let Some(id) = label.strip_prefix(prefix) {
            if let Some(new) = migrate_id(id) {
                return format!("{prefix}{new}");
            }
        }
    }
    label.to_string()
}

/// What to do with the saved local AI choice (`llm_selected_gguf`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GgufChoice {
    /// Leave it as it is.
    Keep,
    /// Replace it.
    Set(String),
    /// The file it pointed at is gone: no choice.
    Clear,
}

/// Migrates a saved local AI id. `import_id_of_stem` gives the import id of
/// the file named `<stem>.gguf` in `LlmModels/` or `LlmModels/imported/`, or
/// None when there is no such file.
pub fn migrate_gguf_choice(
    id: &str,
    import_id_of_stem: &dyn Fn(&str) -> Option<String>,
) -> GgufChoice {
    if let Some(new) = migrate_id(id) {
        return GgufChoice::Set(new.to_string());
    }
    let stem = if DROPPED_GGUF_IDS.contains(&id) {
        id
    } else if let Some(stem) = id.strip_prefix("imported:") {
        stem
    } else {
        return GgufChoice::Keep;
    };
    match import_id_of_stem(stem) {
        Some(import) => GgufChoice::Set(import),
        None => GgufChoice::Clear,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use voice_engine_models::catalog;
    use voice_engine_models::Kind;

    #[test]
    fn every_old_id_maps_to_a_catalog_model() {
        assert_eq!(LEGACY_IDS.len(), 22);
        for (old, new) in LEGACY_IDS {
            assert!(
                catalog::find(new).is_some(),
                "{old} -> {new} is not in the catalog"
            );
            assert!(catalog::find(old).is_none(), "{old} is also a catalog id");
        }
    }

    #[test]
    fn new_ids_are_unique() {
        let mut ids: Vec<_> = LEGACY_IDS.iter().map(|(_, n)| *n).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), LEGACY_IDS.len());
    }

    #[test]
    fn counts_per_family() {
        let kind = |new: &str| catalog::find(new).unwrap().kind;
        let whisper = LEGACY_IDS
            .iter()
            .filter(|(_, n)| kind(n) == Kind::Whisper)
            .count();
        let parakeet = LEGACY_IDS
            .iter()
            .filter(|(_, n)| {
                matches!(
                    kind(n),
                    Kind::Parakeet | Kind::ParakeetUnified | Kind::Nemotron
                )
            })
            .count();
        let llm = LEGACY_IDS
            .iter()
            .filter(|(_, n)| kind(n) == Kind::Llm)
            .count();
        assert_eq!((whisper, parakeet, llm), (13, 8, 1));
    }

    #[test]
    fn each_whisper_id_maps_to_the_model_with_the_same_file() {
        // Parlato stored Models/<old id>.bin: the catalog file must be that
        // file, or the runtime and the one-time move would disagree.
        for (old, new) in LEGACY_IDS.iter().filter(|(o, _)| o.starts_with("ggml-")) {
            let model = catalog::find(new).unwrap();
            assert_eq!(model.kind, Kind::Whisper);
            assert_eq!(model.files.len(), 1);
            assert_eq!(model.files[0].name, format!("{old}.bin"), "{old} -> {new}");
        }
    }

    #[test]
    fn spot_checks() {
        assert_eq!(
            migrate_id("ggml-large-v3-turbo-q5_0"),
            Some("whisper-turbo")
        );
        assert_eq!(
            migrate_id("ggml-large-v3-turbo"),
            Some("whisper-turbo-full")
        );
        assert_eq!(migrate_id("ggml-large-v3-q5_0"), Some("whisper-large-v3"));
        assert_eq!(
            migrate_id("parakeet-ultra-0.6b-int8"),
            Some("parakeet-ultra")
        );
        assert_eq!(
            migrate_id("nemotron-3.5-asr-streaming-0.6b"),
            Some("nemotron-3.5")
        );
        assert_eq!(migrate_id("granite-4.2-3b-q4"), Some("granite-4.2-3b"));
    }

    #[test]
    fn current_ids_and_imports_are_left_alone() {
        for id in [
            "whisper-turbo",
            "parakeet-v3",
            "imported:my-model",
            "",
            "something-else",
        ] {
            assert_eq!(migrate_id(id), None, "{id}");
        }
    }

    #[test]
    fn migration_is_idempotent() {
        for (_, new) in LEGACY_IDS {
            assert_eq!(migrate_id(new), None);
        }
    }

    #[test]
    fn legacy_id_is_the_reverse() {
        for (old, new) in LEGACY_IDS {
            assert_eq!(legacy_id(new), Some(*old));
        }
        assert_eq!(legacy_id("vad"), None);
    }

    #[test]
    fn history_labels_show_the_catalog_id() {
        assert_eq!(
            display_label("local / ggml-large-v3-turbo-q5_0"),
            "local / whisper-turbo"
        );
        assert_eq!(
            display_label("parakeet / parakeet-ultra-0.6b-int8"),
            "parakeet / parakeet-ultra"
        );
        assert_eq!(
            display_label("local / whisper-turbo"),
            "local / whisper-turbo"
        );
        assert_eq!(
            display_label("local / imported:foo"),
            "local / imported:foo"
        );
        assert_eq!(display_label("groq / ggml-base"), "groq / ggml-base");
        assert_eq!(display_label(""), "");
    }

    #[test]
    fn gguf_choice_catalog_id_is_renamed() {
        let none = |_: &str| None;
        assert_eq!(
            migrate_gguf_choice("granite-4.2-3b-q4", &none),
            GgufChoice::Set("granite-4.2-3b".into())
        );
    }

    #[test]
    fn gguf_choice_dropped_ids_become_the_import_id() {
        let find = |stem: &str| Some(format!("import-of-{stem}"));
        for id in DROPPED_GGUF_IDS {
            assert_eq!(
                migrate_gguf_choice(id, &find),
                GgufChoice::Set(format!("import-of-{id}"))
            );
        }
        assert_eq!(
            migrate_gguf_choice("imported:my-model", &find),
            GgufChoice::Set("import-of-my-model".into())
        );
    }

    #[test]
    fn gguf_choice_without_a_file_is_cleared() {
        let none = |_: &str| None;
        assert_eq!(
            migrate_gguf_choice("gemma-2-2b-it-q4", &none),
            GgufChoice::Clear
        );
        assert_eq!(
            migrate_gguf_choice("imported:gone", &none),
            GgufChoice::Clear
        );
    }

    #[test]
    fn gguf_choice_current_ids_are_kept() {
        let find = |_: &str| Some("x".to_string());
        assert_eq!(
            migrate_gguf_choice("granite-4.2-3b", &find),
            GgufChoice::Keep
        );
        assert_eq!(migrate_gguf_choice("qwen3.5-4b", &find), GgufChoice::Keep);
        assert_eq!(
            migrate_gguf_choice("import-0123456789ab", &find),
            GgufChoice::Keep
        );
    }
}
