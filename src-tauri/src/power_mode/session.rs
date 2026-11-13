// Gestion de session Power Mode : snapshot de l'etat global avant
// d'appliquer une config, puis restauration.
//
// Reference VoiceInk : PowerMode/PowerModeSessionManager.swift.
// On snapshot les reglages impactes par une config (enhancement, prompt
// actif, LLM provider/model, source transcription + sous-options, langue)
// avant d'overrider puis on restaure au stop si auto_restore est active.
//
// Stockage : la session vit en memoire (Mutex). Parlato : la baseline est en
// plus ecrite dans le store (`PENDING_BASELINE_KEY`) au debut d'une session
// et effacee a sa fin ; si l'application s'arrete au milieu d'une dictee
// (plantage, fermeture forcee, relance de mise a jour), `restore_pending_baseline`
// la remet au lancement suivant au lieu de laisser les reglages du mode.

use anyhow::{anyhow, Result};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

use tauri::Manager;
use tracing::{info, warn};

use crate::enhancement::prompts as prompt_store;
use crate::enhancement::service as llm_service;

use super::active_window::foreground_window;
use super::browser_url::extract_url;
use super::config::{self, PowerModeConfig};
use super::matcher::resolve;

const SETTINGS_FILE: &str = "parla.settings.json";

/// Cle du store : baseline de la session en cours, tant qu'elle dure.
const PENDING_BASELINE_KEY: &str = "power_mode_pending_baseline";

/// Snapshot des reglages globaux affectes par Power Mode.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Baseline {
    pub enhancement_enabled: bool,
    pub use_screen_capture: bool,
    pub active_prompt_id: Option<String>,
    pub llm_provider_id: Option<String>,
    pub llm_model: Option<String>,
    pub transcription_kind: Option<String>,
    pub whisper_model_id: Option<String>,
    pub cloud_provider: Option<String>,
    pub cloud_model: Option<String>,
    pub parakeet_model_id: Option<String>,
    pub language: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PowerSession {
    /// Parlato : numero de la session (croissant, jamais 0). Une dictee garde
    /// le numero de sa session : sa fin ne restaure rien si une dictee plus
    /// recente a pris la main entre-temps.
    #[serde(default)]
    pub id: u64,
    pub config_id: String,
    pub config_name: String,
    pub emoji: String,
    pub baseline: Baseline,
}

#[derive(Default)]
pub struct PowerSessionState(pub Mutex<Option<PowerSession>>);

static NEXT_SESSION_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn next_session_id() -> u64 {
    NEXT_SESSION_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
}

/// Baseline d'une session qui demarre. Si une session est deja active (une
/// dictee precedente est encore en traitement), sa baseline d'origine est
/// conservee : l'etat courant du store porte deja les reglages de son profil
/// et ne doit jamais devenir la "baseline" a restaurer. Sinon `snapshot`
/// lit l'etat du store.
fn baseline_for_new_session(
    existing: Option<&PowerSession>,
    snapshot: impl FnOnce() -> Baseline,
) -> Baseline {
    match existing {
        Some(s) => s.baseline.clone(),
        None => snapshot(),
    }
}

/// Parlato : session Power Mode de l'enregistrement en cours. `0` = pas
/// d'enregistrement ; `RECORDING_UNOWNED` = enregistrement sans mode
/// correspondant (il ne possede aucune session, meme si celle d'une dictee
/// precedente est encore active) ; sinon l'id de la session ouverte par cet
/// enregistrement.
static RECORDING_OWNER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
const RECORDING_UNOWNED: u64 = u64::MAX;

/// La session de `slot` si elle appartient a l'enregistrement (`owner`).
fn owned_session(slot: &Option<PowerSession>, owner: u64) -> Option<PowerSession> {
    slot.as_ref().filter(|s| s.id == owner).cloned()
}

