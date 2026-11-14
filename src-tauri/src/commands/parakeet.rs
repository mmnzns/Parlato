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

/// Parlato : asynchrone. Le dechargement attend une transcription en cours et
/// la suppression efface de gros fichiers : sur un fil de travail, jamais sur
/// le fil de l'interface (meme signature cote frontend, `invoke` renvoie deja
/// une promesse).
#[command]
pub async fn delete_parakeet_model(
    app: AppHandle,
    state: State<'_, ModelStoreState>,
    id: String,
) -> Result<(), String> {
    let store = state.0.clone();
    let engine = app
        .try_state::<crate::transcription::parakeet::ParakeetEngineState>()
        .map(|e| e.0.clone());
    tokio::task::spawn_blocking(move || {
        // Decharge le modele si c'est celui qui est actif.
        if let Some(engine) = engine {
            engine.unload_if_under(&store.shared().model_dir(&id));
        }
        store.release(&id).map(|_| ()).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("task join: {e}"))?
}

/// Parlato : ce qui a vraiment servi au dernier chargement du modele
/// (`backend()` du moteur) : "cpu", "directml" ou "cpu-fallback" (carte
/// graphique demandee, le processeur a pris le relais).
#[command]
pub fn parakeet_execution_provider(app: AppHandle) -> String {
    app.try_state::<crate::transcription::parakeet::ParakeetEngineState>()
        .map(|e| e.0.provider())
        .unwrap_or_else(|| "cpu".to_string())
}
