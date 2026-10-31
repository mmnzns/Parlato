// Commandes Tauri pour les modeles de l'IA locale (GGUF) : le catalogue du
// moteur (crate::models, dossier de modeles partage) et les fichiers que la
// personne a apportes (imports).

use std::path::PathBuf;

use tauri::{command, AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use crate::enhancement::providers::llamacpp;
use crate::models::download::{self, Flavor};
use crate::models::list::{self, GgufModelState};
use crate::models::{imports, ModelStoreState};

#[command]
pub fn list_gguf_models(state: State<'_, ModelStoreState>) -> Result<Vec<GgufModelState>, String> {
    Ok(list::gguf_models(&state.0))
}

#[command]
pub async fn download_gguf_model(
    app: AppHandle,
    state: State<'_, ModelStoreState>,
    id: String,
) -> Result<String, String> {
    download::run(&app, state.0.clone(), &id, Flavor::Llm).await
}

#[command]
pub fn cancel_download_gguf_model(state: State<'_, ModelStoreState>, id: String) {
    state.0.cancel_download(&id);
}

#[command]
pub fn delete_gguf_model(
    app: AppHandle,
    state: State<'_, ModelStoreState>,
    id: String,
) -> Result<(), String> {
    let store = state.0.clone();
    let catalog_model = voice_engine_models::catalog::find(&id).is_some();
    // Un import : seul un fichier de LlmModels/imported/ est supprime ; un
    // fichier pose ailleurs par la personne reste ou elle l'a mis.
    if !catalog_model && !imports::find_gguf_import(&store.llm_dir(), &id).is_some_and(|f| f.removable) {
        return Ok(());
    }
    // Si c'est le modele actif, decharger le runtime pour liberer la RAM/VRAM.
    if llamacpp::get_selected_gguf(&app).as_deref() == Some(id.as_str()) {
        if let Some(rt) = crate::enhancement::service::llama_runtime() {
            rt.unload();
        }
        let _ = llamacpp::set_selected_gguf(&app, None);
    }
    if catalog_model {
        return store.release(&id).map(|_| ()).map_err(|e| e.to_string());
    }
    imports::delete_gguf_import(&store.llm_dir(), &id)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[command]
pub async fn import_gguf_model(
    app: AppHandle,
    state: State<'_, ModelStoreState>,
) -> Result<String, String> {
    // Native file picker -> .gguf only.
    let file = app
        .dialog()
        .file()
        .add_filter("GGUF", &["gguf"])
        .blocking_pick_file();
    let Some(fp) = file else {
        return Err(download::CANCELLED.into());
    };
    let p: PathBuf = fp.as_path().map(|p| p.to_path_buf()).ok_or_else(|| "chemin invalide".to_string())?;
    imports::import_gguf(&state.0.llm_dir(), &p).map_err(|e| e.to_string())
}

// ---- Selection + inference settings ---------------------------------------

#[command]
pub fn get_selected_gguf(app: AppHandle) -> Option<String> {
    llamacpp::get_selected_gguf(&app)
}

#[command]
pub fn set_selected_gguf(app: AppHandle, id: Option<String>) -> Result<(), String> {
    crate::models::mark_used(&app, id.as_deref());
    // Si l'ID change, decharger le modele actuellement en memoire.
    let prev = llamacpp::get_selected_gguf(&app);
    if prev != id {
        if let Some(rt) = crate::enhancement::service::llama_runtime() {
            rt.unload();
        }
    }
    llamacpp::set_selected_gguf(&app, id.as_deref()).map_err(|e| e.to_string())
}

#[derive(serde::Serialize)]
pub struct LlamaCppSettings {
    pub n_gpu_layers: u32,
    pub context_size: u32,
    pub max_tokens: u32,
}

#[command]
pub fn get_llamacpp_settings(app: AppHandle) -> LlamaCppSettings {
    LlamaCppSettings {
        n_gpu_layers: llamacpp::get_n_gpu_layers(&app),
        context_size: llamacpp::get_context_size(&app),
        max_tokens: llamacpp::get_max_tokens(&app),
    }
}

#[command]
pub fn set_llamacpp_settings(
    app: AppHandle,
    n_gpu_layers: u32,
    context_size: u32,
    max_tokens: u32,
) -> Result<(), String> {
    // Tout changement de params impose un rechargement : decharge maintenant.
    if let Some(rt) = crate::enhancement::service::llama_runtime() {
        rt.unload();
    }
    llamacpp::set_n_gpu_layers(&app, n_gpu_layers).map_err(|e| e.to_string())?;
    llamacpp::set_context_size(&app, context_size).map_err(|e| e.to_string())?;
    llamacpp::set_max_tokens(&app, max_tokens).map_err(|e| e.to_string())?;
    Ok(())
}

#[command]
pub fn llamacpp_cuda_enabled() -> bool {
    cfg!(feature = "cuda-llama")
}