/// Numero d'une session choisie a la main (raccourci, tray). Pendant un
/// enregistrement (`owner != 0`) la session existante n'est reprise que si
/// elle est celle de l'enregistrement ; sinon (aucun mode n'avait matche,
/// l'existante est celle d'une dictee precedente) une nouvelle est creee et
/// la baseline seule est conservee. Hors enregistrement : comportement
/// d'origine (numero de l'existante, sinon nouveau).
fn id_for_manual_select(owner: u64, existing: Option<u64>, fresh: impl FnOnce() -> u64) -> u64 {
    match existing {
        Some(id) if owner == 0 || id == owner => id,
        _ => fresh(),
    }
}

/// Retire la session active si c'est bien la session `id` (sinon une session
/// plus recente a pris la place et rien ne doit etre restaure).
fn take_if_current(slot: &mut Option<PowerSession>, id: u64) -> Option<PowerSession> {
    if slot.as_ref().is_some_and(|s| s.id == id) {
        slot.take()
    } else {
        None
    }
}

/// Parlato : l'hote d'une URL, sans schema, identifiants, port, chemin ni
/// requete. C'est tout ce qui peut aller dans un log (le chemin et la requete
/// peuvent porter des jetons ou des donnees personnelles).
fn url_host(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host_port = authority.rsplit('@').next().unwrap_or("");
    host_port.split(':').next().unwrap_or("")
}

// -- Baseline persistee -----------------------------------------------------

/// Ce que le lancement doit faire d'une baseline laissee par une session
/// interrompue.
#[derive(Debug, PartialEq)]
enum PendingAction {
    /// Rien n'etait en attente.
    Nothing,
    /// Remettre ces reglages, puis effacer.
    Restore(Box<Baseline>),
    /// Effacer sans rien remettre (restauration automatique desactivee : la
    /// personne veut garder les reglages du mode).
    ClearOnly,
}

fn baseline_from_value(value: Option<&serde_json::Value>) -> Option<Baseline> {
    serde_json::from_value(value?.clone()).ok()
}

fn pending_action(value: Option<&serde_json::Value>, auto_restore: bool) -> PendingAction {
    match (value, baseline_from_value(value)) {
        (None, _) => PendingAction::Nothing,
        // Une valeur illisible est effacee : elle ne servirait a rien.
        (Some(_), None) => PendingAction::ClearOnly,
        (Some(_), Some(b)) if auto_restore => PendingAction::Restore(Box::new(b)),
        (Some(_), Some(_)) => PendingAction::ClearOnly,
    }
}

/// Ecrit la baseline avant d'appliquer un mode : si l'application s'arrete
/// entre-temps, le lancement suivant sait quoi remettre.
fn persist_baseline(app: &AppHandle, baseline: &Baseline) {
    let Ok(store) = app.store(SETTINGS_FILE) else {
        return;
    };
    match serde_json::to_value(baseline) {
        Ok(v) => store.set(PENDING_BASELINE_KEY, v),
        Err(e) => {
            warn!("power_mode baseline: {e}");
            return;
        }
    }
    if let Err(e) = store.save() {
        warn!("power_mode baseline save: {e}");
    }
}

fn clear_persisted_baseline(app: &AppHandle) {
    let Ok(store) = app.store(SETTINGS_FILE) else {
        return;
    };
    if store.delete(PENDING_BASELINE_KEY) {
        let _ = store.save();
    }
}

/// Au lancement : si une session a ete interrompue (baseline restee dans le
/// store), remet les reglages d'avant puis l'efface. A appeler avant toute
/// dictee.
pub fn restore_pending_baseline(app: &AppHandle) {
    let Ok(store) = app.store(SETTINGS_FILE) else {
        return;
    };
    let action = pending_action(store.get(PENDING_BASELINE_KEY).as_ref(), config::is_auto_restore(app));
    match action {
        PendingAction::Nothing => {}
        PendingAction::ClearOnly => {
            info!("Power Mode : baseline en attente effacee sans restauration");
            clear_persisted_baseline(app);
        }
        PendingAction::Restore(b) => {
            match restore(app, &b) {
                Ok(()) => {
                    let _ = config::set_active_id(app, None);
                    info!("Power Mode : reglages d'avant la session interrompue restaures");
                }
                Err(e) => warn!("power_mode restore au lancement: {e}"),
            }
            // Effacee meme si la restauration a echoue : sinon l'echec se
            // repeterait a chaque lancement.
            clear_persisted_baseline(app);
        }
    }
}

