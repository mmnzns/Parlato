// Pipeline post-enregistrement : transcription + paste au curseur.
//
// Reference VoiceInk : VoiceInk/Transcription/Core/TranscriptionPipeline.swift
// Ordre exact cote VoiceInk (L39-197) : transcribe -> filter -> format ->
// word-replace -> prompt-detect -> AI enhance -> save -> paste -> dismiss.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{anyhow, Result};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_store::StoreExt;
use tokio::task;
use tracing::{info, warn};

use crate::db::{transcription as history_repo, word_replacement as word_repo, Database};
use crate::enhancement::prompt_detection;
use crate::enhancement::prompts as prompt_store;
use crate::enhancement::service as llm_service;
use crate::mini_recorder;
use crate::audio::feedback::{self, Cue};
use crate::paste::paste_at_cursor;
use crate::services::api_keys;
use crate::text_processing::{
    filler_words, filter as output_filter, formatter as text_formatter,
    word_replacement as word_replacer,
};
use crate::transcription::{
    cloud::{
        catalog::CLOUD_MODELS,
        provider::TranscribeRequest,
        streaming::{StreamingConfig, StreamingHandle, StreamingRegistry},
        CloudRegistry,
    },
    speech, vad,
};

/// Nom du store Tauri ou est persiste l'ID du modele selectionne.
const STORE_FILE: &str = "parla.settings.json";
const SELECTED_MODEL_KEY: &str = "selected_whisper_model";
const LANGUAGE_KEY: &str = "whisper_language";
const SPEECH_THREADS_KEY: &str = "speech_threads";
/// Parlato : secondes pendant lesquelles les modeles restent charges apres
/// une dictee (0 = liberes tout de suite, comme VoiceInk).
const KEEP_ALIVE_KEY: &str = "model_keep_alive_secs";
pub const DEFAULT_KEEP_ALIVE_SECS: u64 = 300;
const RESTORE_CLIPBOARD_KEY: &str = "restore_clipboard_after_paste";
const APPEND_TRAILING_SPACE_KEY: &str = "append_trailing_space";
const TEXT_FORMATTING_KEY: &str = "text_formatting_enabled";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PipelineState {
    Transcribing,
    Enhancing,
    Pasting,
    Done,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
pub struct PipelineEvent {
    pub state: PipelineState,
    pub message: Option<String>,
    pub text: Option<String>,
    pub duration_ms: Option<u64>,
    /// Parlato : quel travail emet l'evenement. La bulle n'ecoute que
    /// `dictation`, la page "Transcrire un fichier" que `file`, pour qu'une
    /// dictee pendant un long fichier ne melange pas les deux.
    pub job: Job,
}

/// Parlato : origine d'un passage dans le pipeline.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Job {
    /// Dictee au raccourci : colle au curseur.
    Dictation,
    /// "Transcrire un fichier" : presse-papiers uniquement.
    File,
    /// "Reessayer la derniere transcription" : presse-papiers uniquement.
    Retry,
}

/// Parlato : un passage dans le pipeline et sa ligne d'historique. L'id est
/// porte ici plutot que dans un etat global, sinon deux passages
/// simultanes (une dictee pendant un fichier) s'ecrasent l'id.
#[derive(Debug, Clone)]
pub struct Run {
    pub job: Job,
    pub history_id: Option<String>,
    /// Generation de la bulle d'une dictee (0 hors dictee), pour le bouton
    /// Annuler de la bulle.
    pub pill: u64,
}

/// Parlato : generation de la bulle dont la dictee a ete annulee pendant
/// la transcription (bouton Annuler). Son texte n'est ni nettoye ni colle.
static CANCELLED_PILL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Annule la dictee en cours de traitement : rien ne sera colle. La
/// transcription deja faite reste dans l'historique.
pub fn cancel_processing(app: &AppHandle) {
    let pill = mini_recorder::generation();
    CANCELLED_PILL.store(pill, std::sync::atomic::Ordering::SeqCst);
    info!(pill, "dictee annulee pendant le traitement");
    // Un nettoyage local en cours s'arrete tout de suite (le modele reste charge).
    crate::enhancement::local_ai::cancel_generation();
    feedback::play(app, Cue::Cancel);
    mini_recorder::close_if_current(app, pill);
}

impl Run {
    fn cancelled(&self) -> bool {
        self.pill != 0 && CANCELLED_PILL.load(std::sync::atomic::Ordering::SeqCst) == self.pill
    }

    fn delivery(&self) -> Delivery {
        if self.job == Job::Dictation {
            Delivery::Paste
        } else {
            Delivery::CopyOnly
        }
    }
}

fn emit_state(
    app: &AppHandle,
    job: Job,
    state: PipelineState,
    message: Option<String>,
    text: Option<String>,
    duration_ms: Option<u64>,
) {
    let _ = app.emit(
        "pipeline:state",
        PipelineEvent {
            state,
            message,
            text,
            duration_ms,
            job,
        },
    );
}

/// Parlato : ce qui garde les modeles charges. Un enregistrement et un
/// passage dans le pipeline comptent chacun comme une activite : les modeles
/// ne sont liberes qu'une fois la derniere terminee (sinon la fin d'un fichier
/// dechargerait le modele sous une dictee en cours, ou la minuterie le
/// dechargerait pendant un long enregistrement). Chaque debut change
/// l'`epoch` : une liberation programmee ne se fait que si rien n'a commence
/// depuis.
#[derive(Default)]
struct Activity {
    active: std::sync::atomic::AtomicUsize,
    epoch: std::sync::atomic::AtomicU64,
}

impl Activity {
    const fn new() -> Self {
        Self {
            active: std::sync::atomic::AtomicUsize::new(0),
            epoch: std::sync::atomic::AtomicU64::new(0),
        }
    }

    fn begin(&self) {
        use std::sync::atomic::Ordering::SeqCst;
        self.epoch.fetch_add(1, SeqCst);
        self.active.fetch_add(1, SeqCst);
    }

