// Commandes Tauri pour le modele VAD Silero (download/delete/state/enable),
// sur le dossier de modeles partage du moteur (id "vad").

use serde_json::Value;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_store::StoreExt;

use crate::models::download::{self, Flavor};
use crate::models::list::{self, VadModelState};
use crate::models::ModelStoreState;
use crate::transcription::vad::VadEngine;

const STORE_FILE: &str = "parla.settings.json";
const VAD_ENABLED_KEY: &str = "vad_enabled";
pub const VAD_ID: &str = "vad";

pub struct VadEngineState(pub std::sync::Arc<VadEngine>);

impl Default for VadEngineState {
    fn default() -> Self {
        Self(std::sync::Arc::new(VadEngine::default()))
    }
}

#[tauri::command]
pub fn vad_get_state(store: State<'_, ModelStoreState>) -> VadModelState {
    list::vad_state(&store.0)
}

#[tauri::command]
pub async fn vad_download(app: AppHandle, store: State<'_, ModelStoreState>) -> Result<String, String> {
    download::run(&app, store.0.clone(), VAD_ID, Flavor::Vad).await
}

#[tauri::command]
pub fn vad_delete(app: AppHandle, store: State<'_, ModelStoreState>) -> Result<(), String> {
    if let Some(engine) = app.try_state::<VadEngineState>() {
        engine.0.unload_if_under(&store.0.shared().model_dir(VAD_ID));
    }
    store.0.release(VAD_ID).map(|_| ()).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn vad_is_enabled(app: AppHandle) -> bool {
    let Some(store) = app.store(STORE_FILE).ok() else {
        return false;
    };
    store
        .get(VAD_ENABLED_KEY)
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

#[tauri::command]
pub fn vad_set_enabled(app: AppHandle, enabled: bool) -> Result<(), String> {
    let store = app.store(STORE_FILE).map_err(|e| e.to_string())?;
    store.set(VAD_ENABLED_KEY, Value::Bool(enabled));
    store.save().map_err(|e| e.to_string())?;
    // Voice detection is a model this app uses once it is on: tell the store
    // so another app's Delete cannot erase it. Turning it off does not
    // release it; Delete on the voice-detection panel does.
    if enabled {
        if let Some(models) = app.try_state::<ModelStoreState>() {
            let _ = models.0.shared().use_model(VAD_ID);
        }
    }
    Ok(())
}

#[tauri::command]
pub fn vad_is_ready(app: AppHandle, store: State<'_, ModelStoreState>) -> bool {
    // Ready = le modele existe sur disque ET vad_is_enabled = true.
    vad_is_enabled(app) && list::vad_state(&store.0).downloaded
}
