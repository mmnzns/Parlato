// Commandes Tauri pour les reglages persistes (modele selectionne, langue, etc.).
//
// Reference VoiceInk : stockage UserDefaults dans VoiceInk. Ici on utilise
// tauri-plugin-store qui ecrit dans un JSON sous AppConfig.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_store::StoreExt;

use crate::text_processing::filler_words;
use crate::transcription::pipeline;

const STORE_FILE: &str = "parla.settings.json";

/// Emet un event Tauri "source:changed" avec la source actuelle.
/// Appele chaque fois que la selection de source (kind / modele) change,
/// que ce soit via une commande UI ou via PowerMode (apply / restore).
/// Permet au frontend (ModelsPage, etc.) de rafraichir sans polling.
pub fn emit_source_changed(app: &AppHandle) {
    let source = get_transcription_source(app.clone());
    if let Err(e) = app.emit("source:changed", &source) {
        tracing::warn!(error = %e, "emit source:changed a echoue");
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TextProcessingSettings {
    pub text_formatting_enabled: bool,
    pub remove_filler_words: bool,
    pub filler_words: Vec<String>,
    pub append_trailing_space: bool,
    pub restore_clipboard_after_paste: bool,
}

#[tauri::command]
pub fn set_selected_whisper_model(app: AppHandle, id: Option<String>) -> Result<(), String> {
    pipeline::set_selected_model(&app, id.as_deref()).map_err(|e| e.to_string())?;
    emit_source_changed(&app);
    Ok(())
}

#[tauri::command]
pub fn get_selected_whisper_model(app: AppHandle) -> Option<String> {
    pipeline::get_selected_model(&app)
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TranscriptionSource {
    /// "local" | "cloud" | "parakeet"
    pub kind: String,
    /// Si kind=local : id du modele Whisper (ggml-base, imported:foo...)
    pub whisper_model_id: Option<String>,
    /// Si kind=cloud : provider id (groq, deepgram, ...)
    pub cloud_provider: Option<String>,
    /// Si kind=cloud : model id chez ce provider (whisper-large-v3-turbo, nova-3, ...)
    pub cloud_model: Option<String>,
    /// Si kind=parakeet : id de variante (parakeet-tdt-0.6b-v2, ...).
    pub parakeet_model_id: Option<String>,
}

impl Default for TranscriptionSource {
    fn default() -> Self {
        Self {
            kind: "local".into(),
            whisper_model_id: None,
            cloud_provider: None,
            cloud_model: None,
            parakeet_model_id: None,
        }
    }
}

#[tauri::command]
pub fn get_transcription_source(app: AppHandle) -> TranscriptionSource {
    let Some(store) = app.store(STORE_FILE).ok() else {
        return TranscriptionSource::default();
    };
    TranscriptionSource {
        kind: store
            .get("transcription_source_kind")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| "local".into()),
        whisper_model_id: store
            .get("selected_whisper_model")
            .and_then(|v| v.as_str().map(|s| s.to_string())),
        cloud_provider: store
            .get("selected_cloud_provider")
            .and_then(|v| v.as_str().map(|s| s.to_string())),
        cloud_model: store
            .get("selected_cloud_model")
            .and_then(|v| v.as_str().map(|s| s.to_string())),
        parakeet_model_id: store
            .get("selected_parakeet_model")
            .and_then(|v| v.as_str().map(|s| s.to_string())),
    }
}

/// Lightweight source kind switch. Does NOT touch the per-kind selection
/// fields (whisper_model_id, cloud_provider, cloud_model, parakeet_model_id)
/// so the user can flip between tabs without losing their current selection
/// in each tab.
#[tauri::command]
pub fn set_transcription_kind(app: AppHandle, kind: String) -> Result<(), String> {
    let store = app.store(STORE_FILE).map_err(|e| e.to_string())?;
    store.set("transcription_source_kind", serde_json::Value::String(kind));
    store.save().map_err(|e| e.to_string())?;
    emit_source_changed(&app);
    Ok(())
}

#[tauri::command]
pub fn set_transcription_source(
    app: AppHandle,
    source: TranscriptionSource,
) -> Result<(), String> {
    let store = app.store(STORE_FILE).map_err(|e| e.to_string())?;
    store.set(
        "transcription_source_kind",
        serde_json::Value::String(source.kind),
    );
    match source.cloud_provider {
        Some(v) => store.set("selected_cloud_provider", serde_json::Value::String(v)),
        None => {
            store.delete("selected_cloud_provider");
        }
    }
    match source.cloud_model {
        Some(v) => store.set("selected_cloud_model", serde_json::Value::String(v)),
        None => {
            store.delete("selected_cloud_model");
        }
    }
    match source.parakeet_model_id {
        Some(v) => store.set("selected_parakeet_model", serde_json::Value::String(v)),
        None => {
            store.delete("selected_parakeet_model");
        }
    }
    store.save().map_err(|e| e.to_string())?;
    emit_source_changed(&app);
    Ok(())
}

#[tauri::command]
pub fn get_text_processing_settings(app: AppHandle) -> TextProcessingSettings {
    TextProcessingSettings {
        text_formatting_enabled: pipeline::get_text_formatting_enabled(&app),
        remove_filler_words: filler_words::is_enabled(&app),
        filler_words: filler_words::current_list(&app),
        append_trailing_space: pipeline::get_append_trailing_space(&app),
        restore_clipboard_after_paste: pipeline::get_restore_clipboard(&app),
    }
}

#[tauri::command]
pub fn set_text_formatting_enabled(app: AppHandle, enabled: bool) -> Result<(), String> {
    let store = app.store(STORE_FILE).map_err(|e| e.to_string())?;
    store.set(
        "text_formatting_enabled",
        serde_json::Value::Bool(enabled),
    );
    store.save().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_remove_filler_words(app: AppHandle, enabled: bool) -> Result<(), String> {
    filler_words::set_enabled(&app, enabled)
}

#[tauri::command]
pub fn set_filler_words(app: AppHandle, words: Vec<String>) -> Result<(), String> {
    filler_words::set_list(&app, words)
}

#[tauri::command]
pub fn set_append_trailing_space(app: AppHandle, enabled: bool) -> Result<(), String> {
    let store = app.store(STORE_FILE).map_err(|e| e.to_string())?;
    store.set(
        "append_trailing_space",
        serde_json::Value::Bool(enabled),
    );
    store.save().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_restore_clipboard_after_paste(
    app: AppHandle,
    enabled: bool,
) -> Result<(), String> {
    let store = app.store(STORE_FILE).map_err(|e| e.to_string())?;
    store.set(
        "restore_clipboard_after_paste",
        serde_json::Value::Bool(enabled),
    );
    store.save().map_err(|e| e.to_string())
}

const KEY_CLOSE_TO_TRAY: &str = "close_to_tray";

/// Close-to-tray behavior for the main window. Default: true (hide instead
/// of quit when the X button is clicked). The tray provides "Quit Parla"
/// as the explicit exit path.
pub fn close_to_tray_enabled(app: &AppHandle) -> bool {
    app.store(STORE_FILE)
        .ok()
        .and_then(|s| s.get(KEY_CLOSE_TO_TRAY).and_then(|v| v.as_bool()))
        .unwrap_or(true)
}

#[tauri::command]
pub fn get_close_to_tray(app: AppHandle) -> bool {
    close_to_tray_enabled(&app)
}

#[tauri::command]
pub fn set_close_to_tray(app: AppHandle, enabled: bool) -> Result<(), String> {
    let store = app.store(STORE_FILE).map_err(|e| e.to_string())?;
    store.set(KEY_CLOSE_TO_TRAY, serde_json::Value::Bool(enabled));
    store.save().map_err(|e| e.to_string())
}

const KEY_HIDE_DOCK: &str = "hide_dock_icon";

/// Parlato : macOS, Parlato seulement dans la barre des menus, sans icone
/// dans le Dock. Off par defaut. Sans effet ailleurs.
pub fn hide_dock_enabled(app: &AppHandle) -> bool {
    app.store(STORE_FILE)
        .ok()
        .and_then(|s| s.get(KEY_HIDE_DOCK).and_then(|v| v.as_bool()))
        .unwrap_or(false)
}

/// Applique le reglage (au demarrage et a chaque changement).
pub fn apply_dock_icon(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    if let Err(e) = app.set_dock_visibility(!hide_dock_enabled(app)) {
        tracing::warn!("dock visibility: {e}");
    }
    #[cfg(not(target_os = "macos"))]
    let _ = app;
}

#[tauri::command]
pub fn get_hide_dock_icon(app: AppHandle) -> bool {
    hide_dock_enabled(&app)
}

#[tauri::command]
pub fn set_hide_dock_icon(app: AppHandle, enabled: bool) -> Result<(), String> {
    let store = app.store(STORE_FILE).map_err(|e| e.to_string())?;
    store.set(KEY_HIDE_DOCK, serde_json::Value::Bool(enabled));
    store.save().map_err(|e| e.to_string())?;
    apply_dock_icon(&app);
    Ok(())
}

const KEY_TRAY_NOTICE_SHOWN: &str = "close_to_tray_notice_shown";

/// Parlato: true the first time the window is hidden to the tray, false
/// afterwards. Marks the notice as shown so it appears only once.
pub fn take_first_tray_hide(app: &AppHandle) -> bool {
    let Ok(store) = app.store(STORE_FILE) else {
        return false;
    };
    if store
        .get(KEY_TRAY_NOTICE_SHOWN)
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        return false;
    }
    store.set(KEY_TRAY_NOTICE_SHOWN, serde_json::Value::Bool(true));
    let _ = store.save();
    true
}

// -- Dictation language (whisper / cloud / parakeet) -----------------------

/// Lit la langue de dictee active. "auto" = auto-detect, code ISO sinon.
/// Stockee sous la cle `whisper_language` (legacy : la cle existait avant
/// l'ajout des sources cloud / parakeet, on la garde pour compat).
#[tauri::command]
pub fn get_dictation_language(app: AppHandle) -> String {
    pipeline::get_language(&app).unwrap_or_else(|| "auto".into())
}

/// Ecrit la langue de dictee. `code` = "auto" | "en" | "fr" | etc.
/// "auto" est stocke tel quel ; le pipeline le mappe vers None au runtime.
#[tauri::command]
pub fn set_dictation_language(app: AppHandle, code: String) -> Result<(), String> {
    let store = app.store(STORE_FILE).map_err(|e| e.to_string())?;
    store.set("whisper_language", serde_json::Value::String(code));
    store.save().map_err(|e| e.to_string())
}

// -- Speech model CPU threads, Whisper and Parakeet (Parlato) ---------------

#[derive(serde::Serialize)]
pub struct SpeechThreads {
    /// Choix enregistre, 0 = automatique.
    pub value: usize,
    /// Ce que "automatique" utilise sur cet ordinateur.
    pub auto: usize,
    /// Threads disponibles sur cet ordinateur.
    pub max: usize,
}

#[tauri::command]
pub fn get_speech_threads(app: AppHandle) -> SpeechThreads {
    SpeechThreads {
        value: pipeline::get_speech_threads(&app),
        auto: crate::transcription::whisper::auto_threads() as usize,
        max: crate::transcription::whisper::max_threads() as usize,
    }
}

#[tauri::command]
pub fn set_speech_threads(app: AppHandle, threads: usize) -> Result<(), String> {
    pipeline::set_speech_threads(&app, threads).map_err(|e| e.to_string())
}

// -- Parakeet on the graphics card (Parlato) -------------------------------

#[derive(serde::Serialize)]
pub struct ParakeetGpu {
    /// Choix enregistre (faux par defaut).
    pub enabled: bool,
    /// Ce build sait utiliser la carte graphique (Windows, ou CUDA).
    pub available: bool,
    /// La carte graphique a echoue depuis le lancement : le processeur sert.
    pub failed: bool,
    /// Ce qui a tourne au dernier chargement : "cpu", "directml", "cuda" ou
    /// "cpu-fallback".
    pub provider: String,
}

#[tauri::command]
pub fn get_parakeet_gpu(app: AppHandle) -> ParakeetGpu {
    let engine = app.try_state::<crate::transcription::parakeet::ParakeetEngineState>();
    ParakeetGpu {
        enabled: pipeline::get_parakeet_gpu(&app),
        available: crate::transcription::parakeet::gpu_available(),
        failed: engine.as_ref().map(|e| e.0.gpu_failed()).unwrap_or(false),
        provider: engine.map(|e| e.0.provider()).unwrap_or("cpu").to_string(),
    }
}

/// Le modele se recharge a la prochaine utilisation (comme pour les threads).
#[tauri::command]
pub fn set_parakeet_gpu(app: AppHandle, enabled: bool) -> Result<(), String> {
    pipeline::set_parakeet_gpu(&app, enabled).map_err(|e| e.to_string())
}

/// Parlato : duree (s) pendant laquelle les modeles restent charges apres
/// une dictee, et la valeur recommandee.
#[derive(Debug, Serialize)]
pub struct ModelKeepAlive {
    pub value: u64,
    pub recommended: u64,
}

#[tauri::command]
pub fn get_model_keep_alive(app: AppHandle) -> ModelKeepAlive {
    ModelKeepAlive {
        value: pipeline::get_model_keep_alive(&app),
        recommended: pipeline::DEFAULT_KEEP_ALIVE_SECS,
    }
}

#[tauri::command]
pub fn set_model_keep_alive(app: AppHandle, secs: u64) -> Result<(), String> {
    pipeline::set_model_keep_alive(&app, secs).map_err(|e| e.to_string())
}

// -- System mute during recording (VoiceInk MediaController) ----------------

#[tauri::command]
pub fn get_system_mute_enabled(app: AppHandle) -> bool {
    crate::audio::mute::is_enabled(&app)
}

#[tauri::command]
pub fn set_system_mute_enabled(app: AppHandle, enabled: bool) -> Result<(), String> {
    crate::audio::mute::set_enabled(&app, enabled).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_audio_resumption_delay(app: AppHandle) -> f64 {
    crate::audio::mute::resumption_delay(&app).as_secs_f64()
}

#[tauri::command]
pub fn set_audio_resumption_delay(app: AppHandle, secs: f64) -> Result<(), String> {
    crate::audio::mute::set_resumption_delay(&app, secs).map_err(|e| e.to_string())
}

// -- Sound feedback cues (VoiceInk SoundManager) ----------------------------

#[tauri::command]
pub fn get_sound_feedback_enabled(app: AppHandle) -> bool {
    crate::audio::feedback::is_enabled(&app)
}

#[tauri::command]
pub fn set_sound_feedback_enabled(app: AppHandle, enabled: bool) -> Result<(), String> {
    crate::audio::feedback::set_enabled(&app, enabled).map_err(|e| e.to_string())
}

// -- Cloud transcription timeout (VoiceInk CloudTranscriptionSettings) ------

const KEY_CLOUD_TIMEOUT: &str = "cloud_transcription_timeout_secs";

/// Timeout batch cloud persiste (secondes), borne 10 s .. 30 min.
pub fn cloud_timeout_secs(app: &AppHandle) -> u64 {
    use crate::transcription::cloud::http;
    app.store(STORE_FILE)
        .ok()
        .and_then(|s| s.get(KEY_CLOUD_TIMEOUT).and_then(|v| v.as_u64()))
        .unwrap_or(http::DEFAULT_BATCH_TIMEOUT_SECS)
        .clamp(http::MIN_BATCH_TIMEOUT_SECS, http::MAX_BATCH_TIMEOUT_SECS)
}

#[tauri::command]
pub fn get_cloud_transcription_timeout(app: AppHandle) -> u64 {
    cloud_timeout_secs(&app)
}

#[tauri::command]
pub fn set_cloud_transcription_timeout(app: AppHandle, secs: u64) -> Result<(), String> {
    use crate::transcription::cloud::http;
    let clamped = secs.clamp(http::MIN_BATCH_TIMEOUT_SECS, http::MAX_BATCH_TIMEOUT_SECS);
    let store = app.store(STORE_FILE).map_err(|e| e.to_string())?;
    store.set(KEY_CLOUD_TIMEOUT, serde_json::Value::from(clamped));
    store.save().map_err(|e| e.to_string())?;
    http::set_batch_timeout_secs(clamped);
    Ok(())
}

// -- Selected input device (VoiceInk AudioDeviceManager) --------------------

#[tauri::command]
pub fn get_selected_input_device(app: AppHandle) -> Option<String> {
    crate::audio::device::selected_input_device(&app)
}

#[tauri::command]
pub fn set_selected_input_device(app: AppHandle, name: Option<String>) -> Result<(), String> {
    crate::audio::device::set_selected_input_device(&app, name).map_err(|e| e.to_string())?;
    crate::tray::refresh(&app);
    Ok(())
}

// -- UI language, mirrored for the native tray menu -------------------------

const KEY_UI_LANGUAGE: &str = "ui_language";

/// Langue de l'interface (en / fr / es), ecrite par le frontend a chaque
/// changement i18next pour que le menu tray natif soit traduit.
pub fn ui_language(app: &AppHandle) -> String {
    app.store(STORE_FILE)
        .ok()
        .and_then(|s| s.get(KEY_UI_LANGUAGE))
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_else(|| "en".into())
}

#[tauri::command]
pub fn get_ui_language(app: AppHandle) -> String {
    ui_language(&app)
}

#[tauri::command]
pub fn set_ui_language(app: AppHandle, language: String) -> Result<(), String> {
    let store = app.store(STORE_FILE).map_err(|e| e.to_string())?;
    let changed = ui_language(&app) != language;
    store.set(KEY_UI_LANGUAGE, serde_json::Value::String(language));
    store.save().map_err(|e| e.to_string())?;
    if changed {
        crate::tray::refresh(&app);
    }
    Ok(())
}

// -- Escape cancel hint (VoiceInk RecorderPanelShortcutManager) -------------

/// Reaffiche l'astuce "Appuyez encore sur Echap pour annuler" au prochain
/// enregistrement (bouton reset du raccourci d'annulation).
#[tauri::command]
pub fn reset_escape_hint(app: AppHandle) -> Result<(), String> {
    let store = app.store(STORE_FILE).map_err(|e| e.to_string())?;
    store.delete(crate::transcription::engine::ESCAPE_HINT_KEY);
    store.save().map_err(|e| e.to_string())
}

// -- Legacy cloud model ids --------------------------------------------------

/// Remplace, une fois au demarrage, les identifiants de modeles cloud retires
/// du catalogue par leur successeur, dans la selection courante et dans les
/// profils Power Mode (`catalog::legacy_cloud_model_successor`). Sans cela la
/// page Modeles IA n'affiche plus le modele actif et le streaming n'est plus
/// detecte pour ces identifiants.
pub fn migrate_legacy_cloud_model_ids(app: &AppHandle) {
    use crate::transcription::cloud::catalog::legacy_cloud_model_successor;

    if let Ok(store) = app.store(STORE_FILE) {
        let provider = store
            .get("selected_cloud_provider")
            .and_then(|v| v.as_str().map(String::from));
        let model = store
            .get("selected_cloud_model")
            .and_then(|v| v.as_str().map(String::from));
        if let (Some(p), Some(m)) = (provider, model) {
            if let Some(next) = legacy_cloud_model_successor(&p, &m) {
                store.set("selected_cloud_model", serde_json::Value::String(next.into()));
                let _ = store.save();
                tracing::info!(provider = %p, from = %m, to = next, "selected cloud model migrated");
            }
        }
    }

    if let Ok(mut configs) = crate::power_mode::config::load_all(app) {
        let mut changed = false;
        for c in configs.iter_mut() {
            if let (Some(p), Some(m)) = (c.cloud_provider.as_deref(), c.cloud_model.as_deref()) {
                if let Some(next) = legacy_cloud_model_successor(p, m) {
                    c.cloud_model = Some(next.to_string());
                    changed = true;
                }
            }
        }
        if changed {
            if let Err(e) = crate::power_mode::config::save_all(app, &configs) {
                tracing::warn!(error = %e, "power mode cloud model migration failed");
            }
        }
    }
}