    /// Termine une activite. Vrai si plus rien n'est actif.
    fn end(&self) -> bool {
        use std::sync::atomic::Ordering::SeqCst;
        self.active.fetch_sub(1, SeqCst).saturating_sub(1) == 0
    }

    fn epoch(&self) -> u64 {
        self.epoch.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Une liberation programmee a l'`epoch` donne peut-elle avoir lieu ?
    fn may_unload(&self, epoch: u64) -> bool {
        use std::sync::atomic::Ordering::SeqCst;
        self.active.load(SeqCst) == 0 && self.epoch.load(SeqCst) == epoch
    }
}

static ACTIVITY: Activity = Activity::new();

/// Un enregistrement est-il compte comme activite ? (une seule a la fois)
static RECORDING_COUNTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn begin_run() {
    ACTIVITY.begin();
}

fn end_run(app: &AppHandle) {
    if !ACTIVITY.end() {
        return;
    }
    // Parlato : les modeles restent prets quelques minutes, la dictee
    // suivante evite leur rechargement (plusieurs secondes pour Whisper
    // large ou un modele de nettoyage local).
    let keep = get_model_keep_alive(app);
    if keep == 0 {
        unload_inference_models(app);
        return;
    }
    let epoch = ACTIVITY.epoch();
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(keep));
        if ACTIVITY.may_unload(epoch) {
            info!(keep, "modeles liberes apres inactivite");
            unload_inference_models(&app);
        }
    });
}

/// Un enregistrement vient de demarrer : il compte comme activite jusqu'a la
/// fin de son passage dans le pipeline, ou jusqu'a son annulation.
pub fn recording_started() {
    use std::sync::atomic::Ordering::SeqCst;
    if !RECORDING_COUNTED.swap(true, SeqCst) {
        ACTIVITY.begin();
    }
}

/// L'enregistrement est termine sans passage dans le pipeline (annule, arret
/// en echec, ou arret sans transcription) : l'activite se termine.
pub fn recording_finished(app: &AppHandle) {
    use std::sync::atomic::Ordering::SeqCst;
    if RECORDING_COUNTED.swap(false, SeqCst) {
        end_run(app);
    }
}

pub fn get_model_keep_alive(app: &AppHandle) -> u64 {
    app.store(STORE_FILE)
        .ok()
        .and_then(|s| s.get(KEEP_ALIVE_KEY))
        .and_then(|v| v.as_u64())
        .unwrap_or(DEFAULT_KEEP_ALIVE_SECS)
}

pub fn set_model_keep_alive(app: &AppHandle, secs: u64) -> Result<()> {
    let store = app.store(STORE_FILE)?;
    store.set(KEEP_ALIVE_KEY, serde_json::Value::from(secs));
    store.save()?;
    Ok(())
}

/// Destination du texte final.
/// - `Paste`    : collage au curseur (dictee normale).
/// - `CopyOnly` : presse-papiers uniquement, pas de Ctrl+V ni d'espace
///   final ni de son "stop" (VoiceInk LastTranscriptionService
///   .retryLastTranscription -> ClipboardManager.copyToClipboard).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delivery {
    Paste,
    CopyOnly,
}

/// Lit la duree du WAV pour le stocker dans la ligne history.
fn wav_duration_secs(wav_path: &Path) -> Option<f64> {
    use hound::WavReader;
    let reader = WavReader::open(wav_path).ok()?;
    let spec = reader.spec();
    let samples = reader.duration() as f64;
    Some(samples / spec.sample_rate as f64)
}

pub fn insert_pending_streaming_row(
    app: &AppHandle,
    provider: &str,
    model: &str,
) -> Option<String> {
    let db = app.try_state::<Database>()?;
    let conn = db.0.lock();
    match history_repo::insert_pending(&conn, None, None) {
        Ok(id) => {
            drop(conn);
            // Pre-remplit transcription_model_name pour la ligne streaming
            // (sera ecrasee par mark_transcribed au finalize).
            let conn = db.0.lock();
            let label = format!("{provider} / {model} (streaming)");
            let _ = conn.execute(
                "UPDATE transcriptions SET transcription_model_name = ?1 WHERE id = ?2",
                rusqlite::params![label, id],
            );
            drop(conn);
            let _ = app.emit("history:created", &id);
            Some(id)
        }
        Err(e) => {
            warn!("history insert_pending streaming: {e}");
            None
        }
    }
}

/// Parlato : retire une ligne d'historique restee en attente (dictee
/// streaming annulee).
pub fn discard_pending_row(app: &AppHandle, id: &str) {
    let Some(db) = app.try_state::<Database>() else {
        return;
    };
    if let Err(e) = history_repo::delete(&db.0.lock(), id) {
        warn!("history delete pending: {e}");
    }
    let _ = app.emit("history:updated", id);
}

fn insert_pending_row(app: &AppHandle, wav_path: Option<&Path>) -> Option<String> {
    let db = app.try_state::<Database>()?;
    let audio_name = wav_path
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str())
        .map(String::from);
    let duration_sec = wav_path.and_then(wav_duration_secs);
    let conn = db.0.lock();
    match history_repo::insert_pending(&conn, audio_name.as_deref(), duration_sec) {
        Ok(id) => {
            drop(conn);
            let _ = app.emit("history:created", &id);
            Some(id)
        }
        Err(e) => {
            warn!("history insert_pending: {e}");
            None
        }
    }
}

fn mark_failed(app: &AppHandle, run: &Run, err: &str) {
    let Some(id) = run.history_id.as_deref() else {
        return;
    };
    if let Some(db) = app.try_state::<Database>() {
        if let Err(e) = history_repo::mark_failed(&db.0.lock(), id, err) {
            warn!("history mark_failed: {e}");
        }
    }
    let _ = app.emit("history:updated", &id);
}

pub fn get_selected_model(app: &AppHandle) -> Option<String> {
    let store = app.store(STORE_FILE).ok()?;
    store
        .get(SELECTED_MODEL_KEY)
        .and_then(|v| v.as_str().map(|s| s.to_string()))
}

