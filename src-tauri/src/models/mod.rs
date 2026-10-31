// Models on the shared engine store (voice-engine v0.2.2): ids, imports,
// listing, downloads and startup. The Tauri commands that the UI calls stay
// in `commands/` (models, parakeet, llm_models, vad).

pub mod download;
pub mod ids;
pub mod imports;
pub mod list;
pub mod startup;
pub mod store;

pub use store::ModelStoreState;

use tauri::{AppHandle, Manager};

/// This app now uses `id` (the person picked it, or a Power Mode points to
/// it): record it in the store so another app's Delete cannot erase it. A
/// model that is not in the catalog (an import, nothing) is ignored.
pub fn mark_used(app: &AppHandle, id: Option<&str>) {
    let Some(id) = id else { return };
    if voice_engine_models::catalog::find(id).is_none() {
        return;
    }
    if let Some(store) = app.try_state::<ModelStoreState>() {
        if let Err(e) = store.0.shared().use_model(id) {
            tracing::warn!(id, error = %e, "could not record the model as used");
        }
    }
}