// -- Snapshot ---------------------------------------------------------------

pub fn snapshot(app: &AppHandle) -> Baseline {
    let store = app.store(SETTINGS_FILE).ok();

    let enhancement_enabled = prompt_store::is_enhancement_enabled(app);
    let use_screen_capture = crate::screen_context::service::is_enabled(app);
    let active_prompt_id = prompt_store::get_active_prompt_id(app);

    let sel = llm_service::get_selection(app);
    let (llm_provider_id, llm_model) = match sel {
        Some(s) => (Some(s.provider_id), Some(s.model)),
        None => (None, None),
    };

    let get_str = |k: &str| -> Option<String> {
        store
            .as_ref()
            .and_then(|s| s.get(k).and_then(|v| v.as_str().map(String::from)))
    };

    Baseline {
        enhancement_enabled,
        use_screen_capture,
        active_prompt_id,
        llm_provider_id,
        llm_model,
        transcription_kind: get_str("transcription_source_kind"),
        whisper_model_id: get_str("selected_whisper_model"),
        cloud_provider: get_str("selected_cloud_provider"),
        cloud_model: get_str("selected_cloud_model"),
        parakeet_model_id: get_str("selected_parakeet_model"),
        language: get_str("whisper_language"),
    }
}

// -- Apply ------------------------------------------------------------------

/// Applique les overrides de `cfg`. Ne touche pas aux champs None de la
/// config (laisse la baseline en place).
pub fn apply(app: &AppHandle, cfg: &PowerModeConfig) -> Result<()> {
    let store = app
        .store(SETTINGS_FILE)
        .map_err(|e| anyhow!("store: {e}"))?;

    prompt_store::set_enhancement_enabled(app, cfg.is_enhancement_enabled)?;

    if let Some(b) = cfg.use_screen_capture {
        crate::screen_context::service::set_enabled(app, b)?;
    }

    if let Some(pid) = &cfg.selected_prompt_id {
        prompt_store::set_active_prompt_id(app, Some(pid))?;
    }

    match (&cfg.selected_llm_provider, &cfg.selected_llm_model) {
        (Some(p), Some(m)) => {
            llm_service::set_selection(app, p, m)?;
        }
        (Some(p), None) => {
            llm_service::set_selection(app, p, "")?;
        }
        _ => {}
    }

    if let Some(kind) = &cfg.transcription_kind {
        store.set(
            "transcription_source_kind",
            serde_json::Value::String(kind.clone()),
        );
        match kind.as_str() {
            "local" => {
                if let Some(id) = &cfg.whisper_model_id {
                    store.set(
                        "selected_whisper_model",
                        serde_json::Value::String(id.clone()),
                    );
                }
            }
            "cloud" => {
                if let Some(p) = &cfg.cloud_provider {
                    store.set(
                        "selected_cloud_provider",
                        serde_json::Value::String(p.clone()),
                    );
                }
                if let Some(m) = &cfg.cloud_model {
                    store.set(
                        "selected_cloud_model",
                        serde_json::Value::String(m.clone()),
                    );
                }
            }
            "parakeet" => {
                if let Some(id) = &cfg.parakeet_model_id {
                    store.set(
                        "selected_parakeet_model",
                        serde_json::Value::String(id.clone()),
                    );
                }
            }
            _ => {}
        }
    }

    if let Some(lang) = &cfg.language {
        store.set("whisper_language", serde_json::Value::String(lang.clone()));
    }

    store.save().map_err(|e| anyhow!("store save: {e}"))?;
    crate::commands::settings::emit_source_changed(app);
    Ok(())
}