pub fn set_selected_model(app: &AppHandle, id: Option<&str>) -> Result<()> {
    let store = app
        .store(STORE_FILE)
        .map_err(|e| anyhow!("store open: {e}"))?;
    match id {
        Some(id) => store.set(SELECTED_MODEL_KEY, serde_json::Value::String(id.to_string())),
        None => {
            store.delete(SELECTED_MODEL_KEY);
        }
    }
    store.save().map_err(|e| anyhow!("store save: {e}"))?;
    Ok(())
}

pub fn get_language(app: &AppHandle) -> Option<String> {
    let store = app.store(STORE_FILE).ok()?;
    let raw = store
        .get(LANGUAGE_KEY)
        .and_then(|v| v.as_str().map(|s| s.to_string()))?;
    // "auto" est notre valeur explicite pour auto-detect ; le pipeline whisper
    // (et les providers cloud) attendent None / null dans ce cas.
    if raw == "auto" || raw.is_empty() {
        None
    } else {
        Some(raw)
    }
}

/// Parlato : threads CPU choisis pour Whisper et Parakeet, 0 = automatique.
pub fn get_speech_threads(app: &AppHandle) -> usize {
    app.store(STORE_FILE)
        .ok()
        .and_then(|s| s.get(SPEECH_THREADS_KEY))
        .and_then(|v| v.as_u64())
        .map(|n| n.min(speech::max_threads() as u64) as usize)
        .unwrap_or(0)
}

pub fn set_speech_threads(app: &AppHandle, threads: usize) -> Result<()> {
    let store = app.store(STORE_FILE)?;
    store.set(SPEECH_THREADS_KEY, serde_json::Value::from(threads));
    store.save()?;
    Ok(())
}

pub fn get_restore_clipboard(app: &AppHandle) -> bool {
    let Some(store) = app.store(STORE_FILE).ok() else {
        return true;
    };
    store
        .get(RESTORE_CLIPBOARD_KEY)
        .and_then(|v| v.as_bool())
        .unwrap_or(true)
}

/// Parlato : actif par defaut, le mot tape ensuite ne se colle pas au texte
/// dicte.
pub fn get_append_trailing_space(app: &AppHandle) -> bool {
    let Some(store) = app.store(STORE_FILE).ok() else {
        return true;
    };
    store
        .get(APPEND_TRAILING_SPACE_KEY)
        .and_then(|v| v.as_bool())
        .unwrap_or(true)
}

/// Parlato : actif par defaut, les longues dictees arrivent en paragraphes.
pub fn get_text_formatting_enabled(app: &AppHandle) -> bool {
    let Some(store) = app.store(STORE_FILE).ok() else {
        return true;
    };
    store
        .get(TEXT_FORMATTING_KEY)
        .and_then(|v| v.as_bool())
        .unwrap_or(true)
}

