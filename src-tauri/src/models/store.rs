// The shared model store in the app's managed state.
//
// `SharedStore` (voice-engine-models) keeps every model in one folder that
// Parlato, Talaan and Ensayo share. This wrapper adds what Parlato needs
// around it: the folders of its own imports, the cancel flags of running
// downloads, and the lookup from a model id to the path a runtime loads.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use parking_lot::Mutex;
use voice_engine_models::{catalog, Kind, ModelHandle, ModelInfo, Removed, SharedStore, Status};

use super::imports;

pub struct ModelStore {
    shared: SharedStore,
    /// Parlato's own app-local data folder: `Models/imported`, `LlmModels`,
    /// and the copies an older Parlato left behind.
    local_dir: PathBuf,
    downloads: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

/// Tauri managed state.
pub struct ModelStoreState(pub Arc<ModelStore>);

impl ModelStore {
    pub fn new(shared: SharedStore, local_dir: PathBuf) -> Self {
        Self {
            shared,
            local_dir,
            downloads: Mutex::new(HashMap::new()),
        }
    }

    pub fn shared(&self) -> &SharedStore {
        &self.shared
    }

    /// `Models/`: Parlato's own Whisper folder (imports live in `imported/`).
    pub fn whisper_dir(&self) -> PathBuf {
        self.local_dir.join("Models")
    }

    /// `LlmModels/`: local AI files Parlato 0.14 kept, and the imports.
    pub fn llm_dir(&self) -> PathBuf {
        self.local_dir.join("LlmModels")
    }

    pub fn parakeet_legacy_dir(&self) -> PathBuf {
        self.local_dir.join("ParakeetModels")
    }

    /// Where the runtime reads `model` once it is installed in `dir`: the
    /// folder for the ONNX families, the single file for the others.
    pub fn installed_path(model: &ModelInfo, dir: &Path) -> PathBuf {
        match model.kind {
            Kind::Parakeet | Kind::ParakeetUnified | Kind::Nemotron => dir.to_path_buf(),
            _ => model
                .files
                .first()
                .map_or_else(|| dir.to_path_buf(), |f| dir.join(f.name)),
        }
    }

    /// The path a runtime loads for `id`, only when the model is fully there:
    /// a catalog model (Installed in the shared folder), an imported Whisper
    /// file (`imported:<stem>`) or an imported local AI file (`import-...`).
    pub fn path_for_id(&self, id: &str) -> Option<PathBuf> {
        if let Some(model) = catalog::find(id) {
            return match self.shared.status(id) {
                Status::Installed { dir, .. } => Some(Self::installed_path(model, &dir)),
                _ => None,
            };
        }
        if id.starts_with(imports::WHISPER_IMPORT_PREFIX) {
            return imports::whisper_import_path(&self.whisper_dir(), id);
        }
        imports::find_gguf_import(&self.llm_dir(), id).map(|f| f.path)
    }

    /// A handle to keep while the model is loaded (another app's Delete cannot
    /// erase it meanwhile). None for imports, which are not in the store.
    pub fn hold(&self, id: &str) -> Option<ModelHandle> {
        self.shared.hold(id).ok()
    }

    // -- Running downloads ----------------------------------------------------

    /// Registers a download. None when one for `id` is already running here.
    pub fn begin_download(&self, id: &str) -> Option<Arc<AtomicBool>> {
        let mut map = self.downloads.lock();
        if map.contains_key(id) {
            return None;
        }
        let flag = Arc::new(AtomicBool::new(false));
        map.insert(id.to_string(), flag.clone());
        Some(flag)
    }

    pub fn end_download(&self, id: &str) {
        self.downloads.lock().remove(id);
    }

    pub fn cancel_download(&self, id: &str) {
        if let Some(flag) = self.downloads.lock().get(id) {
            flag.store(true, Ordering::SeqCst);
        }
    }

    // -- Delete ----------------------------------------------------------------

