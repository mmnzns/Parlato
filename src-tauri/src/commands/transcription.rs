// Commandes Tauri pour la transcription Whisper locale.

use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};
use tracing::info;

use crate::transcription::{model_manager::ModelManager, whisper::{WhisperEngine, WhisperParams}};

pub struct WhisperEngineState(pub Arc<WhisperEngine>);

impl Default for WhisperEngineState {
    fn default() -> Self {
        Self(Arc::new(WhisperEngine::new()))
    }
}

#[derive(Debug, Deserialize)]
pub struct TranscribeRequest {
    pub wav_path: String,
    pub model_id: String,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub initial_prompt: Option<String>,
    #[serde(default)]
    pub n_threads: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct TranscribeResponse {
    pub text: String,
    pub model_id: String,
    pub duration_ms: u64,
}

#[tauri::command]
pub async fn transcribe_wav(
    app: AppHandle,
    engine_state: State<'_, WhisperEngineState>,
    models_state: State<'_, super::models::ModelManagerState>,
    req: TranscribeRequest,
) -> Result<TranscribeResponse, String> {
    let engine = engine_state.0.clone();
    let models: Arc<ModelManager> = models_state.0.clone();
    let _ = app;

    let model_path: PathBuf = models
        .path_if_present(&req.model_id)
        .ok_or_else(|| format!("modele non telecharge: {}", req.model_id))?;

    let wav_path = PathBuf::from(&req.wav_path);
    if !wav_path.exists() {
        return Err(format!("fichier WAV introuvable: {}", req.wav_path));
    }

    let params = WhisperParams {
        language: req.language,
        initial_prompt: req.initial_prompt,
        n_threads: req.n_threads.unwrap_or(0),
        ..Default::default()
    };

    let model_id = req.model_id.clone();
    let start = std::time::Instant::now();
    let text = tokio::task::spawn_blocking(move || -> Result<String, String> {
        engine
            .load(&model_path)
            .map_err(|e| e.to_string())?;
        engine
            .transcribe_wav(&wav_path, &params)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("tache transcription panic: {e}"))??;

    let duration_ms = start.elapsed().as_millis() as u64;
    info!(model_id, chars = text.len(), duration_ms, "Transcription terminee");

    Ok(TranscribeResponse {
        text,
        model_id,
        duration_ms,
    })
}

#[derive(Debug, Serialize)]
pub struct FileTranscriptionStarted {
    /// Duree de l'audio converti, en secondes.
    pub duration_sec: f64,
}

/// Parlato : "Transcribe a file". Convertit le fichier choisi (audio ou
/// video, voir transcription::audio_file) en WAV 16 kHz dans Recordings,
/// puis lance le pipeline complet avec la source active. Retourne des que
/// la conversion est faite ; le resultat arrive par les events
/// pipeline:state (Transcribing / Enhancing / Done / Failed).
#[tauri::command]
pub async fn transcribe_file(
    app: AppHandle,
    path: String,
) -> Result<FileTranscriptionStarted, String> {
    let input = PathBuf::from(&path);
    if !input.is_file() {
        return Err("PARLA_ERR:audioMissing".into());
    }
    let dir = super::recording::recordings_dir(&app).map_err(|e| e.to_string())?;
    let wav_path = dir.join(format!("{}.wav", uuid::Uuid::new_v4()));

    let wav_clone = wav_path.clone();
    let duration_sec = tokio::task::spawn_blocking(move || {
        crate::transcription::audio_file::convert_to_wav(&input, &wav_clone)
    })
    .await
    .map_err(|e| format!("tache conversion panic: {e}"))?
    .map_err(|e| {
        let _ = std::fs::remove_file(&wav_path);
        e.to_string()
    })?;

    info!(path, duration_sec, "Fichier converti, lancement du pipeline");
    crate::transcription::pipeline::run_file(app, wav_path);
    Ok(FileTranscriptionStarted { duration_sec })
}