// -- Restore ----------------------------------------------------------------

pub fn restore(app: &AppHandle, b: &Baseline) -> Result<()> {
    let store = app
        .store(SETTINGS_FILE)
        .map_err(|e| anyhow!("store: {e}"))?;

    prompt_store::set_enhancement_enabled(app, b.enhancement_enabled)?;
    crate::screen_context::service::set_enabled(app, b.use_screen_capture)?;

    match &b.active_prompt_id {
        Some(id) => prompt_store::set_active_prompt_id(app, Some(id))?,
        None => prompt_store::set_active_prompt_id(app, None)?,
    }

    match (&b.llm_provider_id, &b.llm_model) {
        (Some(p), Some(m)) => llm_service::set_selection(app, p, m)?,
        (Some(p), None) => llm_service::set_selection(app, p, "")?,
        (None, _) => {}
    }

    set_or_delete(&store, "transcription_source_kind", b.transcription_kind.as_deref());
    set_or_delete(&store, "selected_whisper_model", b.whisper_model_id.as_deref());
    set_or_delete(&store, "selected_cloud_provider", b.cloud_provider.as_deref());
    set_or_delete(&store, "selected_cloud_model", b.cloud_model.as_deref());
    set_or_delete(&store, "selected_parakeet_model", b.parakeet_model_id.as_deref());
    set_or_delete(&store, "whisper_language", b.language.as_deref());

    store.save().map_err(|e| anyhow!("store save: {e}"))?;
    crate::commands::settings::emit_source_changed(app);
    Ok(())
}

fn set_or_delete(
    store: &std::sync::Arc<tauri_plugin_store::Store<tauri::Wry>>,
    key: &str,
    value: Option<&str>,
) {
    match value {
        Some(v) => store.set(key, serde_json::Value::String(v.to_string())),
        None => {
            store.delete(key);
        }
    }
}

// -- High-level API (called by the recording pipeline) ---------------------

/// Parlato : `begin_session` pour un enregistrement qui demarre ; celui-ci ne
/// possede que la session qu'il ouvre lui-meme.
pub fn begin_recording_session(app: &AppHandle) -> Option<PowerSession> {
    RECORDING_OWNER.store(RECORDING_UNOWNED, std::sync::atomic::Ordering::SeqCst);
    let session = begin_session(app)?;
    RECORDING_OWNER.store(session.id, std::sync::atomic::Ordering::SeqCst);
    Some(session)
}

/// Parlato : a l'arret de l'enregistrement, la session de CET enregistrement
/// (jamais celle d'une dictee precedente encore en traitement). Libere la
/// propriete : la dictee porte desormais sa session.
pub fn take_recording_session(app: &AppHandle) -> Option<PowerSession> {
    let owner = RECORDING_OWNER.swap(0, std::sync::atomic::Ordering::SeqCst);
    let state = app.try_state::<PowerSessionState>()?;
    let slot = state.0.lock();
    owned_session(&slot, owner)
}

/// Parlato : annulation ou echec de demarrage. Termine seulement la session
/// de cet enregistrement ; `true` si une session a bien pris fin.
pub fn end_recording_session(app: &AppHandle) -> bool {
    let owner = RECORDING_OWNER.swap(0, std::sync::atomic::Ordering::SeqCst);
    if owner == 0 || owner == RECORDING_UNOWNED {
        return false;
    }
    end_session(app, owner)
}