    /// Delete in the settings: this app lets go of the model (the last user
    /// erases the files), then the copy an older Parlato left at its old path
    /// goes too, or its hard link would keep the disk space.
    pub fn release(&self, id: &str) -> std::io::Result<voice_engine_models::Released> {
        let released = self.shared.release(id)?;
        self.remove_legacy_copy(id);
        Ok(released)
    }

    /// "Remove from this computer": the model goes for every app (the shared
    /// folder, all the markers, the old copies in the apps' old folders).
    /// `InUse` when any app has it loaded or is downloading it, nothing
    /// touched: this app's own `ModelHandle` counts, unload first. The copy an
    /// older Parlato left at its old path goes too, as in `release`.
    pub fn remove_everywhere(&self, id: &str) -> std::io::Result<Removed> {
        let removed = self.shared.remove_everywhere(id)?;
        if removed != Removed::InUse {
            self.remove_legacy_copy(id);
        }
        Ok(removed)
    }

    /// Removes the file or folder Parlato 0.14 stored for this catalog id
    /// (`Models/<old id>.bin`, `ParakeetModels/<old id>/`,
    /// `LlmModels/granite-4.2-3b-q4.gguf`, the VAD file). After the move these
    /// are hard links to the shared files. Only names built from the fixed
    /// tables are touched.
    fn remove_legacy_copy(&self, id: &str) {
        let Some(path) = self.legacy_path(id) else {
            return;
        };
        if path.is_dir() {
            let _ = std::fs::remove_dir_all(&path);
        } else if path.is_file() {
            let _ = std::fs::remove_file(&path);
        }
    }

