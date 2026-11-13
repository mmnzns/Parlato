// Commandes Tauri exposees au frontend pour la capture audio.
//
// Reference VoiceInk : VoiceInk/Recorder.swift + VoiceInk/VoiceInkEngine.swift
// - VoiceInk nomme le WAV uuid.uuidString dans ~/Library/.../Recordings/
// - Nous utilisons AppLocalData/Recordings/{uuid}.wav

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use parking_lot::Mutex;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tracing::{info, warn};
use uuid::Uuid;

use crate::audio::feedback::{self, Cue};
use crate::audio::{
    list_input_devices,
    recorder::{ChunkCallback, FailureCallback, RecorderFailure},
    AudioDeviceInfo, AudioMeter, AudioRecorder, RecorderConfig, RecorderHandle,
};

/// Parlato : ou en est l'enregistreur. `Starting` existe pour ne pas garder le
/// verrou de l'etat pendant l'ouverture du peripherique (jusqu'a 5 s si le
/// pilote est lent) : le compteur de niveau, `is_recording` et le tray lisent
/// l'etat sans attendre.
#[derive(Debug)]
pub enum Slot<H> {
    Idle,
    Starting,
    Recording(H),
}

impl<H> Slot<H> {
    /// Idle -> Starting. Faux (rien ne change) si un enregistrement est deja
    /// en cours ou en train de demarrer.
    pub fn begin_start(&mut self) -> bool {
        if matches!(self, Slot::Idle) {
            *self = Slot::Starting;
            true
        } else {
            false
        }
    }

    /// Starting -> Recording. Rend le handle si on n'etait pas en `Starting`
    /// (l'appelant doit alors l'arreter proprement, il ne reste a personne).
    pub fn finish_start(&mut self, handle: H) -> Option<H> {
        if matches!(self, Slot::Starting) {
            *self = Slot::Recording(handle);
            None
        } else {
            Some(handle)
        }
    }

    /// Starting -> Idle (le demarrage a echoue).
    pub fn abort_start(&mut self) {
        if matches!(self, Slot::Starting) {
            *self = Slot::Idle;
        }
    }

    /// Recording -> Idle, en rendant le handle.
    pub fn take_recording(&mut self) -> Option<H> {
        match std::mem::replace(self, Slot::Idle) {
            Slot::Recording(h) => Some(h),
            other => {
                *self = other;
                None
            }
        }
    }

    /// Un enregistrement tourne ou est en train de demarrer.
    pub fn is_active(&self) -> bool {
        !matches!(self, Slot::Idle)
    }

    pub fn is_starting(&self) -> bool {
        matches!(self, Slot::Starting)
    }

    pub fn recording(&self) -> Option<&H> {
        match self {
            Slot::Recording(h) => Some(h),
            _ => None,
        }
    }
}

/// Etat partage du recorder courant.
pub struct RecorderState(Mutex<Slot<RecorderHandle>>);

impl Default for RecorderState {
    fn default() -> Self {
        Self(Mutex::new(Slot::Idle))
    }
}

impl RecorderState {
    /// Enregistrement en cours OU demarrage en cours.
    pub fn is_active(&self) -> bool {
        self.0.lock().is_active()
    }

    pub fn is_starting(&self) -> bool {
        self.0.lock().is_starting()
    }

    fn meter(&self) -> AudioMeter {
        self.0
            .lock()
            .recording()
            .map(|h| h.current_meter())
            .unwrap_or_default()
    }
}

/// Parlato : numero de la derniere dictee demarree. Etiquette les evenements
/// `recording:*` et `pipeline:state` pour que la bulle ignore ceux d'une dictee
/// plus ancienne que celle qu'elle montre.
static DICTATION: AtomicU64 = AtomicU64::new(0);

/// Numero de la dictee la plus recente (0 = aucune depuis le lancement).
pub fn current_dictation() -> u64 {
    DICTATION.load(Ordering::SeqCst)
}

fn next_dictation() -> u64 {
    DICTATION.fetch_add(1, Ordering::SeqCst) + 1
}

/// Parlato : bouton Annuler de la bulle pendant la transcription.
#[tauri::command]
pub fn cancel_processing(app: AppHandle) {
    crate::transcription::pipeline::cancel_processing(&app);
}

