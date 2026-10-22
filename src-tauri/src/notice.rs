// Parlato : courts messages affiches dans la bulle d'enregistrement quand
// une action se fait hors de la fenetre principale (raccourcis "copier /
// coller / reessayer la derniere transcription", menu de la barre des
// taches). Avant, ces resultats partaient dans un evenement que l'interface
// se contentait d'ecrire dans la console : l'utilisateur ne voyait rien.
//
// Reference VoiceInk : LastTranscriptionService affiche une notification
// systeme (NotificationManager). Ici la bulle sert de notification, sur
// Windows comme sur Mac, sans plugin ni permission en plus.
//
// La bulle peut etre creee a cette occasion : son webview n'est pas encore
// charge quand on emet l'evenement, donc le message est aussi garde ici et
// la bulle le recupere au montage (`take_recorder_notice`).

use std::time::Duration;

use parking_lot::Mutex;
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tracing::info;

use crate::mini_recorder;

/// Duree d'affichage d'un message.
const NOTICE_MS: u64 = 2500;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", content = "detail", rename_all = "camelCase")]
pub enum Notice {
    /// Derniere transcription copiee dans le presse-papiers.
    Copied,
    /// "Reessayer" lance : la transcription est en cours.
    RetryStarted,
    /// "Reessayer" termine : resultat dans le presse-papiers.
    RetryDone,
    /// Echec, avec le message d'erreur (code PARLA_ERR traduit par
    /// l'interface).
    Failed(String),
    /// Parakeet n'a pas pu utiliser la carte graphique et a utilise le
    /// processeur (une fois par lancement de l'application).
    GpuFallback,
}

static PENDING: Mutex<Option<Notice>> = Mutex::new(None);
/// Message a montrer quand la dictee en cours aura fini (sa bulle est
/// occupee jusque-la).
static AFTER_DICTATION: Mutex<Option<Notice>> = Mutex::new(None);

/// Garde `notice` pour `flush_after_dictation`.
pub fn send_after_dictation(notice: Notice) {
    *AFTER_DICTATION.lock() = Some(notice);
}

/// Montre le message garde par `send_after_dictation`, s'il y en a un.
pub fn flush_after_dictation(app: &AppHandle) {
    let queued = AFTER_DICTATION.lock().take();
    if let Some(notice) = queued {
        send(app, notice);
    }
}

/// Montre `notice` dans la bulle pendant quelques secondes. Ne fait rien
/// pendant un enregistrement : la bulle est alors occupee.
pub fn send(app: &AppHandle, notice: Notice) {
    if crate::commands::recording::recording_active(app) {
        info!(?notice, "notice ignoree pendant un enregistrement");
        return;
    }
    *PENDING.lock() = Some(notice.clone());
    let generation = mini_recorder::show_for_notice(app);
    let _ = app.emit("recorder:notice", &notice);
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(NOTICE_MS));
        mini_recorder::close_if_current(&app, generation);
    });
}

/// Message en attente, lu par la bulle a son montage ou a l'evenement.
#[tauri::command]
pub fn take_recorder_notice() -> Option<Notice> {
    PENDING.lock().take()
}