    fn legacy_path(&self, id: &str) -> Option<PathBuf> {
        let model = catalog::find(id)?;
        match model.kind {
            Kind::Vad => Some(self.whisper_dir().join(model.files.first()?.name)),
            Kind::Whisper => {
                let old = super::ids::legacy_id(id)?;
                Some(self.whisper_dir().join(format!("{old}.bin")))
            }
            Kind::Parakeet | Kind::ParakeetUnified | Kind::Nemotron => {
                let old = super::ids::legacy_id(id)?;
                Some(self.parakeet_legacy_dir().join(old))
            }
            Kind::Llm if id == "granite-4.2-3b" => {
                Some(self.llm_dir().join(imports::LEGACY_GRANITE_FILE))
            }
            _ => None,
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::fs::File;
    use voice_engine_models::App;

    /// A store in a temp folder: tests never touch the real shared folder.
    pub(crate) fn temp_store(root: &Path) -> ModelStore {
        let shared = SharedStore::open_at(App::Parlato, root.join("shared")).unwrap();
        ModelStore::new(shared, root.join("app"))
    }

    /// Makes `id` look installed: every file at its exact size (sparse).
    pub(crate) fn install(store: &ModelStore, id: &str) {
        let model = catalog::find(id).unwrap();
        let dir = store.shared().model_dir(id);
        std::fs::create_dir_all(&dir).unwrap();
        for f in model.files {
            File::create(dir.join(f.name))
                .unwrap()
                .set_len(f.size)
                .unwrap();
        }
    }

    #[test]
    fn path_for_id_needs_a_complete_model() {
        let tmp = tempfile::tempdir().unwrap();
        let store = temp_store(tmp.path());
        assert_eq!(store.path_for_id("vad"), None);
        assert_eq!(store.path_for_id("whisper-tiny"), None);
        install(&store, "vad");
        install(&store, "whisper-tiny");
        let dir = store.shared().model_dir("whisper-tiny");
        assert_eq!(
            store.path_for_id("whisper-tiny"),
            Some(dir.join("ggml-tiny.bin"))
        );
        assert_eq!(
            store.path_for_id("vad"),
            Some(
                store
                    .shared()
                    .model_dir("vad")
                    .join("ggml-silero-v6.2.0.bin")
            )
        );
        // A truncated file is not installed.
        let file = dir.join("ggml-tiny.bin");
        File::create(&file).unwrap().set_len(10).unwrap();
        assert_eq!(store.path_for_id("whisper-tiny"), None);
    }

    #[test]
    fn parakeet_path_is_the_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let store = temp_store(tmp.path());
        install(&store, "parakeet-v3");
        assert_eq!(
            store.path_for_id("parakeet-v3"),
            Some(store.shared().model_dir("parakeet-v3"))
        );
    }

    #[test]
    fn imports_resolve_by_id() {
        let tmp = tempfile::tempdir().unwrap();
        let store = temp_store(tmp.path());
        let wh = store.whisper_dir().join("imported");
        std::fs::create_dir_all(&wh).unwrap();
        std::fs::write(wh.join("mine.bin"), b"x").unwrap();
        assert_eq!(
            store.path_for_id("imported:mine"),
            Some(wh.join("mine.bin"))
        );
        assert_eq!(store.path_for_id("imported:other"), None);
        let llm = store.llm_dir();
        std::fs::create_dir_all(&llm).unwrap();
        std::fs::write(llm.join("m.gguf"), b"gguf").unwrap();
        let id = imports::import_id_of_stem(&llm, "m").unwrap();
        assert_eq!(store.path_for_id(&id), Some(llm.join("m.gguf")));
        assert_eq!(store.path_for_id("nonsense"), None);
    }

    #[test]
    fn download_flags_guard_reentrancy_and_cancel() {
        let tmp = tempfile::tempdir().unwrap();
        let store = temp_store(tmp.path());
        let flag = store.begin_download("vad").unwrap();
        assert!(store.begin_download("vad").is_none());
        assert!(!flag.load(Ordering::SeqCst));
        store.cancel_download("vad");
        assert!(flag.load(Ordering::SeqCst));
        store.end_download("vad");
        assert!(store.begin_download("vad").is_some());
    }

    #[test]
    fn hold_blocks_erase_until_dropped() {
        let tmp = tempfile::tempdir().unwrap();
        let store = temp_store(tmp.path());
        install(&store, "vad");
        store.shared().use_model("vad").unwrap();
        let handle = store.hold("vad").unwrap();
        assert!(handle.dir().ends_with("vad"));
        let kept = store.release("vad").unwrap();
        assert_eq!(
            kept,
            voice_engine_models::Released::Kept { users: Vec::new() }
        );
        drop(handle);
        assert!(store.hold("nonsense").is_none());
    }

    #[test]
    fn release_erases_the_shared_files_and_the_old_copy() {
        let tmp = tempfile::tempdir().unwrap();
        let store = temp_store(tmp.path());
        install(&store, "whisper-tiny");
        store.shared().use_model("whisper-tiny").unwrap();
        // The hard link a move leaves at the old path.
        std::fs::create_dir_all(store.whisper_dir()).unwrap();
        let old = store.whisper_dir().join("ggml-tiny.bin");
        std::fs::hard_link(
            store
                .shared()
                .model_dir("whisper-tiny")
                .join("ggml-tiny.bin"),
            &old,
        )
        .unwrap();
        let released = store.release("whisper-tiny").unwrap();
        assert_eq!(released, voice_engine_models::Released::Erased);
        assert!(!store.shared().model_dir("whisper-tiny").exists());
        assert!(!old.exists());
    }

    #[test]
    fn legacy_paths_match_parlato_0_14() {
        let tmp = tempfile::tempdir().unwrap();
        let store = temp_store(tmp.path());
        let app = tmp.path().join("app");
        assert_eq!(
            store.legacy_path("whisper-turbo"),
            Some(app.join("Models/ggml-large-v3-turbo-q5_0.bin"))
        );
        assert_eq!(
            store.legacy_path("vad"),
            Some(app.join("Models/ggml-silero-v6.2.0.bin"))
        );
        assert_eq!(
            store.legacy_path("parakeet-ultra"),
            Some(app.join("ParakeetModels/parakeet-ultra-0.6b-int8"))
        );
        assert_eq!(
            store.legacy_path("granite-4.2-3b"),
            Some(app.join("LlmModels/granite-4.2-3b-q4.gguf"))
        );
        assert_eq!(store.legacy_path("qwen3.5-4b"), None);
        assert_eq!(store.legacy_path("whisper-large-v2"), None);
    }
}
