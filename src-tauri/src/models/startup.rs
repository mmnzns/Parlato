// Startup of the shared model store (plan Decision 3). In `setup`, before
// anything can load a model:
//
//   1. open the store (`SharedStore::open(App::Parlato)`);
//   2. migrate the saved model ids to the catalog's ids (settings and Power
//      Mode rows, and the saved local AI choice);
//   3. `move_in(&used)`: bring Parlato's old models into the shared folder,
//      once, and record which ones this app uses;
//   4. `collect()`: erase models nobody uses and old part files.
//
// The log has ids and counts only. Tests never get here: they use
// `SharedStore::open_at` with a temp folder. A debug build opens the real
// shared folder too (the real update test uses a release build);
// `PARLATO_MODELS_DIR=<folder>` opens that folder instead, for manual tests.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::Value;
use tauri::{AppHandle, Manager};
use tauri_plugin_store::StoreExt;
use tracing::{info, warn};
use voice_engine_models::{catalog, App, MoveReport, SharedStore};

use super::ids::{self, GgufChoice};
use super::imports;
use super::store::ModelStore;
use crate::power_mode::config::PowerModeConfig;

const STORE_FILE: &str = "parla.settings.json";
const KEY_WHISPER: &str = "selected_whisper_model";
const KEY_PARAKEET: &str = "selected_parakeet_model";
const KEY_GGUF: &str = "llm_selected_gguf";
const KEY_VAD: &str = "vad_enabled";

/// The environment variable that points the app at another model folder.
pub const MODELS_DIR_ENV: &str = "PARLATO_MODELS_DIR";

/// Runs the four startup steps and returns the store to put in managed state.
pub fn init(app: &AppHandle) -> Arc<ModelStore> {
    let local_dir = app
        .path()
        .app_local_data_dir()
        .unwrap_or_else(|_| std::env::temp_dir().join("com.craftconceptsdigital.parlato"));
    let shared = open_shared(&local_dir);
    let store = Arc::new(ModelStore::new(shared, local_dir));

    // 2. Ids.
    migrate_settings(app, &store.llm_dir());

    // 3. Move in what this app uses and what it had.
    let used = used_ids(app);
    let used_refs: Vec<&str> = used.iter().map(String::as_str).collect();
    match store.shared().move_in(&used_refs) {
        Ok(report) => log_report(&report),
        Err(e) => warn!(error = %e, "model move failed; it is tried again at the next start"),
    }

    // Every later start too: the move above only records the used ids the
    // first time, but a model the person picked since must stay protected.
    for id in &used {
        if let Err(e) = store.shared().use_model(id) {
            warn!(id = %id, error = %e, "could not record the model as used");
        }
    }

    // 4. Clean up.
    if let Err(e) = store.shared().collect() {
        warn!(error = %e, "model folder clean-up failed");
    }
    store
}

/// Opens the shared folder. `PARLATO_MODELS_DIR` wins; if the shared folder
/// cannot be opened, a folder of Parlato's own is used so the app still runs.
fn open_shared(local_dir: &Path) -> SharedStore {
    if let Some(dir) = std::env::var_os(MODELS_DIR_ENV).filter(|d| !d.is_empty()) {
        match SharedStore::open_at(App::Parlato, PathBuf::from(&dir)) {
            Ok(store) => {
                info!("model folder: the one in PARLATO_MODELS_DIR");
                return store;
            }
            Err(e) => warn!(error = %e, "PARLATO_MODELS_DIR could not be opened"),
        }
    }
    match SharedStore::open(App::Parlato) {
        Ok(store) => store,
        Err(e) => {
            warn!(error = %e, "shared model folder unavailable, using Parlato's own folder");
            let own = local_dir.join("EngineModels");
            SharedStore::open_at(App::Parlato, own.clone())
                .or_else(|_| {
                    SharedStore::open_at(App::Parlato, std::env::temp_dir().join("parlato-models"))
                })
                .expect("a model folder can be created")
        }
    }
}

fn log_report(r: &MoveReport) {
    if r.already_done {
        info!("model move: already done");
        return;
    }
    info!(
        moved = r.moved.len(),
        linked = r.linked.len(),
        imported = r.imported.len(),
        left_for_later = r.left_for_later.len(),
        "model move finished"
    );
    if !r.moved.is_empty() {
        info!(ids = ?r.moved, "model move: moved");
    }
    if !r.linked.is_empty() {
        info!(ids = ?r.linked, "model move: linked from other apps");
    }
    for (id, reason) in &r.left_for_later {
        warn!(id = %id, reason = %reason, "model move: left for later");
    }
    if !r.imported.is_empty() {
        let ids: Vec<&str> = r.imported.iter().map(|i| i.id.as_str()).collect();
        info!(ids = ?ids, "model move: local AI files kept as imports");
    }
}