/// Au demarrage d'un enregistrement : detecte la fenetre active + URL,
/// cherche la meilleure config, snapshot la baseline, applique la config.
/// Si aucune config ne matche : aucun effet.
pub fn begin_session(app: &AppHandle) -> Option<PowerSession> {
    let state = app.try_state::<PowerSessionState>()?;
    let configs = match config::load_all(app) {
        Ok(c) => c,
        Err(e) => {
            warn!("power_mode load: {e}");
            return None;
        }
    };
    if configs.is_empty() {
        return None;
    }

    let active = match foreground_window() {
        Ok(a) => a,
        Err(e) => {
            warn!("foreground_window: {e}");
            return None;
        }
    };
    let url = extract_url(&active);
    let matched = resolve(&configs, &active, url.as_deref())?;

    // Parlato : une session deja active (dictee precedente en cours de
    // traitement) garde sa baseline d'origine, voir `baseline_for_new_session`.
    let existing = state.0.lock().clone();
    let baseline = baseline_for_new_session(existing.as_ref(), || snapshot(app));
    if existing.is_none() {
        persist_baseline(app, &baseline);
    }
    if let Err(e) = apply(app, matched) {
        warn!("power_mode apply: {e}");
        return None;
    }

    let session = PowerSession {
        id: next_session_id(),
        config_id: matched.id.clone(),
        config_name: matched.name.clone(),
        emoji: matched.emoji.clone(),
        baseline,
    };
    *state.0.lock() = Some(session.clone());
    let _ = config::set_active_id(app, Some(&session.config_id));
    info!(
        config = %session.config_name,
        exe = %active.exe_name,
        // Parlato : l'hote seulement, jamais le chemin ni la requete de l'URL.
        host = url.as_deref().map(url_host).unwrap_or(""),
        "Power Mode actif"
    );
    Some(session)
}

/// A la fin d'une dictee (success, echec ou cancel) : restaure la baseline
/// si auto_restore est active, sinon laisse la config en place. Parlato :
/// seulement si la session active est encore la session `id` de cette
/// dictee ; une dictee plus recente a pu en demarrer une autre, que cette
/// fin ne doit pas defaire. Retourne `true` si cette session a pris fin.
pub fn end_session(app: &AppHandle, id: u64) -> bool {
    let Some(state) = app.try_state::<PowerSessionState>() else {
        return false;
    };
    let session = take_if_current(&mut state.0.lock(), id);
    let Some(session) = session else {
        return false;
    };
    // La session est finie : plus rien a remettre au prochain lancement.
    clear_persisted_baseline(app);
    if config::is_auto_restore(app) {
        if let Err(e) = restore(app, &session.baseline) {
            warn!("power_mode restore: {e}");
        } else {
            let _ = config::set_active_id(app, None);
            info!(config = %session.config_name, "Power Mode baseline restauree");
        }
    }
    true
}

/// Parlato : touche d'envoi du mode actif, pressee apres le collage.
/// Parlato : celle de la session de la dictee (`session`), pas de la session
/// courante, qui peut deja etre celle de la dictee suivante.
pub fn auto_send_key(app: &AppHandle, session: Option<&PowerSession>) -> config::AutoSendKey {
    let Some(id) = session.map(|s| s.config_id.clone()) else {
        return config::AutoSendKey::None;
    };
    config::load_all(app)
        .unwrap_or_default()
        .into_iter()
        .find(|c| c.id == id)
        .map(|c| c.auto_send_key)
        .unwrap_or_default()
}

/// Renvoie la session courante sans l'alterer (pour l'UI).
pub fn current(app: &AppHandle) -> Option<PowerSession> {
    app.try_state::<PowerSessionState>()?.0.lock().clone()
}

/// Liste des configs activees, dans l'ordre stocke. Mirroir VoiceInk
/// `enabledConfigurations = configurations.filter { $0.isEnabled }`
/// (PowerModeConfig.swift L300). Sert a la selection manuelle par index via
/// les raccourcis Alt+chiffre et a compter les profils adressables.
pub fn enabled_configs(app: &AppHandle) -> Vec<PowerModeConfig> {
    config::load_all(app)
        .unwrap_or_default()
        .into_iter()
        .filter(|c| c.is_enabled)
        .collect()
}