pub(crate) fn is_vad_enabled(app: &AppHandle) -> bool {
    let Some(store) = app.store(STORE_FILE).ok() else {
        return false;
    };
    store
        .get("vad_enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

pub fn supports_streaming(provider_id: &str, model_id: &str) -> bool {
    CLOUD_MODELS
        .iter()
        .any(|m| m.provider_id == provider_id && m.model_id == model_id && m.supports_streaming)
}

/// Determine si la source actuelle doit utiliser le streaming cloud.
/// Renvoie Some((provider, model)) si oui, None sinon.
pub fn active_streaming_target(app: &AppHandle) -> Option<(String, String)> {
    let store = app.store(STORE_FILE).ok()?;
    let kind = store.get("transcription_source_kind")?.as_str()?.to_string();
    if kind != "cloud" {
        return None;
    }
    let provider = store
        .get("selected_cloud_provider")?
        .as_str()?
        .to_string();
    let model = store.get("selected_cloud_model")?.as_str()?.to_string();
    if supports_streaming(&provider, &model) {
        Some((provider, model))
    } else {
        None
    }
}

/// Lance une session streaming cloud via le registry et retourne le handle.
pub async fn start_streaming_session(
    app: AppHandle,
    provider: String,
    model: String,
    language: Option<String>,
) -> Result<StreamingHandle> {
    let registry: Arc<StreamingRegistry> = app
        .state::<crate::commands::streaming::StreamingRegistryState>()
        .0
        .clone();

    let key = api_keys::get_api_key(&provider)?
        .ok_or_else(|| anyhow!("PARLA_ERR:apiKey:{provider}"))?;

    // Injection du dictionnaire enabled comme en batch.
    let mut custom_vocab = Vec::new();
    if let Some(db) = app.try_state::<Database>() {
        if let Ok(rules) = word_repo::list_enabled(&db.0.lock()) {
            for r in rules {
                for v in r.original_text.split(',') {
                    let t = v.trim();
                    if !t.is_empty() {
                        custom_vocab.push(t.to_string());
                    }
                }
            }
        }
    }

    let config = StreamingConfig {
        model,
        language,
        custom_vocabulary: custom_vocab,
    };

    crate::transcription::cloud::streaming::start_streaming(app, registry, provider, key, config)
        .await
}

/// Finalise une session streaming et applique le post-traitement + paste.
/// Parlato : si la session echoue (cle refusee, hors ligne, erreur du
/// fournisseur), l'enregistrement WAV fait en parallele est transcrit en
/// mode fichier par le meme fournisseur, dans la meme ligne d'historique,
/// et la dictee se termine toujours par `finish_dictation`.
pub fn finish_streaming(
    app: AppHandle,
    handle: StreamingHandle,
    history_id: Option<String>,
    wav_path: PathBuf,
) {
    let pill = mini_recorder::generation();
    let released = std::time::Instant::now();
    begin_run();
    recording_finished(&app);
    tauri::async_runtime::spawn(async move {
        let run = Run {
            job: Job::Dictation,
            history_id,
            pill,
        };
        let start = std::time::Instant::now();
        let result = match handle.finalize().await {
            Ok(text) => {
                let duration_ms = start.elapsed().as_millis() as u64;
                info!(chars = text.len(), duration_ms, "Streaming cloud finalise");
                mark_transcribed_in_history(
                    &app,
                    &run,
                    &text,
                    duration_ms,
                    "streaming",
                    get_language(&app).as_deref(),
                );
                finalize_text(&app, &run, text, duration_ms).await
            }
            Err(e) => {
                warn!("Streaming echoue, transcription du fichier enregistre: {e}");
                run_pipeline(app.clone(), &run, wav_path).await
            }
        };
        finish_dictation(&app, &run, result, pill, released).await;
    });
}

/// Declenche le pipeline complet apres la fin d'un enregistrement : transcribe
/// puis paste. Emet des evenements pipeline:state vers le frontend.
pub fn run_after_recording(app: AppHandle, wav_path: PathBuf) {
    let pill = mini_recorder::generation();
    let released = std::time::Instant::now();
    let run = Run {
        job: Job::Dictation,
        history_id: insert_pending_row(&app, Some(&wav_path)),
        pill,
    };
    begin_run();
    // Le passage prend le relais de l'enregistrement (jamais de trou entre
    // les deux).
    recording_finished(&app);
    // tauri::async_runtime::spawn fonctionne depuis n'importe quel thread
    // (le hotkey-dispatch est un std::thread sans runtime Tokio attache).
    tauri::async_runtime::spawn(async move {
        let result = run_pipeline(app.clone(), &run, wav_path).await;
        finish_dictation(&app, &run, result, pill, released).await;
    });
}

/// Fin commune d'une dictee (batch ou streaming), succes ou echec.
/// Parlato : un echec est montre dans la bulle (message traduit) avec le
/// son d'annulation, au lieu de la fermer sans rien dire ; elle reste
/// ouverte un peu plus longtemps pour qu'on ait le temps de le lire.
async fn finish_dictation(
    app: &AppHandle,
    run: &Run,
    result: Result<Outcome>,
    pill: u64,
    released: std::time::Instant,
) {
    // Parlato : une ligne par dictee, du relachement de la touche a la fin
    // (collage compris), pour reperer une lenteur dans parlato.log.
    info!(
        total_ms = released.elapsed().as_millis() as u64,
        outcome = ?result.as_ref().map_err(|_| "failed"),
        "Dictee terminee"
    );
    let failed = matches!(result, Err(_) | Ok(Outcome::NotPasted));
    let result_kind = result.as_ref().ok().copied();
    let empty = matches!(result, Ok(Outcome::Empty));
    if let Err(e) = result {
        warn!("Pipeline echec: {e}");
        mark_failed(app, run, &e.to_string());
        emit_state(
            app,
            Job::Dictation,
            PipelineState::Failed,
            Some(e.to_string()),
            None,
            None,
        );
        feedback::play(app, Cue::Cancel);
    }
    // Libere les modeles d'inference une fois le dernier passage termine.
    // Reference VoiceInk VoiceInkEngine.cleanupResources.
    end_run(app);
    // Fin de la session Power Mode : restauration de la baseline si
    // auto_restore est active (reference VoiceInk
    // PowerModeSessionManager.endSession). Faite tout de suite, avant
    // l'attente, pour ne pas toucher la session d'une dictee suivante.
    crate::power_mode::session::end_session(app);
    let _ = app.emit("power_mode:active", serde_json::Value::Null);
    crate::tray::refresh(app);
    // VoiceInk ferme la bulle juste apres le paste ; 200 ms de marge pour
    // les derniers evenements, plus le temps de lire un echec.
    let delay = if matches!(result_kind, Some(Outcome::Cancelled)) {
        0
    } else if failed {
        FAILED_PILL_MS
    } else if empty {
        EMPTY_PILL_MS
    } else {
        200
    };
    tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
    mini_recorder::close_if_current(app, pill);
    crate::notice::flush_after_dictation(app);
}

/// Duree d'affichage d'un echec dans la bulle.
const FAILED_PILL_MS: u64 = 3500;
/// Duree d'affichage de "Rien entendu" (dictee sans parole).
const EMPTY_PILL_MS: u64 = 1500;

/// Parlato : issue d'un passage reussi. `Empty` = aucune parole, rien n'a
/// ete colle ; la bulle le dit au lieu de disparaitre sans rien.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Delivered,
    Empty,
    /// Texte transcrit mais pas colle : il est dans le presse-papiers.
    NotPasted,
    /// Annule depuis la bulle pendant le traitement : rien n'est colle.
    Cancelled,
}

/// Retranscrit un fichier audio existant (dernier enregistrement) avec la
/// source courante, enhancement compris, et copie le resultat dans le
/// presse-papiers. Cree une nouvelle entree d'historique, comme VoiceInk
/// AudioTranscriptionService.retranscribeAudio. Pas de mini-recorder, pas
/// de session Power Mode : c'est une action utilitaire hors dictee.
pub fn run_retry(app: AppHandle, wav_path: PathBuf) {
    spawn_copy_only(app, wav_path, Job::Retry);
}

/// Parlato : transcrit un fichier importe ("Transcribe a file"), deja
/// converti en WAV 16 kHz par transcription::audio_file. Meme chemin que
/// run_retry (source courante, enhancement, historique, presse-papiers)
/// mais sans notice tray : la page suit l'avancement via pipeline:state.
pub fn run_file(app: AppHandle, wav_path: PathBuf) {
    spawn_copy_only(app, wav_path, Job::File);
}

fn spawn_copy_only(app: AppHandle, wav_path: PathBuf, job: Job) {
    let run = Run {
        job,
        history_id: insert_pending_row(&app, Some(&wav_path)),
        pill: 0,
    };
    begin_run();
    tauri::async_runtime::spawn(async move {
        let result = run_pipeline(app.clone(), &run, wav_path).await;
        match result {
            Ok(_) => {
                if job == Job::Retry {
                    crate::notice::send(&app, crate::notice::Notice::RetryDone);
                }
            }
            Err(e) => {
                warn!("Retry pipeline echec: {e}");
                mark_failed(&app, &run, &e.to_string());
                if job == Job::Retry {
                    crate::notice::send(&app, crate::notice::Notice::Failed(e.to_string()));
                }
                emit_state(
                    &app,
                    job,
                    PipelineState::Failed,
                    Some(e.to_string()),
                    None,
                    None,
                );
            }
        }
        end_run(&app);
    });
}

/// Decharge tous les modeles d'inference resident en memoire (whisper,
/// parakeet, VAD, IA locale). Appele en fin de pipeline pour que la RAM
/// idle de Parla redescende a ~200 Mo (Tauri + WebView2 + SQLite ouvert)
/// au lieu de ~800 Mo ou plus selon les modeles utilises. Chaque engine
/// gere son propre Mutex, donc cette fonction est sans erreur et sans
/// await : on tombe juste le Arc<Option<T>> a None.
pub(crate) fn unload_inference_models(app: &AppHandle) {
    if let Some(st) = app.try_state::<crate::commands::transcription::WhisperEngineState>() {
        st.0.unload();
    }
    if let Some(st) = app.try_state::<crate::commands::vad::VadEngineState>() {
        st.0.unload();
    }
    if let Some(st) = app.try_state::<crate::transcription::parakeet::ParakeetEngineState>() {
        st.0.unload();
    }
    // Parlato : l'IA locale suit le meme delai ("Garder les modeles prets").
    // Sur un fil : la liberation attend une generation en cours, et on est
    // peut-etre dans une tache async.
    if let Some(ai) = crate::enhancement::local_ai::local_ai() {
        ai.unload_in_background();
    }
}

#[derive(Debug, Clone)]
enum Source {
    Local { model_id: String },
    Cloud { provider: String, model: String },
    Parakeet { model_id: String },
}

fn resolve_source(app: &AppHandle) -> Source {
    let store = match app.store(STORE_FILE) {
        Ok(s) => s,
        Err(_) => {
            return Source::Local {
                model_id: get_selected_model(app).unwrap_or_default(),
            };
        }
    };
    let kind = store
        .get("transcription_source_kind")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "local".into());
    match kind.as_str() {
        "cloud" => {
            let provider = store
                .get("selected_cloud_provider")
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_default();
            let model = store
                .get("selected_cloud_model")
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_default();
            Source::Cloud { provider, model }
        }
        "parakeet" => Source::Parakeet {
            model_id: store
                .get("selected_parakeet_model")
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_default(),
        },
        _ => Source::Local {
            model_id: get_selected_model(app).unwrap_or_default(),
        },
    }
}