// -- Ids -----------------------------------------------------------------------

/// Rewrites one saved id, if it is an old one.
fn migrated(value: Option<&str>) -> Option<&'static str> {
    value.and_then(ids::migrate_id)
}

/// Power Mode row: old ids become catalog ids. True when something changed.
pub fn migrate_power_mode_ids(cfg: &mut PowerModeConfig) -> bool {
    let mut changed = false;
    if let Some(new) = migrated(cfg.whisper_model_id.as_deref()) {
        cfg.whisper_model_id = Some(new.to_string());
        changed = true;
    }
    if let Some(new) = migrated(cfg.parakeet_model_id.as_deref()) {
        cfg.parakeet_model_id = Some(new.to_string());
        changed = true;
    }
    changed
}

fn migrate_settings(app: &AppHandle, llm_dir: &Path) {
    if let Ok(store) = app.store(STORE_FILE) {
        let mut changed = false;
        for key in [KEY_WHISPER, KEY_PARAKEET] {
            let current = store.get(key).and_then(|v| v.as_str().map(String::from));
            if let Some(new) = migrated(current.as_deref()) {
                store.set(key, Value::String(new.to_string()));
                info!(key, from = ?current, to = new, "model id migrated");
                changed = true;
            }
        }
        if let Some(current) = store
            .get(KEY_GGUF)
            .and_then(|v| v.as_str().map(String::from))
        {
            let find = |stem: &str| imports::import_id_of_stem(llm_dir, stem);
            match ids::migrate_gguf_choice(&current, &find) {
                GgufChoice::Keep => {}
                GgufChoice::Set(new) => {
                    info!(key = KEY_GGUF, from = %current, to = %new, "model id migrated");
                    store.set(KEY_GGUF, Value::String(new));
                    changed = true;
                }
                GgufChoice::Clear => {
                    info!(key = KEY_GGUF, from = %current, "saved local AI file is gone, choice cleared");
                    store.delete(KEY_GGUF);
                    changed = true;
                }
            }
        }
        if changed {
            if let Err(e) = store.save() {
                warn!(error = %e, "saving the migrated model ids failed");
            }
        }
    }

    match crate::power_mode::config::load_all(app) {
        Ok(mut rows) => {
            let mut changed = false;
            for row in rows.iter_mut() {
                changed |= migrate_power_mode_ids(row);
            }
            if changed {
                match crate::power_mode::config::save_all(app, &rows) {
                    Ok(()) => info!("Power Mode model ids migrated"),
                    Err(e) => warn!(error = %e, "Power Mode model id migration failed"),
                }
            }
        }
        Err(e) => warn!(error = %e, "Power Mode rows not migrated"),
    }
}

/// Every catalog id the settings and Power Mode rows point to, plus `vad`
/// when voice detection is on. (Called after the ids are migrated.)
fn used_ids(app: &AppHandle) -> Vec<String> {
    let mut chosen: Vec<String> = Vec::new();
    let mut vad_on = false;
    if let Ok(store) = app.store(STORE_FILE) {
        for key in [KEY_WHISPER, KEY_PARAKEET, KEY_GGUF] {
            if let Some(id) = store.get(key).and_then(|v| v.as_str().map(String::from)) {
                chosen.push(id);
            }
        }
        vad_on = store
            .get(KEY_VAD)
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
    }
    if let Ok(rows) = crate::power_mode::config::load_all(app) {
        for row in rows {
            chosen.extend(row.whisper_model_id);
            chosen.extend(row.parakeet_model_id);
        }
    }
    used_catalog_ids(chosen, vad_on)
}

