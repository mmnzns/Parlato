// Commande Tauri "Remove from this computer" pour tous les modeles du
// catalogue (Whisper, Parakeet, IA locale, detection de voix). Le modele est
// decharge de Parlato, puis efface pour toutes les applications (voir
// crate::models::remove). Un import n'est pas dans le dossier partage : il
// n'a que Delete.

use tauri::{AppHandle, Manager, State};
use voice_engine_models::{catalog, Kind, ModelInfo};

use crate::enhancement::providers::llamacpp;
use crate::models::remove::{self, Removal};
use crate::models::store::ModelStore;
use crate::models::ModelStoreState;
use crate::transcription::speech;

#[tauri::command]
pub async fn remove_model_everywhere(
    app: AppHandle,
    state: State<'_, ModelStoreState>,
    id: String,
) -> Result<Removal, String> {
    let model = catalog::find(&id).ok_or_else(|| "not a catalog model".to_string())?;
    let store = state.0.clone();
    // Decharger attend une dictee ou une generation en cours : sur un fil, pas
    // sur celui de l'interface.
    tauri::async_runtime::spawn_blocking(move || -> Result<Removal, String> {
        let removal = remove::run(
            &store,
            &id,
            || unload(&app, &store, model),
            |was| reload(&app, &store, model, was),
        )
        .map_err(|e| e.to_string())?;
        if removal != Removal::InUse && model.kind == Kind::Llm {
            // Comme Delete : le modele choisi n'existe plus, plus de choix.
            if llamacpp::get_selected_gguf(&app).as_deref() == Some(id.as_str()) {
                let _ = llamacpp::set_selected_gguf(&app, None);
            }
        }
        Ok(removal)
    })
    .await
    .map_err(|e| format!("task join: {e}"))?
}

/// Ce qu'il faut pour recharger le modele tel qu'il etait.
#[derive(Clone, Copy)]
enum Was {
    /// Whisper, Parakeet ou la detection de voix : rechargement standard.
    Loaded,
    /// IA locale : le contexte du chargement.
    Llm { n_ctx: u32 },
}

/// Decharge `model` de Parlato (le verrou du dossier partage part avec).
/// `Some` si Parlato l'avait charge.
fn unload(app: &AppHandle, store: &ModelStore, model: &ModelInfo) -> Option<Was> {
    let dir = store.shared().model_dir(model.id);
    match model.kind {
        Kind::Whisper => app
            .try_state::<crate::commands::transcription::WhisperEngineState>()
            .is_some_and(|e| e.0.unload_if_under(&dir))
            .then_some(Was::Loaded),
        Kind::Parakeet | Kind::ParakeetUnified | Kind::Nemotron => app
            .try_state::<crate::transcription::parakeet::ParakeetEngineState>()
            .is_some_and(|e| e.0.unload_if_under(&dir))
            .then_some(Was::Loaded),
        Kind::Vad => app
            .try_state::<crate::commands::vad::VadEngineState>()
            .is_some_and(|e| e.0.unload_if_under(&dir))
            .then_some(Was::Loaded),
        Kind::Llm => {
            let ai = crate::enhancement::local_ai::local_ai()?;
            let n_ctx = ai.loaded_n_ctx(model.id)?;
            // L'assistant continue de tourner ; seul le modele est libere.
            ai.unload();
            Some(Was::Llm { n_ctx })
        }
        // Parlato ne charge ni separation des voix ni embeddings.
        Kind::Diarization | Kind::Embedding => None,
    }
}

/// Recharge le modele comme il l'etait (rien n'a ete efface). Un echec est
/// journalise : la prochaine dictee le recharge de toute facon.
fn reload(app: &AppHandle, store: &ModelStore, model: &ModelInfo, was: Was) {
    let id = model.id;
    let result: anyhow::Result<()> = match (model.kind, was) {
        (Kind::Whisper, _) => speech::Source::for_id(store, id)
            .ok_or_else(|| anyhow::anyhow!("modele absent"))
            .and_then(|source| {
                let engine = app
                    .state::<crate::commands::transcription::WhisperEngineState>()
                    .0
                    .clone();
                let threads =
                    speech::threads_option(crate::transcription::pipeline::get_speech_threads(app));
                engine.ensure_loaded(&source, threads, store.hold(id))
            }),
        (Kind::Parakeet | Kind::ParakeetUnified | Kind::Nemotron, _) => {
            speech::Source::for_id(store, id)
                .ok_or_else(|| anyhow::anyhow!("modele absent"))
                .and_then(|source| {
                    let engine = app
                        .state::<crate::transcription::parakeet::ParakeetEngineState>()
                        .0
                        .clone();
                    let threads = speech::threads_option(
                        crate::transcription::pipeline::get_speech_threads(app),
                    );
                    engine.ensure_loaded(&source, threads, store.hold(id))
                })
        }
        (Kind::Vad, _) => {
            let engine = app
                .state::<crate::commands::vad::VadEngineState>()
                .0
                .clone();
            engine
                .ensure_loaded(&store.shared().model_dir(id), store.hold(id))
                .map(|_| ())
        }
        (Kind::Llm, Was::Llm { n_ctx }) => crate::enhancement::local_ai::local_ai()
            .ok_or_else(|| anyhow::anyhow!("pas d'IA locale"))
            .and_then(|ai| {
                let model_ref = ai.resolve(id)?;
                ai.ensure_loaded(&model_ref, n_ctx).map(|_| ())
            }),
        _ => Ok(()),
    };
    if let Err(e) = result {
        tracing::warn!(id, error = %e, "rechargement apres un retrait refuse impossible");
    }
}