/// Selection manuelle d'un profil par id (menu tray "Power Mode: X").
/// Pendant un enregistrement : comme `select_by_slot` (baseline conservee).
/// Hors enregistrement : applique le profil et le marque actif, sans
/// session (VoiceInk MenuBarView -> ModeManager.setActiveConfiguration).
pub fn select_by_id(app: &AppHandle, id: &str) -> Option<PowerSession> {
    let configs = enabled_configs(app);
    let cfg = configs.iter().find(|c| c.id == id)?;
    if current(app).is_some() {
        return select_config(app, cfg);
    }
    if let Err(e) = apply(app, cfg) {
        warn!("power_mode select_by_id apply: {e}");
        return None;
    }
    let _ = config::set_active_id(app, Some(&cfg.id));
    info!(config = %cfg.name, "Power Mode selectionne depuis le tray");
    None
}

/// Identifiant du profil effectif pour l'affichage : session en cours,
/// sinon dernier profil marque actif s'il est toujours active.
pub fn effective_config_id(app: &AppHandle) -> Option<String> {
    if let Some(s) = current(app) {
        return Some(s.config_id);
    }
    let id = config::active_id(app)?;
    enabled_configs(app)
        .into_iter()
        .find(|c| c.id == id)
        .map(|c| c.id)
}

/// Parlato : raccourci Alt+chiffre, `slot` = index renvoye par le hook
/// (0 = Alt+1 ... 9 = Alt+0). Cible le mode active qui porte ce raccourci,
/// et non plus le Nieme mode active.
pub fn select_by_slot(app: &AppHandle, slot: usize) -> Option<PowerSession> {
    let cfg = config_for_slot(enabled_configs(app), slot)?;
    select_config(app, &cfg)
}

fn config_for_slot(configs: Vec<PowerModeConfig>, slot: usize) -> Option<PowerModeConfig> {
    configs
        .into_iter()
        .find(|c| c.shortcut_slot.map(usize::from) == Some(slot))
}

/// Nombre de touches Alt+chiffre a capturer pendant l'enregistrement : le
/// plus haut raccourci attribue + 1 (0 = aucune, les touches passent a
/// l'application).
pub fn shortcut_capture_count(app: &AppHandle) -> usize {
    capture_count(&enabled_configs(app))
}

fn capture_count(configs: &[PowerModeConfig]) -> usize {
    configs
        .iter()
        .filter_map(|c| c.shortcut_slot)
        .map(|s| usize::from(s) + 1)
        .max()
        .unwrap_or(0)
}

/// Selection manuelle d'un profil (raccourci Alt+chiffre ou tray).
/// Contrairement a begin_session (matching auto fenetre/URL), on applique le
/// profil demande explicitement.
///
/// Si une session Power Mode est deja active, on CONSERVE sa baseline
/// d'origine : le restore de fin de dictee doit ramener aux reglages d'avant
/// l'enregistrement, pas a ceux du profil precedemment applique. Sinon on
/// snapshot l'etat courant (cas ou aucun profil n'avait matche au start).
///
/// Reference VoiceInk : MiniRecorderShortcutManager.setupPowerModeHandler ->
/// setActiveConfiguration + PowerModeSessionManager.beginSession(with:).
fn select_config(app: &AppHandle, cfg: &PowerModeConfig) -> Option<PowerSession> {
    let state = app.try_state::<PowerSessionState>()?;

    // Bind avant le match pour relacher le lock immediatement (ne pas le tenir
    // pendant snapshot/apply).
    let existing = state.0.lock().clone();
    let baseline = baseline_for_new_session(existing.as_ref(), || snapshot(app));
    if existing.is_none() {
        persist_baseline(app, &baseline);
    }
    // Parlato : la session en cours garde son numero (la dictee le porte).
    let owner = RECORDING_OWNER.load(std::sync::atomic::Ordering::SeqCst);
    let id = id_for_manual_select(owner, existing.as_ref().map(|s| s.id), next_session_id);
    if owner != 0 {
        RECORDING_OWNER.store(id, std::sync::atomic::Ordering::SeqCst);
    }

    if let Err(e) = apply(app, cfg) {
        warn!("power_mode select apply: {e}");
        return None;
    }

    let session = PowerSession {
        id,
        config_id: cfg.id.clone(),
        config_name: cfg.name.clone(),
        emoji: cfg.emoji.clone(),
        baseline,
    };
    *state.0.lock() = Some(session.clone());
    let _ = config::set_active_id(app, Some(&session.config_id));
    info!(
        config = %session.config_name,
        "Power Mode selectionne via raccourci"
    );
    Some(session)
}

