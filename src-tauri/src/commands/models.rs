// Commandes Tauri pour le catalogue des modeles Whisper, sur le dossier de
// modeles partage du moteur (voir crate::models). Les noms de commandes et
// d'evenements restent ceux d'avant.

use tauri::{AppHandle, Manager, State};

use crate::models::download::{self, Flavor};
use crate::models::list::{self, WhisperModelState};
use crate::models::{imports, ModelStoreState};

#[tauri::command]
pub async fn list_whisper_models(
    state: State<'_, ModelStoreState>,
) -> Result<Vec<WhisperModelState>, String> {
    Ok(list::whisper_models(&state.0))
}

#[tauri::command]
pub async fn download_whisper_model(
    app: AppHandle,
    state: State<'_, ModelStoreState>,
    id: String,
) -> Result<String, String> {
    download::run(&app, state.0.clone(), &id, Flavor::Whisper).await
}

#[tauri::command]
pub async fn cancel_download_whisper_model(
    state: State<'_, ModelStoreState>,
    id: String,
) -> Result<(), String> {
    state.0.cancel_download(&id);
    Ok(())
}

#[tauri::command]
pub async fn delete_whisper_model(
    app: AppHandle,
    state: State<'_, ModelStoreState>,
    id: String,
) -> Result<(), String> {
    let store = state.0.clone();
    if id.starts_with(imports::WHISPER_IMPORT_PREFIX) {
        if let Some(path) = store.path_for_id(&id) {
            unload_whisper_if_loaded(&app, &path);
        }
        return imports::delete_whisper_import(&store.whisper_dir(), &id).map_err(|e| e.to_string());
    }
    // Le modele est decharge s'il est en memoire, sinon un verrou le garde.
    let dir = store.shared().model_dir(&id);
    unload_whisper_if_loaded(&app, &dir);
    store.release(&id).map(|_| ()).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn import_whisper_model(
    state: State<'_, ModelStoreState>,
    path: String,
) -> Result<String, String> {
    let src = std::path::PathBuf::from(path);
    imports::import_whisper(&state.0.whisper_dir(), &src).map_err(|e| e.to_string())
}

/// Decharge le moteur Whisper si le modele charge est `path` (un fichier) ou
/// se trouve dans `path` (un dossier du dossier partage).
fn unload_whisper_if_loaded(app: &AppHandle, path: &std::path::Path) {
    if let Some(engine) = app.try_state::<super::transcription::WhisperEngineState>() {
        engine.0.unload_if_under(path);
    }
}
