// Commandes Tauri pour les modeles Parakeet, Parakeet Unified et Nemotron,
// sur le dossier de modeles partage du moteur (voir crate::models).

use tauri::{command, AppHandle, Manager, State};

use crate::models::download::{self, Flavor};
use crate::models::list::{self, ParakeetModelState};
use crate::models::ModelStoreState;

#[command]
pub fn list_parakeet_models(state: State<'_, ModelStoreState>) -> Result<Vec<ParakeetModelState>, String> {
    Ok(list::parakeet_models(&state.0))
}

#[command]
pub async fn download_parakeet_model(
    app: AppHandle,
    state: State<'_, ModelStoreState>,
    id: String,
) -> Result<String, String> {
    download::run(&app, state.0.clone(), &id, Flavor::Parakeet).await
}

#[command]
pub fn cancel_download_parakeet_model(state: State<'_, ModelStoreState>, id: String) {
    state.0.cancel_download(&id);
}

#[command]
pub fn delete_parakeet_model(
    app: AppHandle,
    state: State<'_, ModelStoreState>,
    id: String,
) -> Result<(), String> {
    // Decharge le modele si c'est celui qui est actif.
    if let Some(engine) = app.try_state::<crate::transcription::parakeet::ParakeetEngineState>() {
        engine.0.unload_if_dir(&state.0.shared().model_dir(&id));
    }
    state.0.release(&id).map(|_| ()).map_err(|e| e.to_string())
}

/// Parlato : ce qui a vraiment servi au dernier chargement du modele :
/// "cpu", "directml", "cuda" ou "cpu-fallback" (carte graphique demandee, le
/// processeur a pris le relais).
#[command]
pub fn parakeet_execution_provider(app: AppHandle) -> String {
    app.try_state::<crate::transcription::parakeet::ParakeetEngineState>()
        .map(|e| e.0.provider())
        .unwrap_or("cpu")
        .to_string()
}
