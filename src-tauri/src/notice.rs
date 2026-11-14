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
    /// Le nettoyage IA etait voulu mais n'a pas tourne : le texte brut a ete
    /// colle tel que dit (au plus une fois toutes les 10 minutes).
    AiSkipped,
}

static PENDING: Mutex<Option<Notice>> = Mutex::new(None);
/// Message a montrer quand la dictee en cours aura fini (sa bulle est
/// occupee jusque-la).
static AFTER_DICTATION: Mutex<Option<Notice>> = Mutex::new(None);

/// Parlato : delai minimum entre deux messages `AiSkipped` (par lancement de
/// l'application), pour ne pas repeter le meme avis a chaque dictee.
const AI_SKIPPED_EVERY: Duration = Duration::from_secs(10 * 60);
static AI_SKIPPED_LAST: Mutex<Option<std::time::Instant>> = Mutex::new(None);

/// Vrai si au moins `every` s'est ecoule depuis le dernier message montre.
/// Ne note rien : le delai ne court que lorsque le message a vraiment ete
/// affiche (voir `flush_after_dictation`).
fn due(last: Option<std::time::Instant>, now: std::time::Instant, every: Duration) -> bool {
    match last {
        Some(t) => now.saturating_duration_since(t) >= every,
        None => true,
    }
}

/// Garde `AiSkipped` pour `flush_after_dictation`, au plus une fois toutes
/// les 10 minutes. Ne remplace jamais un message deja garde (le repli
/// carte graphique ne se dit qu'une fois). Le delai de 10 minutes ne
/// commence qu'a l'affichage : un message ecarte parce qu'un nouvel
/// enregistrement a demarre ne le consomme pas.
pub fn ai_skipped_after_dictation() {
    let mut slot = AFTER_DICTATION.lock();
    if slot.is_some() {
        return;
    }
    if due(
        *AI_SKIPPED_LAST.lock(),
        std::time::Instant::now(),
        AI_SKIPPED_EVERY,
    ) {
        *slot = Some(Notice::AiSkipped);
    }
}

/// Garde `notice` pour `flush_after_dictation`.
pub fn send_after_dictation(notice: Notice) {
    *AFTER_DICTATION.lock() = Some(notice);
}

/// Montre le message garde par `send_after_dictation`, s'il y en a un.
pub fn flush_after_dictation(app: &AppHandle) {
    let queued = AFTER_DICTATION.lock().take();
    if let Some(notice) = queued {
        let is_ai_skipped = matches!(notice, Notice::AiSkipped);
        if send(app, notice) && is_ai_skipped {
            *AI_SKIPPED_LAST.lock() = Some(std::time::Instant::now());
        }
    }
}

/// Numero de bulle de la derniere notice montree.
static SHOWN_GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// La bulle appartient-elle encore a la derniere notice (personne ne l'a
/// rouverte depuis) ? Un arret sans enregistrement ne doit pas la fermer.
pub fn is_showing() -> bool {
    let shown = SHOWN_GENERATION.load(std::sync::atomic::Ordering::SeqCst);
    shown != 0 && mini_recorder::generation() == shown
}

/// Montre `notice` dans la bulle pendant quelques secondes. Ne fait rien
/// pendant un enregistrement : la bulle est alors occupee. Retourne `true`
/// si le message a ete montre.
pub fn send(app: &AppHandle, notice: Notice) -> bool {
    if crate::commands::recording::recording_active(app) {
        info!(?notice, "notice ignoree pendant un enregistrement");
        return false;
    }
    *PENDING.lock() = Some(notice.clone());
    let generation = mini_recorder::show_for_notice(app);
    SHOWN_GENERATION.store(generation, std::sync::atomic::Ordering::SeqCst);
    let _ = app.emit("recorder:notice", &notice);
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(NOTICE_MS));
        mini_recorder::close_if_current(&app, generation);
    });
    true
}

/// Message en attente, lu par la bulle a son montage ou a l'evenement.
#[tauri::command]
pub fn take_recorder_notice() -> Option<Notice> {
    PENDING.lock().take()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn ai_skipped_is_rate_limited() {
        let every = Duration::from_secs(600);
        let t0 = Instant::now();
        assert!(due(None, t0, every), "first one is shown");
        // Shown at t0: the window runs from then.
        assert!(!due(Some(t0), t0 + Duration::from_secs(599), every));
        assert!(due(Some(t0), t0 + Duration::from_secs(600), every));
    }

    #[test]
    fn checking_does_not_consume_the_slot() {
        let every = Duration::from_secs(600);
        let t0 = Instant::now();
        // A notice that was dropped never recorded a time: still due.
        assert!(due(None, t0, every));
        assert!(due(None, t0 + Duration::from_secs(1), every));
    }

    #[test]
    fn the_notice_kind_name_matches_the_locale_key() {
        let json = serde_json::to_string(&Notice::AiSkipped).unwrap();
        assert_eq!(json, r#"{"kind":"aiSkipped"}"#);
    }
}