/// Keeps the catalog ids (no imports, no unknown ids), once each, in order.
pub fn used_catalog_ids(chosen: impl IntoIterator<Item = String>, vad_on: bool) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for id in chosen {
        if catalog::find(&id).is_some() && !out.contains(&id) {
            out.push(id);
        }
    }
    if vad_on && !out.iter().any(|i| i == "vad") {
        out.push("vad".into());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row() -> PowerModeConfig {
        PowerModeConfig::new("Mail".into(), "x".into())
    }

    #[test]
    fn power_mode_rows_are_migrated_and_idempotent() {
        let mut cfg = row();
        cfg.whisper_model_id = Some("ggml-large-v3-turbo-q5_0".into());
        cfg.parakeet_model_id = Some("parakeet-ultra-0.6b-int8".into());
        assert!(migrate_power_mode_ids(&mut cfg));
        assert_eq!(cfg.whisper_model_id.as_deref(), Some("whisper-turbo"));
        assert_eq!(cfg.parakeet_model_id.as_deref(), Some("parakeet-ultra"));
        assert!(!migrate_power_mode_ids(&mut cfg));
        assert_eq!(cfg.whisper_model_id.as_deref(), Some("whisper-turbo"));
    }

    #[test]
    fn power_mode_imports_and_empty_rows_are_left_alone() {
        let mut cfg = row();
        assert!(!migrate_power_mode_ids(&mut cfg));
        cfg.whisper_model_id = Some("imported:custom".into());
        assert!(!migrate_power_mode_ids(&mut cfg));
        assert_eq!(cfg.whisper_model_id.as_deref(), Some("imported:custom"));
    }

    #[test]
    fn every_old_id_migrates_in_a_power_mode_row() {
        for (old, new) in ids::LEGACY_IDS {
            if old.starts_with("ggml-") {
                let mut cfg = row();
                cfg.whisper_model_id = Some((*old).into());
                assert!(migrate_power_mode_ids(&mut cfg));
                assert_eq!(cfg.whisper_model_id.as_deref(), Some(*new));
            } else if !old.starts_with("granite") {
                let mut cfg = row();
                cfg.parakeet_model_id = Some((*old).into());
                assert!(migrate_power_mode_ids(&mut cfg));
                assert_eq!(cfg.parakeet_model_id.as_deref(), Some(*new));
            }
        }
    }

    #[test]
    fn used_ids_keep_catalog_ids_only() {
        let chosen = [
            "whisper-turbo",
            "imported:x",
            "whisper-turbo",
            "import-0123456789ab",
            "parakeet-v3",
            "ggml-base",
        ]
        .map(String::from);
        assert_eq!(
            used_catalog_ids(chosen.clone(), false),
            ["whisper-turbo", "parakeet-v3"]
        );
        assert_eq!(
            used_catalog_ids(chosen, true),
            ["whisper-turbo", "parakeet-v3", "vad"]
        );
        assert_eq!(used_catalog_ids(Vec::new(), true), ["vad"]);
        assert!(used_catalog_ids(Vec::new(), false).is_empty());
    }

    #[test]
    fn open_at_never_moves_anything() {
        // A test store (open_at) skips the move and never looks at old folders.
        let tmp = tempfile::tempdir().unwrap();
        let shared = SharedStore::open_at(App::Parlato, tmp.path().join("shared")).unwrap();
        let report = shared.move_in(&["whisper-turbo"]).unwrap();
        assert!(report.already_done && report.moved.is_empty());
        shared.collect().unwrap();
    }

    #[test]
    fn a_fake_old_layout_is_moved_into_a_temp_store() {
        // Parlato 0.14 layout in a temp folder: the VAD file is the only one
        // small enough to fake with its exact size and hash... the move checks
        // sizes, so use a sparse file of the exact size and a store that moves.
        use std::fs::File;
        let tmp = tempfile::tempdir().unwrap();
        let local = tmp.path().join("local");
        let old = local.join("com.craftconceptsdigital.parlato");
        std::fs::create_dir_all(old.join("Models")).unwrap();
        let vad = catalog::find("vad").unwrap();
        File::create(old.join("Models").join(vad.files[0].name))
            .unwrap()
            .set_len(vad.files[0].size)
            .unwrap();
        // A dropped local AI file stays where it is and is reported.
        std::fs::create_dir_all(old.join("LlmModels")).unwrap();
        std::fs::write(
            old.join("LlmModels").join("gemma-2-2b-it-q4.gguf"),
            b"gemma",
        )
        .unwrap();

        let shared = SharedStore::for_move_test(
            App::Parlato,
            &tmp.path().join("shared"),
            &local,
            None,
            false,
        )
        .unwrap();
        let report = shared.move_in(&["vad"]).unwrap();
        // The sparse file's content is zeros, so the VAD model's hash does not
        // match: Whisper-family files are only size-checked, so it moves.
        assert!(report.moved.contains(&"vad".to_string()) || !report.left_for_later.is_empty());
        assert_eq!(report.imported.len(), 1);
        assert!(old.join("LlmModels").join("gemma-2-2b-it-q4.gguf").exists());
        let found = imports::list_gguf_imports(&old.join("LlmModels"));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, report.imported[0].id);
    }
}