#[cfg(test)]
mod pending_baseline_tests {
    use super::*;
    use serde_json::json;

    fn baseline() -> Baseline {
        Baseline {
            enhancement_enabled: true,
            language: Some("es".into()),
            whisper_model_id: Some("base".into()),
            ..Default::default()
        }
    }

    #[test]
    fn a_baseline_survives_the_store_round_trip() {
        let v = serde_json::to_value(baseline()).unwrap();
        let back = baseline_from_value(Some(&v)).unwrap();
        assert!(back.enhancement_enabled);
        assert_eq!(back.language.as_deref(), Some("es"));
        assert_eq!(back.whisper_model_id.as_deref(), Some("base"));
    }

    #[test]
    fn nothing_pending_means_nothing_to_do() {
        assert_eq!(pending_action(None, true), PendingAction::Nothing);
        assert_eq!(pending_action(None, false), PendingAction::Nothing);
    }

    #[test]
    fn a_pending_baseline_is_restored_when_auto_restore_is_on() {
        let v = serde_json::to_value(baseline()).unwrap();
        match pending_action(Some(&v), true) {
            PendingAction::Restore(b) => assert_eq!(b.language.as_deref(), Some("es")),
            other => panic!("expected Restore, got {other:?}"),
        }
    }

    #[test]
    fn a_pending_baseline_is_only_cleared_when_auto_restore_is_off() {
        let v = serde_json::to_value(baseline()).unwrap();
        assert_eq!(pending_action(Some(&v), false), PendingAction::ClearOnly);
    }

    #[test]
    fn an_unreadable_value_is_cleared_not_restored() {
        let junk = json!("not a baseline");
        assert_eq!(pending_action(Some(&junk), true), PendingAction::ClearOnly);
    }

    #[test]
    fn a_partial_value_from_an_older_build_still_restores() {
        // Baseline has no required fields beyond the two bools; the store
        // may hold fewer keys than today's struct.
        let v = json!({ "enhancement_enabled": false, "use_screen_capture": false });
        assert!(matches!(pending_action(Some(&v), true), PendingAction::Restore(_)));
    }
}

#[cfg(test)]
mod slot_tests {
    use super::*;

    fn cfg(id: &str, slot: Option<u8>) -> PowerModeConfig {
        let mut c = PowerModeConfig::new(id.into(), "*".into());
        c.id = id.into();
        c.shortcut_slot = slot;
        c
    }

    #[test]
    fn no_shortcut_by_default() {
        assert_eq!(
            PowerModeConfig::new("a".into(), "*".into()).shortcut_slot,
            None
        );
        assert_eq!(capture_count(&[cfg("a", None), cfg("b", None)]), 0);
    }

    #[test]
    fn slot_picks_its_owner_not_the_nth_mode() {
        let configs = vec![cfg("a", None), cfg("b", Some(2)), cfg("c", Some(0))];
        assert_eq!(config_for_slot(configs.clone(), 0).unwrap().id, "c");
        assert_eq!(config_for_slot(configs.clone(), 2).unwrap().id, "b");
        assert!(config_for_slot(configs.clone(), 1).is_none());
        assert_eq!(capture_count(&configs), 3);
    }
}

#[cfg(test)]
mod session_id_tests {
    use super::*;