/// Parlato : un enregistrement est-il en cours ?
pub fn recording_active(app: &AppHandle) -> bool {
    app.try_state::<RecorderState>()
        .map(|s| s.is_active())
        .unwrap_or(false)
}

#[derive(Debug, Serialize, Clone)]
pub struct RecordingStarted {
    pub recording_id: String,
    pub wav_path: String,
    /// Parlato : numero de la dictee (voir `current_dictation`).
    pub dictation_id: u64,
}

#[derive(Debug, Serialize, Clone)]
pub struct RecordingStopped {
    pub wav_path: String,
    pub dictation_id: u64,
}

#[tauri::command]
pub fn list_audio_devices() -> Vec<AudioDeviceInfo> {
    list_input_devices()
}

#[tauri::command]
pub fn start_recording(
    app: AppHandle,
    state: State<'_, RecorderState>,
    device_name: Option<String>,
) -> Result<RecordingStarted, String> {
    start_recording_core(&app, &state, device_name).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn stop_recording(
    app: AppHandle,
    state: State<'_, RecorderState>,
    #[allow(non_snake_case)] runPipeline: Option<bool>,
) -> Result<RecordingStopped, String> {
    let stopped = stop_recording_core(&app, &state).map_err(|e| e.to_string())?;
    if runPipeline.unwrap_or(true) {
        crate::transcription::pipeline::run_after_recording(
            app.clone(),
            std::path::PathBuf::from(&stopped.wav_path),
            // Parlato : ces commandes (test du micro) n'ouvrent jamais de
            // session Power Mode ; en prendre une ici ferait terminer par ce
            // passage la session d'une vraie dictee en cours.
            None,
        );
    } else {
        crate::transcription::pipeline::recording_finished(&app);
    }
    Ok(stopped)
}

#[tauri::command]
pub fn cancel_recording(
    app: AppHandle,
    state: State<'_, RecorderState>,
) -> Result<(), String> {
    cancel_recording_core(&app, &state).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_audio_meter(state: State<'_, RecorderState>) -> AudioMeter {
    state.meter()
}

#[tauri::command]
pub fn is_recording(state: State<'_, RecorderState>) -> bool {
    state.is_active()
}

/// Parlato : un enregistrement ou une transcription est-il en cours ? (la mise
/// a jour attend la fin : redemarrer couperait la dictee ou le fichier.)
#[tauri::command]
pub fn is_busy(state: State<'_, RecorderState>) -> bool {
    state.is_active() || crate::transcription::pipeline::is_busy()
}

/// Bouton d'enregistrement de la bulle (VoiceInk RecorderRecordButton) :
/// meme cycle complet que le raccourci et le menu tray.
#[tauri::command]
pub fn toggle_recording_from_ui(app: AppHandle) {
    // `toggle_from_ui` tourne sur son propre thread (creer la bulle depuis
    // une commande synchrone peut bloquer sous Windows).
    crate::transcription::engine::toggle_from_ui(&app);
}

// ============================================================================
// Helpers reutilisables (hotkey manager, etc.)
// ============================================================================

pub fn start_recording_core(
    app: &AppHandle,
    state: &RecorderState,
    device_name: Option<String>,
) -> anyhow::Result<RecordingStarted> {
    start_recording_core_with_chunk(app, state, device_name, None)
}

/// Variante permettant de passer un callback qui recoit chaque chunk audio
/// Int16 16 kHz mono (utilise pour le streaming cloud).
pub fn start_recording_core_with_chunk(
    app: &AppHandle,
    state: &RecorderState,
    device_name: Option<String>,
    on_chunk: Option<ChunkCallback>,
) -> anyhow::Result<RecordingStarted> {
    // Parlato : pas de dictee tant que l'editeur "Review before pasting" est ouvert.
    if crate::review::is_open() {
        anyhow::bail!(crate::review::OPEN_CODE);
    }
    // Parlato : le verrou n'est tenu que le temps de passer en `Starting` puis
    // d'enregistrer le resultat, jamais pendant l'ouverture du peripherique.
    if !state.0.lock().begin_start() {
        anyhow::bail!("un enregistrement est deja en cours");
    }
    let dictation_id = next_dictation();
    let started = match open_recorder(app, device_name, on_chunk, dictation_id) {
        Ok(opened) => opened,
        Err(e) => {
            state.0.lock().abort_start();
            return Err(e);
        }
    };
    let (handle, id, wav_path) = started;
    // Les modeles ne se liberent pas pendant l'enregistrement, aussi long
    // soit-il : il compte comme activite des son debut. Compte AVANT de passer
    // en `Recording` : un arret ne peut prendre le handle qu'apres, donc son
    // `recording_finished` trouve toujours l'activite comptee (compter apres
    // laissait l'activite coincee si l'arret arrivait entre les deux).
    crate::transcription::pipeline::recording_started();
    if let Some(orphan) = state.0.lock().finish_start(handle) {
        // Ne devrait pas arriver (seul ce chemin quitte `Starting`) : on ne
        // laisse pas le peripherique ouvert.
        let _ = orphan.stop();
        crate::transcription::pipeline::recording_finished(app);
        anyhow::bail!("l'enregistreur a change d'etat pendant son demarrage");
    }

    info!(recording_id = %id, dictation_id, path = %wav_path.display(), "Enregistrement demarre");

    let started = RecordingStarted {
        recording_id: id,
        wav_path: wav_path.to_string_lossy().into_owned(),
        dictation_id,
    };
    let _ = app.emit("recording:started", started.clone());
    Ok(started)
}

/// Prepare et ouvre le peripherique (sans toucher a l'etat partage).
fn open_recorder(
    app: &AppHandle,
    device_name: Option<String>,
    on_chunk: Option<ChunkCallback>,
    dictation: u64,
) -> anyhow::Result<(RecorderHandle, String, PathBuf)> {
    // Parlato (macOS) : cpal lists the devices and records silence when the
    // microphone is refused, so ask TCC first and say so with a stable code.
    // (Windows reports the refusal when the capture opens: audio/recorder.rs.)
    #[cfg(target_os = "macos")]
    if crate::commands::permissions_macos::microphone_denied() {
        warn!("acces au micro refuse (macOS)");
        anyhow::bail!(crate::audio::recorder::MIC_PERMISSION_CODE);
    }
    // L'IA locale se charge en arriere-plan pendant l'enregistrement : le
    // nettoyage ne l'attendra pas (no-op si deja chargee ou non choisie).
    crate::enhancement::local_ai::preload_if_wanted(app);

    // Start cue, played before the microphone capture begins so it never
    // leaks into the recording (VoiceInk SoundManager.playStartSound). On the
    // hotkey path the system mute is deferred ~250ms (audio/mute.rs) so the
    // speakers are still audible here. Fire-and-forget, no-op if disabled.
    feedback::play(app, Cue::Start);

    let dir = recordings_dir(app)?;
    let id = Uuid::new_v4().to_string();
    let wav_path = dir.join(format!("{id}.wav"));

    // Sans peripherique explicite (hotkey, tray), on utilise le micro
    // choisi dans les reglages / le menu tray, sinon le defaut systeme.
    // Reference VoiceInk AudioDeviceManager (systemDefault / custom).
    let device_name = device_name.or_else(|| crate::audio::device::selected_input_device(app));
    let config = RecorderConfig {
        device_name,
        output_path: wav_path.clone(),
    };

    // Parlato : micro perdu / disque plein en cours de route. Le rappel tourne
    // sur le thread worker, qui n'attend rien : le traitement part sur son
    // propre thread.
    let failure_app = app.clone();
    let on_failure: FailureCallback = Arc::new(move |failure: RecorderFailure| {
        crate::transcription::engine::on_recorder_failure(&failure_app, dictation, failure);
    });

    let handle = AudioRecorder::start(config, on_chunk, Some(on_failure))?;
    Ok((handle, id, wav_path))
}

pub fn stop_recording_core(
    app: &AppHandle,
    state: &RecorderState,
) -> anyhow::Result<RecordingStopped> {
    let Some(handle) = state.0.lock().take_recording() else {
        anyhow::bail!("aucun enregistrement en cours");
    };
    let stopped = handle.stop();
    let path = match stopped {
        Ok(p) => p,
        Err(e) => {
            // Plus d'enregistrement, et pas de passage derriere.
            crate::transcription::pipeline::recording_finished(app);
            return Err(e);
        }
    };
    info!(path = %path.display(), "Enregistrement termine");
    // The stop cue is NOT played here. Like VoiceInk (playStopSound at the
    // paste site), it fires at the end of the pipeline once the text is
    // inserted at the cursor - see transcription/pipeline.rs finalize_text.
    // That also avoids the system-mute resumption-delay clipping the cue.
    let stopped = RecordingStopped {
        wav_path: path.to_string_lossy().into_owned(),
        dictation_id: current_dictation(),
    };
    let _ = app.emit("recording:stopped", stopped.clone());
    Ok(stopped)
}

pub fn cancel_recording_core(
    app: &AppHandle,
    state: &RecorderState,
) -> anyhow::Result<()> {
    let Some(handle) = state.0.lock().take_recording() else {
        return Ok(());
    };
    let stopped = handle.stop();
    // Annule : plus rien ne le retient, que l'arret ait reussi ou non.
    crate::transcription::pipeline::recording_finished(app);
    let path = stopped?;
    if let Err(e) = std::fs::remove_file(&path) {
        warn!("Impossible de supprimer l'enregistrement annule {}: {e}", path.display());
    }
    info!(path = %path.display(), "Enregistrement annule");
    // Cancel / ESC cue (VoiceInk SoundManager.playEscSound). Only when there
    // was an active recording to cancel (early return above otherwise).
    feedback::play(app, Cue::Cancel);
    let _ = app.emit("recording:cancelled", current_dictation());
    Ok(())
}

pub fn recordings_dir(app: &AppHandle) -> anyhow::Result<PathBuf> {
    let base = app
        .path()
        .app_local_data_dir()
        .map_err(|e| anyhow::anyhow!("app_local_data_dir: {e}"))?;
    let dir = base.join("Recordings");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_start_moves_idle_to_starting_to_recording() {
        let mut slot: Slot<u32> = Slot::Idle;
        assert!(!slot.is_active());
        assert!(slot.begin_start());
        assert!(slot.is_starting());
        assert!(slot.is_active(), "starting counts as busy");
        assert!(slot.recording().is_none(), "no handle yet: the meter reads silence");
        assert_eq!(slot.finish_start(7), None);
        assert!(!slot.is_starting());
        assert_eq!(slot.recording(), Some(&7));
    }

    #[test]
    fn a_second_start_is_refused_while_starting_or_recording() {
        let mut slot: Slot<u32> = Slot::Idle;
        assert!(slot.begin_start());
        assert!(!slot.begin_start(), "refused while starting");
        slot.finish_start(1);
        assert!(!slot.begin_start(), "refused while recording");
        assert_eq!(slot.recording(), Some(&1), "the running recording is untouched");
    }

    #[test]
    fn a_failed_start_returns_to_idle() {
        let mut slot: Slot<u32> = Slot::Idle;
        assert!(slot.begin_start());
        slot.abort_start();
        assert!(!slot.is_active());
        assert!(slot.begin_start(), "can start again");
    }

    #[test]
    fn abort_does_not_touch_a_running_recording() {
        let mut slot: Slot<u32> = Slot::Idle;
        slot.begin_start();
        slot.finish_start(3);
        slot.abort_start();
        assert_eq!(slot.recording(), Some(&3));
    }

    #[test]
    fn take_recording_only_takes_a_running_recording() {
        let mut slot: Slot<u32> = Slot::Idle;
        assert_eq!(slot.take_recording(), None);
        slot.begin_start();
        assert_eq!(slot.take_recording(), None, "nothing to stop while starting");
        assert!(slot.is_starting(), "and the start is left alone");
        slot.finish_start(5);
        assert_eq!(slot.take_recording(), Some(5));
        assert!(!slot.is_active());
    }

    #[test]
    fn a_handle_that_arrives_outside_starting_is_given_back() {
        let mut slot: Slot<u32> = Slot::Idle;
        assert_eq!(slot.finish_start(9), Some(9));
        assert!(!slot.is_active());
    }

    #[test]
    fn the_state_is_readable_while_a_start_is_in_flight() {
        // The lock is never held across the device open: a reader gets an
        // answer even though the start has not finished.
        let state = RecorderState::default();
        assert!(state.0.lock().begin_start());
        assert!(state.is_active());
        assert!(state.is_starting());
        assert_eq!(state.meter().rms_db, AudioMeter::default().rms_db);
    }

    #[test]
    fn dictation_ids_only_go_up() {
        let a = next_dictation();
        let b = next_dictation();
        assert!(b > a);
        assert!(current_dictation() >= b);
    }
}