async fn transcribe_cloud(
    app: &AppHandle,
    wav_path: &Path,
    provider_id: &str,
    model: &str,
    language: Option<String>,
) -> Result<(String, u64)> {
    let registry: std::sync::Arc<CloudRegistry> = app
        .state::<crate::commands::cloud::CloudRegistryState>()
        .0
        .clone();
    let provider = registry
        .find(provider_id)
        .ok_or_else(|| anyhow!("provider cloud inconnu: {provider_id}"))?;
    let key = api_keys::get_api_key(provider_id)
        .map_err(|e| anyhow!("keyring: {e}"))?
        .ok_or_else(|| anyhow!("PARLA_ERR:apiKey:{provider_id}"))?;

    let mut req = TranscribeRequest::new(model);
    req.language = language;

    // Injection du dictionnaire enabled (VoiceInk Soniox/Deepgram/Speechmatics).
    if let Some(db) = app.try_state::<Database>() {
        if let Ok(rules) = word_repo::list_enabled(&db.0.lock()) {
            for r in rules {
                for v in r.original_text.split(',') {
                    let t = v.trim();
                    if !t.is_empty() {
                        req.custom_vocabulary.push(t.to_string());
                    }
                }
            }
        }
    }

    let start = std::time::Instant::now();
    let text = provider.transcribe(wav_path, &key, &req).await?;
    Ok((text, start.elapsed().as_millis() as u64))
}

