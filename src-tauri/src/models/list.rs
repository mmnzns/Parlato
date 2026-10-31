// What the Models screens list: the engine catalog joined with where each
// model is on this computer (`SharedStore::status`), plus Parlato's imports.
//
// The payloads keep the fields the UI already reads (Whisper, Parakeet, local
// AI and voice detection); the few new ones are additions.

use std::path::Path;

use serde::Serialize;
use voice_engine_models::{catalog, Kind, ModelInfo, Status};

use super::imports::{self, ImportedFile};
use super::store::ModelStore;
use crate::transcription::langs::{NEMOTRON_LANGS, PARAKEET_V3_LANGS, WHISPER_FULL_LANGS};

const WHISPER_IMPORT_NOTES: &str = "Modele Whisper GGML importe par l'utilisateur";
const GGUF_IMPORT_NOTES: &str = "Modele GGUF importe par l'utilisateur";

#[derive(Debug, Clone, Serialize)]
pub struct WhisperModelState {
    pub id: String,
    pub display_name: String,
    pub size_bytes: u64,
    pub multilingual: bool,
    pub notes: String,
    pub downloaded: bool,
    pub on_disk_bytes: Option<u64>,
    pub path: Option<String>,
    /// True for a model the person imported (outside the catalog).
    pub imported: bool,
    /// 0 to 1 (0 = unknown, for imports).
    pub speed: f32,
    pub accuracy: f32,
    pub language_codes: Vec<String>,
    /// Another app is downloading this model right now.
    pub downloading_elsewhere: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParakeetModelState {
    pub id: String,
    pub display_name: String,
    pub multilingual: bool,
    pub is_quantized: bool,
    pub size_bytes: u64,
    pub notes: String,
    pub downloaded: bool,
    pub missing_files: Vec<String>,
    pub on_disk_bytes: Option<u64>,
    pub path: Option<String>,
    pub speed: f32,
    pub accuracy: f32,
    pub language_codes: Vec<String>,
    pub downloading_elsewhere: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct GgufModelState {
    pub id: String,
    pub display_name: String,
    pub size_bytes: u64,
    pub context_length: u32,
    pub notes: String,
    pub downloaded: bool,
    pub on_disk_bytes: Option<u64>,
    pub path: Option<String>,
    pub imported: bool,
    /// Delete removes the file. False for an import that lives outside
    /// `LlmModels/imported/`: Parlato leaves it where the person put it.
    pub removable: bool,
    pub downloading_elsewhere: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct VadModelState {
    pub downloaded: bool,
    pub path: Option<String>,
    pub on_disk_bytes: Option<u64>,
    pub downloading_elsewhere: bool,
}

/// The catalog's 1 to 3 dots as the 0 to 1 the UI draws.
fn rating(dots: u8) -> f32 {
    f32::from(dots.min(3)) / 3.0
}

fn path_string(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

fn is_elsewhere(status: &Status) -> bool {
    matches!(status, Status::Downloading { by_me: false, .. })
}

pub fn whisper_languages(model: &ModelInfo) -> Vec<String> {
    if model.english_only {
        vec!["en".into()]
    } else {
        WHISPER_FULL_LANGS.iter().map(|s| s.to_string()).collect()
    }
}

fn parakeet_languages(model: &ModelInfo) -> &'static [&'static str] {
    match model.kind {
        Kind::Nemotron => NEMOTRON_LANGS,
        _ if model.english_only => &["en"],
        _ => PARAKEET_V3_LANGS,
    }
}

pub fn whisper_models(store: &ModelStore) -> Vec<WhisperModelState> {
    let mut out: Vec<WhisperModelState> = store
        .shared()
        .list()
        .into_iter()
        .filter(|(m, _)| m.kind == Kind::Whisper)
        .map(|(m, status)| {
            let installed = match &status {
                Status::Installed { dir, .. } => Some(ModelStore::installed_path(m, dir)),
                _ => None,
            };
            WhisperModelState {
                id: m.id.into(),
                display_name: m.name.into(),
                size_bytes: m.size(),
                multilingual: !m.english_only,
                notes: m.note.into(),
                downloaded: installed.is_some(),
                on_disk_bytes: installed.as_ref().map(|_| m.size()),
                path: installed.as_deref().map(path_string),
                imported: false,
                speed: rating(m.speed),
                accuracy: rating(m.accuracy),
                language_codes: whisper_languages(m),
                downloading_elsewhere: is_elsewhere(&status),
            }
        })
        .collect();
    out.extend(
        imports::list_whisper_imports(&store.whisper_dir())
            .into_iter()
            .map(whisper_import_state),
    );
    out
}

fn whisper_import_state(f: ImportedFile) -> WhisperModelState {
    WhisperModelState {
        id: f.id,
        display_name: format!("{} (importe)", f.stem),
        size_bytes: f.size,
        // A file the person brought: assume multilingual, no ratings.
        multilingual: true,
        notes: WHISPER_IMPORT_NOTES.into(),
        downloaded: true,
        on_disk_bytes: Some(f.size),
        path: Some(path_string(&f.path)),
        imported: true,
        speed: 0.0,
        accuracy: 0.0,
        language_codes: WHISPER_FULL_LANGS.iter().map(|s| s.to_string()).collect(),
        downloading_elsewhere: false,
    }
}

fn missing_files(model: &ModelInfo, dir: &Path) -> Vec<String> {
    model
        .files
        .iter()
        .filter(|f| {
            !std::fs::metadata(dir.join(f.name)).is_ok_and(|m| m.is_file() && m.len() == f.size)
        })
        .map(|f| f.name.to_string())
        .collect()
}

pub fn parakeet_models(store: &ModelStore) -> Vec<ParakeetModelState> {
    store
        .shared()
        .list()
        .into_iter()
        .filter(|(m, _)| {
            matches!(
                m.kind,
                Kind::Parakeet | Kind::ParakeetUnified | Kind::Nemotron
            )
        })
        .map(|(m, status)| {
            let installed = matches!(status, Status::Installed { .. });
            let dir = store.shared().model_dir(m.id);
            ParakeetModelState {
                id: m.id.into(),
                display_name: m.name.into(),
                multilingual: !m.english_only,
                is_quantized: m.files.iter().any(|f| f.name.contains(".int8.")),
                size_bytes: m.size(),
                notes: m.note.into(),
                downloaded: installed,
                missing_files: if installed {
                    Vec::new()
                } else {
                    missing_files(m, &dir)
                },
                on_disk_bytes: installed.then(|| m.size()),
                path: installed.then(|| path_string(&dir)),
                speed: rating(m.speed),
                accuracy: rating(m.accuracy),
                language_codes: parakeet_languages(m)
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
                downloading_elsewhere: is_elsewhere(&status),
            }
        })
        .collect()
}

/// The local AI model Parlato recommends and lists first. The catalog never
/// recommends; this is the app's own choice.
pub const RECOMMENDED_LOCAL_AI: &str = "granite-4.2-3b";

pub fn gguf_models(store: &ModelStore) -> Vec<GgufModelState> {
    let mut catalog_models: Vec<&ModelInfo> = catalog::local_ai().collect();
    // Stable: the recommended model first, the rest in catalog order.
    catalog_models.sort_by_key(|m| m.id != RECOMMENDED_LOCAL_AI);
    let mut out: Vec<GgufModelState> = catalog_models
        .into_iter()
        .map(|m| {
            let status = store.shared().status(m.id);
            let installed = match &status {
                Status::Installed { dir, .. } => Some(ModelStore::installed_path(m, dir)),
                _ => None,
            };
            GgufModelState {
                id: m.id.into(),
                display_name: m.name.into(),
                size_bytes: m.size(),
                context_length: m.llm.map_or(0, |l| l.context_max),
                notes: m.note.into(),
                downloaded: installed.is_some(),
                on_disk_bytes: installed.as_ref().map(|_| m.size()),
                path: installed.as_deref().map(path_string),
                imported: false,
                removable: true,
                downloading_elsewhere: is_elsewhere(&status),
            }
        })
        .collect();
    out.extend(
        imports::list_gguf_imports(&store.llm_dir())
            .into_iter()
            .map(|f| GgufModelState {
                id: f.id,
                display_name: f.stem,
                size_bytes: f.size,
                context_length: 0,
                notes: GGUF_IMPORT_NOTES.into(),
                downloaded: true,
                on_disk_bytes: Some(f.size),
                path: Some(path_string(&f.path)),
                imported: true,
                removable: f.removable,
                downloading_elsewhere: false,
            }),
    );
    out
}

pub fn vad_state(store: &ModelStore) -> VadModelState {
    let status = store.shared().status("vad");
    match (&status, catalog::find("vad")) {
        (Status::Installed { dir, .. }, Some(m)) => VadModelState {
            downloaded: true,
            path: Some(path_string(&ModelStore::installed_path(m, dir))),
            on_disk_bytes: Some(m.size()),
            downloading_elsewhere: false,
        },
        _ => VadModelState {
            downloaded: false,
            path: None,
            on_disk_bytes: None,
            downloading_elsewhere: is_elsewhere(&status),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::super::store::tests::{install, temp_store};
    use super::*;

    #[test]
    fn whisper_list_is_the_catalog_with_nothing_installed() {
        let tmp = tempfile::tempdir().unwrap();
        let store = temp_store(tmp.path());
        let list = whisper_models(&store);
        let expected = catalog::all()
            .iter()
            .filter(|m| m.kind == Kind::Whisper)
            .count();
        assert_eq!(list.len(), expected);
        assert_eq!(expected, 14);
        assert!(list
            .iter()
            .all(|m| !m.downloaded && m.path.is_none() && !m.imported));
        let tiny_en = list.iter().find(|m| m.id == "whisper-tiny-en").unwrap();
        assert!(!tiny_en.multilingual);
        assert_eq!(tiny_en.language_codes, ["en"]);
        let tiny = list.iter().find(|m| m.id == "whisper-tiny").unwrap();
        assert!(tiny.multilingual && tiny.language_codes.len() > 90);
        assert!(tiny.speed > 0.0 && tiny.speed <= 1.0);
    }

    #[test]
    fn installed_whisper_shows_its_file_and_imports_follow() {
        let tmp = tempfile::tempdir().unwrap();
        let store = temp_store(tmp.path());
        install(&store, "whisper-tiny");
        let imported = store.whisper_dir().join("imported");
        std::fs::create_dir_all(&imported).unwrap();
        std::fs::write(imported.join("custom.bin"), b"abc").unwrap();
        let list = whisper_models(&store);
        let tiny = list.iter().find(|m| m.id == "whisper-tiny").unwrap();
        assert!(tiny.downloaded);
        assert!(tiny
            .path
            .as_deref()
            .unwrap()
            .ends_with("whisper-tiny/ggml-tiny.bin"));
        assert_eq!(tiny.on_disk_bytes, Some(tiny.size_bytes));
        let last = list.last().unwrap();
        assert_eq!(last.id, "imported:custom");
        assert!(last.imported && last.downloaded);
        assert_eq!(last.size_bytes, 3);
    }

    #[test]
    fn parakeet_list_covers_three_kinds() {
        let tmp = tempfile::tempdir().unwrap();
        let store = temp_store(tmp.path());
        let list = parakeet_models(&store);
        let ids: Vec<_> = list.iter().map(|m| m.id.as_str()).collect();
        for id in [
            "parakeet-v3",
            "parakeet-v3-full",
            "parakeet-v2",
            "parakeet-v2-full",
            "parakeet-ultra",
            "parakeet-unified-en",
            "parakeet-unified-en-full",
            "nemotron-3.5",
        ] {
            assert!(ids.contains(&id), "{id}");
        }
        assert_eq!(list.len(), 8);
        let ultra = list.iter().find(|m| m.id == "parakeet-ultra").unwrap();
        assert!(ultra.is_quantized && ultra.multilingual);
        assert_eq!(ultra.language_codes.len(), PARAKEET_V3_LANGS.len());
        let v2 = list.iter().find(|m| m.id == "parakeet-v2").unwrap();
        assert_eq!(v2.language_codes, ["en"]);
        let nemo = list.iter().find(|m| m.id == "nemotron-3.5").unwrap();
        assert_eq!(nemo.language_codes.len(), NEMOTRON_LANGS.len());
        let full = list.iter().find(|m| m.id == "parakeet-v3-full").unwrap();
        assert!(!full.is_quantized);
    }

    #[test]
    fn partial_parakeet_lists_its_missing_files() {
        let tmp = tempfile::tempdir().unwrap();
        let store = temp_store(tmp.path());
        let model = catalog::find("parakeet-v3").unwrap();
        let dir = store.shared().model_dir("parakeet-v3");
        std::fs::create_dir_all(&dir).unwrap();
        let first = &model.files[0];
        std::fs::File::create(dir.join(first.name))
            .unwrap()
            .set_len(first.size)
            .unwrap();
        let state = parakeet_models(&store)
            .into_iter()
            .find(|m| m.id == "parakeet-v3")
            .unwrap();
        assert!(!state.downloaded);
        assert_eq!(state.missing_files.len(), model.files.len() - 1);
        assert!(!state.missing_files.contains(&first.name.to_string()));
        install(&store, "parakeet-v3");
        let state = parakeet_models(&store)
            .into_iter()
            .find(|m| m.id == "parakeet-v3")
            .unwrap();
        assert!(state.downloaded && state.missing_files.is_empty());
        assert!(state.path.is_some());
    }

    #[test]
    fn gguf_list_is_the_catalog_then_the_imports() {
        let tmp = tempfile::tempdir().unwrap();
        let store = temp_store(tmp.path());
        let llm = store.llm_dir();
        std::fs::create_dir_all(llm.join("imported")).unwrap();
        std::fs::write(llm.join("granite-4.2-3b-q4.gguf"), b"link").unwrap();
        std::fs::write(llm.join("phi-3.5-mini-instruct-q4.gguf"), b"phi").unwrap();
        std::fs::write(llm.join("imported").join("mine.gguf"), b"mine").unwrap();
        let list = gguf_models(&store);
        let ids: Vec<_> = list.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(&ids[..3], ["granite-4.2-3b", "qwen3.5-4b", "qwen3.5-9b"]);
        assert_eq!(list.len(), 5);
        assert!(list[..3]
            .iter()
            .all(|m| !m.imported && !m.downloaded && m.context_length > 0));
        assert!(
            list[3].imported
                && list[3].display_name == "phi-3.5-mini-instruct-q4"
                && !list[3].removable
        );
        assert!(list[4].imported && list[4].display_name == "mine" && list[4].removable);
        assert!(list[3].id.starts_with("import-"));
    }

    #[test]
    fn vad_state_follows_the_store() {
        let tmp = tempfile::tempdir().unwrap();
        let store = temp_store(tmp.path());
        let s = vad_state(&store);
        assert!(!s.downloaded && s.path.is_none() && s.on_disk_bytes.is_none());
        install(&store, "vad");
        let s = vad_state(&store);
        assert!(s.downloaded);
        assert!(s.path.unwrap().ends_with("vad/ggml-silero-v6.2.0.bin"));
        assert_eq!(s.on_disk_bytes, Some(885_098));
    }

    #[test]
    fn parakeet_files_have_the_names_the_runtime_reads() {
        // The runtime loads the folder: TDT wants config.json, vocab.txt,
        // nemo128.onnx, an encoder and a decoder_joint; Unified and Nemotron
        // want tokenizer.model, an encoder and a decoder_joint.
        for m in catalog::all() {
            let has = |s: &str| m.files.iter().any(|f| f.name.starts_with(s));
            match m.kind {
                Kind::Parakeet => {
                    for f in [
                        "config.json",
                        "vocab.txt",
                        "nemo128.onnx",
                        "encoder-model",
                        "decoder_joint-model",
                    ] {
                        assert!(has(f), "{} lacks {f}", m.id);
                    }
                }
                Kind::ParakeetUnified | Kind::Nemotron => {
                    for f in ["tokenizer.model", "encoder", "decoder_joint"] {
                        assert!(has(f), "{} lacks {f}", m.id);
                    }
                }
                _ => {}
            }
        }
    }
}
