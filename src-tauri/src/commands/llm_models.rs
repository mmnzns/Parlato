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
pub async fn delete_gguf_model(
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
    // Le modele charge (le sien, ou le choisi) est decharge d'abord : son verrou
    // du dossier partage et son fichier. `unload` attend une generation en
    // cours : sur un fil, pas sur celui de l'interface.
    if llamacpp::get_selected_gguf(&app).as_deref() == Some(id.as_str()) {
        let _ = llamacpp::set_selected_gguf(&app, None);
    }
    if let Some(ai) = crate::enhancement::local_ai::local_ai() {
        if ai.is_loaded(&id) {
            let _ = tauri::async_runtime::spawn_blocking(move || ai.unload()).await;
        }
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
    let prev = llamacpp::get_selected_gguf(&app);
    llamacpp::set_selected_gguf(&app, id.as_deref()).map_err(|e| e.to_string())?;
    // Si l'ID change, le modele en memoire est decharge et le nouveau se
    // charge en arriere-plan (rien ne l'attend).
    if prev != id {
        reload_in_background(&app);
    }
    Ok(())
}

/// Decharge le modele en memoire puis charge celui que la configuration veut
/// (s'il y en a un), sur un fil.
pub fn reload_in_background(app: &AppHandle) {
    let Some(ai) = crate::enhancement::local_ai::local_ai() else {
        return;
    };
    let app = app.clone();
    let _ = std::thread::Builder::new()
        .name("parlato-llm-reload".into())
        .spawn(move || {
            ai.unload();
            crate::enhancement::local_ai::preload_if_wanted(&app);
        });
}

#[derive(serde::Serialize)]
pub struct LlamaCppSettings {
    pub context_size: u32,
    pub max_tokens: u32,
}

#[command]
pub fn get_llamacpp_settings(app: AppHandle) -> LlamaCppSettings {
    LlamaCppSettings {
        context_size: llamacpp::get_context_size(&app),
        max_tokens: llamacpp::get_max_tokens(&app),
    }
}

#[command]
pub fn set_llamacpp_settings(
    app: AppHandle,
    context_size: u32,
    max_tokens: u32,
) -> Result<(), String> {
    llamacpp::set_context_size(&app, context_size).map_err(|e| e.to_string())?;
    llamacpp::set_max_tokens(&app, max_tokens).map_err(|e| e.to_string())?;
    // Un autre contexte impose un rechargement.
    reload_in_background(&app);
    Ok(())
}

/// Ou l'IA locale tourne (ou tournera) : "metal", "vulkan" ou "cpu". Le moteur
/// le dit a chaque chargement et chaque reponse.
#[command]
pub fn llamacpp_backend() -> String {
    crate::enhancement::local_ai::local_ai()
        .map(|ai| ai.backend())
        .unwrap_or_else(|| "cpu".into())
}