async fn run_pipeline(app: AppHandle, run: &Run, wav_path: PathBuf) -> Result<Outcome> {
    let source = resolve_source(&app);
    let language = get_language(&app);

    let source_label = match &source {
        Source::Local { model_id } => format!("local / {model_id}"),
        Source::Cloud { provider, model } => format!("{provider} / {model}"),
        Source::Parakeet { model_id } => format!("parakeet / {model_id}"),
    };
    emit_state(
        &app,
        run.job,
        PipelineState::Transcribing,
        Some(format!("Transcription {source_label}")),
        None,
        None,
    );

    // Branche cloud : pas de VAD, pas de moteur local. Upload direct au provider.
    if let Source::Cloud { provider, model } = &source {
        if provider.is_empty() || model.is_empty() {
            anyhow::bail!("PARLA_ERR:cloudUnconfigured");
        }
        let (text, duration_ms) = transcribe_cloud(
            &app,
            &wav_path,
            provider,
            model,
            language.clone(),
        )
        .await?;
        info!(chars = text.len(), duration_ms, provider, model, "Transcription cloud terminee");
        mark_transcribed_in_history(
            &app,
            run,
            &text,
            duration_ms,
            &format!("{provider} / {model}"),
            language.as_deref(),
        );
        return finalize_text(&app, run, text, duration_ms).await;
    }

    // Branche Parakeet (local via voice-engine-stt : TDT, Unified, Nemotron).
    if let Source::Parakeet { model_id } = &source {
        if model_id.is_empty() {
            anyhow::bail!("PARLA_ERR:noParakeet");
        }
        let store = app.state::<crate::models::ModelStoreState>().0.clone();
        let model_source = speech::Source::for_id(&store, model_id)
            .ok_or_else(|| anyhow!("PARLA_ERR:parakeetIncomplete:{model_id}"))?;
        let engine_state = app
            .state::<crate::transcription::parakeet::ParakeetEngineState>()
            .0
            .clone();
        // Verrou du dossier partage tant que le modele est charge.
        let handle = store.hold(model_id);
        let wav_path_clone = wav_path.clone();
        let language_clone = language.clone();
        let vad = vad_for(&app);
        let threads = speech::threads_option(get_speech_threads(&app));
        let start = std::time::Instant::now();
        let engine_for_notice = engine_state.clone();
        let text = task::spawn_blocking(move || -> Result<String> {
            engine_state.ensure_loaded(&model_source, threads, handle)?;
            let used_vad = vad.is_some();
            let mut samples = speech_samples(vad, &wav_path_clone)?;
            if samples.is_empty() {
                return Ok(String::new());
            }
            // VoiceInk 2.21 : apres la VAD, au moins 1 s d'audio pour le modele.
            if used_vad && samples.len() < 16_000 {
                samples.resize(16_000, 0.0);
            }
            engine_state.transcribe_samples(&samples, language_clone.as_deref())
        })
        .await
        .map_err(|e| anyhow!("task join: {e}"))??;
        let duration_ms = start.elapsed().as_millis() as u64;
        info!(
            chars = text.len(),
            duration_ms,
            model = model_id,
            "Transcription Parakeet terminee"
        );
        // Parlato : repli carte graphique -> processeur, a dire une fois dans la
        // bulle, une fois la dictee finie (la bulle est occupee d'ici la).
        if engine_for_notice.take_gpu_notice() && run.job == Job::Dictation {
            crate::notice::send_after_dictation(crate::notice::Notice::GpuFallback);
        }
        mark_transcribed_in_history(
            &app,
            run,
            &text,
            duration_ms,
            &format!("parakeet / {model_id}"),
            language.as_deref(),
        );
        return finalize_text(&app, run, text, duration_ms).await;
    }

    // Branche locale Whisper (comme avant).
    let Source::Local { model_id } = source.clone() else {
        unreachable!();
    };
    if model_id.is_empty() {
        anyhow::bail!("PARLA_ERR:noWhisper");
    }

    let engine = app
        .state::<crate::commands::transcription::WhisperEngineState>()
        .0
        .clone();
    let store = app.state::<crate::models::ModelStoreState>().0.clone();

    let model_source = speech::Source::for_id(&store, &model_id)
        .ok_or_else(|| anyhow!("PARLA_ERR:modelNotDownloaded:{model_id}"))?;
    // Verrou du dossier partage tant que le modele est charge (rien pour un
    // modele importe, qui n'est pas dans le dossier partage).
    let handle = store.hold(&model_id);

    // VAD optionnelle (matching VoiceInk IsVADEnabled), cf `speech_samples`.
    let vad = vad_for(&app);
    let threads = speech::threads_option(get_speech_threads(&app));

    let wav_path_clone = wav_path.clone();
    let language_clone = language.clone();
    let engine_for_notice = engine.clone();
    let start = std::time::Instant::now();
    let text = task::spawn_blocking(move || -> Result<String> {
        engine.ensure_loaded(&model_source, threads, handle)?;
        let samples = speech_samples(vad, &wav_path_clone)?;
        if samples.is_empty() {
            return Ok(String::new());
        }
        // Parlato : les mots du dictionnaire ne sont pas envoyes a Whisper
        // local (parite 0.14, plan decision 7).
        engine.transcribe_samples(&samples, language_clone.as_deref(), &[])
    })
    .await
    .map_err(|e| anyhow!("task join: {e}"))??;

    let duration_ms = start.elapsed().as_millis() as u64;
    info!(chars = text.len(), duration_ms, "Transcription locale terminee");
    // Parlato : repli Metal -> processeur, a dire une fois dans la bulle.
    if engine_for_notice.take_gpu_notice() && run.job == Job::Dictation {
        crate::notice::send_after_dictation(crate::notice::Notice::GpuFallback);
    }
    mark_transcribed_in_history(
        &app,
        run,
        &text,
        duration_ms,
        &format!("local / {model_id}"),
        language.as_deref(),
    );
    finalize_text(&app, run, text, duration_ms).await
}

/// VAD Silero si elle est activee et que son modele est telecharge : le moteur,
/// le dossier du modele "vad" et le verrou du dossier partage (le detecteur le
/// garde tant qu'il est charge).
/// Parlato : partagee par Whisper et Parakeet (VoiceInk 2.21 6985c4b
/// l'applique aussi a FluidAudio).
fn vad_for(app: &AppHandle) -> Option<vad::VadUse> {
    if !is_vad_enabled(app) {
        return None;
    }
    let store = app.state::<crate::models::ModelStoreState>().0.clone();
    let voice_engine_models::Status::Installed { dir, .. } =
        store.shared().status(crate::commands::vad::VAD_ID)
    else {
        return None;
    };
    let engine = app.state::<crate::commands::vad::VadEngineState>().0.clone();
    Some((engine, dir, store.hold(crate::commands::vad::VAD_ID)))
}

/// Lit le WAV. Avec la VAD, ne garde que les plages de parole mises bout a
/// bout (vide si aucune parole).
fn speech_samples(vad: Option<vad::VadUse>, wav: &Path) -> Result<Vec<f32>> {
    let Some((engine, model_dir, handle)) = vad else {
        return speech::read_wav_as_f32(wav);
    };
    let (samples, ranges) = vad::run_vad_on_wav(&engine, &model_dir, handle, wav)?;
    let total: usize = ranges.iter().map(|(s, e)| e - s).sum();
    let mut speech: Vec<f32> = Vec::with_capacity(total);
    for (s, e) in &ranges {
        speech.extend_from_slice(&samples[*s..*e]);
    }
    Ok(speech)
}

fn mark_transcribed_in_history(
    app: &AppHandle,
    run: &Run,
    text: &str,
    duration_ms: u64,
    model_label: &str,
    language: Option<&str>,
) {
    let Some(id) = run.history_id.as_deref() else {
        return;
    };
    let Some(db) = app.try_state::<Database>() else {
        return;
    };
    let session = crate::power_mode::session::current(app);
    let fields = history_repo::TranscribeFields {
        text,
        transcription_duration_sec: duration_ms as f64 / 1000.0,
        transcription_model_name: model_label,
        language,
        power_mode_name: session.as_ref().map(|s| s.config_name.as_str()),
        power_mode_emoji: session.as_ref().map(|s| s.emoji.as_str()),
    };
    if let Err(e) = history_repo::mark_transcribed(&db.0.lock(), id, &fields) {
        warn!("history mark_transcribed: {e}");
    }
    let _ = app.emit("history:updated", &id);
}