    fn session(id: u64, name: &str, baseline_lang: &str) -> PowerSession {
        PowerSession {
            id,
            config_id: name.into(),
            config_name: name.into(),
            baseline: Baseline {
                language: Some(baseline_lang.into()),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn a_session_ends_exactly_once() {
        let mut slot = Some(session(4, "a", "en"));
        assert!(take_if_current(&mut slot, 4).is_some());
        assert!(take_if_current(&mut slot, 4).is_none());
        // A stale id never ends a newer session.
        let mut slot = Some(session(5, "b", "en"));
        assert!(take_if_current(&mut slot, 4).is_none());
        assert!(slot.is_some());
    }

    #[test]
    fn ids_only_grow() {
        let a = next_session_id();
        let b = next_session_id();
        assert!(a >= 1 && b > a);
    }

    #[test]
    fn a_new_session_keeps_the_original_baseline() {
        let first = session(1, "work", "original");
        let kept = baseline_for_new_session(Some(&first), || panic!("must not re-snapshot"));
        assert_eq!(kept.language.as_deref(), Some("original"));
        let fresh = baseline_for_new_session(None, || Baseline {
            language: Some("from store".into()),
            ..Default::default()
        });
        assert_eq!(fresh.language.as_deref(), Some("from store"));
    }

    #[test]
    fn an_old_dictation_does_not_end_the_newer_session() {
        // Dictation A (id 1) is still processing when dictation B (id 2)
        // begins, replacing the slot but keeping A's baseline.
        let mut slot = Some(session(1, "a", "original"));
        let baseline = baseline_for_new_session(slot.as_ref(), || unreachable!());
        let mut b = session(2, "b", "ignored");
        b.baseline = baseline;
        slot = Some(b);
        // A ends first: nothing is restored, B stays active.
        assert!(take_if_current(&mut slot, 1).is_none());
        assert_eq!(slot.as_ref().map(|s| s.id), Some(2));
        // B ends: it restores the ORIGINAL baseline.
        let ended = take_if_current(&mut slot, 2).unwrap();
        assert_eq!(ended.baseline.language.as_deref(), Some("original"));
        assert!(slot.is_none());
    }

    #[test]
    fn a_recording_without_a_matching_mode_owns_no_session() {
        // Dictation A (id 3) is still processing; recording B matched nothing.
        let slot = Some(session(3, "a", "original"));
        assert!(owned_session(&slot, RECORDING_UNOWNED).is_none());
        assert!(owned_session(&slot, 0).is_none());
        assert_eq!(owned_session(&slot, 3).map(|s| s.id), Some(3));
        assert!(owned_session(&None, 3).is_none());
    }

    #[test]
    fn a_manual_pick_does_not_hijack_a_previous_dictations_session() {
        // Recording without a match: a pick gets a fresh id.
        assert_eq!(id_for_manual_select(RECORDING_UNOWNED, Some(3), || 9), 9);
        // Recording that owns session 3: the pick keeps it.
        assert_eq!(id_for_manual_select(3, Some(3), || 9), 3);
        // Recording owning 3 but the slot holds another: fresh.
        assert_eq!(id_for_manual_select(3, Some(2), || 9), 9);
        // No existing session: fresh.
        assert_eq!(id_for_manual_select(RECORDING_UNOWNED, None, || 9), 9);
        // Not recording: original behaviour.
        assert_eq!(id_for_manual_select(0, Some(3), || 9), 3);
        assert_eq!(id_for_manual_select(0, None, || 9), 9);
    }

    #[test]
    fn ending_with_no_session_is_a_no_op() {
        let mut slot = None;
        assert!(take_if_current(&mut slot, 7).is_none());
    }

    #[test]
    fn logs_only_the_host_of_a_url() {
        assert_eq!(url_host("https://mail.example.com/inbox/123?token=abc#x"), "mail.example.com");
        assert_eq!(url_host("http://user:pw@example.com:8080/a"), "example.com");
        assert_eq!(url_host("example.com/path?q=1"), "example.com");
        assert_eq!(url_host(""), "");
    }
}