fn mark_enhanced_in_history(app: &AppHandle, run: &Run, enhanced: &str, enhance_ms: u64) {
    let Some(id) = run.history_id.as_deref() else {
        return;
    };
    let Some(db) = app.try_state::<Database>() else {
        return;
    };
    // Ces 2 champs sont renseignes via le selection LLM actuel au moment
    // de l'enhance (l'info Prompt active est lue separement).
    let selection = llm_service::get_selection(app);
    let model_label = selection
        .map(|s| format!("{} / {}", s.provider_id, s.model))
        .unwrap_or_default();
    let prompt_name = match crate::enhancement::prompts::get_active_prompt(app) {
        Ok(p) => Some(p.title),
        Err(_) => None,
    };
    let fields = history_repo::EnhanceFields {
        enhanced_text: enhanced,
        enhancement_duration_sec: enhance_ms as f64 / 1000.0,
        ai_enhancement_model_name: &model_label,
        prompt_name: prompt_name.as_deref(),
        ai_request_system_message: None,
        ai_request_user_message: None,
    };
    if let Err(e) = history_repo::mark_enhanced(&db.0.lock(), id, &fields) {
        warn!("history mark_enhanced: {e}");
    }
    let _ = app.emit("history:updated", &id);
}

/// Post-traitement + paste commun aux branches locale et cloud.
/// Ordre VoiceInk TranscriptionPipeline.swift L67-100 :
///   filter -> trim -> formatter (si active) -> word_replacement -> paste.
async fn finalize_text(app: &AppHandle, run: &Run, text: String, duration_ms: u64) -> Result<Outcome> {
    let delivery = run.delivery();
    if run.cancelled() {
        return Ok(Outcome::Cancelled);
    }
    let fillers = if filler_words::is_enabled(app) {
        filler_words::current_list(app)
    } else {
        Vec::new()
    };
    let mut text = output_filter::filter(&text, &fillers);

    if get_text_formatting_enabled(app) {
        text = text_formatter::format(&text);
    }

    if let Some(db) = app.try_state::<Database>() {
        match word_repo::list_enabled(&db.0.lock()) {
            Ok(rules) if !rules.is_empty() => {
                text = word_replacer::apply(&text, &rules);
            }
            Ok(_) => {}
            Err(e) => warn!("Lecture word_replacements: {e}"),
        }
    }

    if text.trim().is_empty() {
        warn!("Transcription vide, on saute le paste");
        emit_state(
            app,
            run.job,
            PipelineState::Done,
            Some("Aucun texte detecte".into()),
            Some(String::new()),
            Some(duration_ms),
        );
        return Ok(Outcome::Empty);
    }

    // prompt-detect : cherche un trigger_word dans le texte qui activerait
    // un prompt specifique (VoiceInk PromptDetectionService). Si match, on
    // force l'enhancement ON avec ce prompt et on strip le trigger du texte.
    let detection = match prompt_store::load_cached(app) {
        Ok(all) => prompt_detection::detect_and_strip(&all, &text),
        Err(e) => {
            warn!("prompt-detect: load prompts echec: {e}");
            None
        }
    };

    let prompt_override = if let Some(ref d) = detection {
        info!(
            trigger = %d.trigger_word,
            prompt_id = %d.prompt_id,
            "trigger_word detecte, enhancement force avec prompt specifique"
        );
        text = d.processed_text.clone();
        prompt_store::load_cached(app)
            .ok()
            .and_then(|all| all.into_iter().find(|p| p.id == d.prompt_id))
    } else {
        None
    };

    // Enhancement LLM optionnel (VoiceInk TranscriptionPipeline.swift L119-150).
    // Si active et configure, remplace le texte avant paste. En cas d'echec,
    // on preserve le texte brut (fallback VoiceInk).
    let should_enhance = if prompt_override.is_some() {
        // Trigger detecte : VoiceInk force l'enhancement ON meme si le
        // toggle global est off. On exige juste un provider configure.
        llm_service::has_provider_configured(app)
    } else {
        llm_service::is_configured(app) && !llm_service::should_skip_short(app, &text)
    };

    if should_enhance {
        emit_state(
            app,
            run.job,
            PipelineState::Enhancing,
            Some("Enhancement LLM".into()),
            Some(text.clone()),
            Some(duration_ms),
        );
        match llm_service::enhance_with_override(app.clone(), text.clone(), prompt_override).await {
            Ok(Some((enhanced, enhance_ms))) => {
                if !enhanced.trim().is_empty() {
                    mark_enhanced_in_history(app, run, &enhanced, enhance_ms);
                    text = enhanced;
                }
            }
            Ok(None) => {}
            Err(e) => warn!("Enhancement LLM echec, fallback texte brut: {e}"),
        }
    }
    // Parlato : repli carte graphique -> processeur de l'IA locale, a dire une
    // fois dans la bulle, une fois la dictee finie.
    if let Some(ai) = crate::enhancement::local_ai::local_ai() {
        if ai.take_gpu_notice() && run.job == Job::Dictation {
            crate::notice::send_after_dictation(crate::notice::Notice::GpuFallback);
        }
    }

    // Annule pendant le nettoyage IA.
    if run.cancelled() {
        return Ok(Outcome::Cancelled);
    }

    emit_state(
        app,
        run.job,
        PipelineState::Pasting,
        None,
        Some(text.clone()),
        Some(duration_ms),
    );

    if delivery == Delivery::CopyOnly {
        // Retry : presse-papiers uniquement, texte brut sans espace final.
        let copy_text = text.clone();
        task::spawn_blocking(move || crate::paste::copy_to_clipboard(&copy_text))
            .await
            .map_err(|e| anyhow!("task join: {e}"))??;
    } else {
        let append_space = get_append_trailing_space(app);
        let restore = get_restore_clipboard(app);
        let final_text = if append_space {
            format!("{text} ")
        } else {
            text.clone()
        };

        let final_text_clone = final_text.clone();
        let pasted =
            task::spawn_blocking(move || paste_at_cursor(&final_text_clone, restore, None))
                .await
                .map_err(|e| anyhow!("task join: {e}"))?;
        // Parlato : la transcription a reussi, seul le collage a echoue
        // (fenetre administrateur, Accessibilite absente). L'historique garde
        // le texte, qui reste aussi dans le presse-papiers ; la bulle le dit.
        if let Err(e) = pasted {
            warn!("paste: {e}");
            emit_state(
                app,
                run.job,
                PipelineState::Failed,
                Some("PARLA_ERR:pasteFailed".into()),
                Some(text),
                Some(duration_ms),
            );
            feedback::play(app, Cue::Cancel);
            return Ok(Outcome::NotPasted);
        }

        // Parlato : touche d'envoi du Power mode actif (Entree...).
        let send = crate::power_mode::session::auto_send_key(app);
        if send != crate::paste::AutoSendKey::None {
            match task::spawn_blocking(move || crate::paste::send_after_paste(send)).await {
                Ok(Err(e)) => warn!("auto-send: {e}"),
                Err(e) => warn!("auto-send join: {e}"),
                Ok(Ok(())) => {}
            }
        }

        // Stop cue, played once the text is actually inserted at the cursor
        // (VoiceInk SoundManager.playStopSound at TranscriptionPipeline.swift:209).
        // Only reached for non-empty transcriptions (empty text returns earlier),
        // so the cue doubles as a "text inserted" confirmation.
        feedback::play(app, Cue::Stop);
    }

    emit_state(
        app,
        run.job,
        PipelineState::Done,
        None,
        Some(text),
        Some(duration_ms),
    );
    Ok(Outcome::Delivered)
}

#[cfg(test)]
mod tests {
    // -- Keep-alive : l'activite ---------------------------------------------

    #[test]
    fn a_scheduled_unload_waits_for_every_activity() {
        let a = Activity::new();
        // A dictation ran and ended: an unload is scheduled at that epoch.
        a.begin();
        assert!(a.end());
        let scheduled = a.epoch();
        assert!(a.may_unload(scheduled));
        // A new activity cancels it, even after it ended.
        a.begin();
        assert!(!a.may_unload(scheduled));
        assert!(a.end());
        assert!(!a.may_unload(scheduled));
        assert!(a.may_unload(a.epoch()));
    }

    #[test]
    fn a_long_recording_holds_the_models_until_its_pipeline_ends() {
        let a = Activity::new();
        // A previous dictation scheduled an unload.
        a.begin();
        assert!(a.end());
        let scheduled = a.epoch();
        // The recording starts: from then on the old timer cannot unload.
        a.begin();
        assert!(!a.may_unload(scheduled));
        // Hours later, still recording: a timer scheduled now would not fire either.
        assert!(!a.may_unload(a.epoch()));
        // Stop: the pipeline begins before the recording lets go, so there is
        // never an idle moment between them.
        a.begin();
        assert!(!a.end(), "the pipeline still holds the models");
        assert!(!a.may_unload(a.epoch()));
        // The pipeline ends: now idle, and only now an unload can be scheduled.
        assert!(a.end());
        assert!(a.may_unload(a.epoch()));
    }

    #[test]
    fn a_cancelled_recording_leaves_the_models_to_the_timer() {
        let a = Activity::new();
        a.begin(); // recording
        assert!(a.end()); // cancelled: idle again, the caller schedules the unload
        assert!(a.may_unload(a.epoch()));
    }

    use super::*;

    /// Smoke test of the shared VAD step in front of Parakeet (counts and
    /// timings only, never the text):
    /// `PARLATO_VAD_MODEL=<silero .bin> PARLATO_PARAKEET_DIR=<dir>
    /// PARLATO_WAV=<16 kHz wav> cargo test --release --lib vad_parakeet_smoke -- --ignored --nocapture`.
    /// `PARLATO_PARAKEET_ID` picks another TDT catalog id than parakeet-ultra.
    #[test]
    #[ignore]
    fn vad_parakeet_smoke() {
        let vad_model = PathBuf::from(std::env::var("PARLATO_VAD_MODEL").expect("PARLATO_VAD_MODEL"));
        let dir = PathBuf::from(std::env::var("PARLATO_PARAKEET_DIR").expect("PARLATO_PARAKEET_DIR"));
        let wav = PathBuf::from(std::env::var("PARLATO_WAV").expect("PARLATO_WAV"));
        let id = std::env::var("PARLATO_PARAKEET_ID").unwrap_or_else(|_| "parakeet-ultra".into());
        let all = speech::read_wav_as_f32(&wav).unwrap();
        let (vad_dir, _keep) = vad::tests::vad_dir_for(&vad_model);
        let vad = Some((Arc::new(vad::VadEngine::default()), vad_dir, None));
        let t0 = std::time::Instant::now();
        let speech = speech_samples(vad, &wav).unwrap();
        println!("vad: {} ms", t0.elapsed().as_millis());
        let engine = crate::transcription::parakeet::ParakeetEngine::default();
        let source = speech::Source::Catalog {
            model: voice_engine_models::catalog::find(&id).expect("catalog id"),
            dir,
        };
        engine.ensure_loaded_with_gpu(&source, None, false, None).unwrap();
        let t1 = std::time::Instant::now();
        let text = engine.transcribe_samples(&speech, None).unwrap();
        println!(
            "--- {:.1}s audio, {:.1}s kept after VAD, {} words in {} ms ---",
            all.len() as f32 / 16000.0,
            speech.len() as f32 / 16000.0,
            text.split_whitespace().count(),
            t1.elapsed().as_millis()
        );
        assert!(speech.len() < all.len());
        assert!(!text.trim().is_empty());
    }
}
