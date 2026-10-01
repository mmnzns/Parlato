// Parlato : apercu du texte en direct pendant la dictee avec Parakeet Unified
// ou Nemotron (modeles locaux), comme le fait deja le streaming cloud.
//
// Reference VoiceInk : Transcription/Streaming (texte partiel affiche dans
// la mini-recorder pendant l'enregistrement). VoiceInk le fait aussi pour
// Parakeet en local via FluidAudio ; ici on s'appuie sur
// `ParakeetUnified::transcribe_chunk` de parakeet-rs.
//
// L'apercu ne sert qu'a l'affichage : le texte colle reste celui du passage
// complet habituel apres l'arret (VAD, meme qualite qu'avant). Le recorder
// audio pousse ses morceaux dans un canal ; un thread dedie les donne au
// modele et emet "streaming:event" (meme evenement que le cloud, la
// mini-recorder l'affiche deja). Le modele est charge au debut de
// l'enregistrement, ce qui evite aussi de le charger apres l'arret.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_store::StoreExt;
use tracing::{debug, warn};

use crate::audio::recorder::ChunkCallback;
use crate::transcription::cloud::streaming::session::StreamingEvent;
use crate::transcription::parakeet::ParakeetEngineState;
use crate::transcription::parakeet_model_manager::{find_variant, ParakeetKind};

struct LivePreview {
    stop: Arc<AtomicBool>,
    // Garde le canal ouvert ; le lacher reveille le thread qui s'arrete.
    _tx: Sender<Vec<i16>>,
}

static CURRENT: Mutex<Option<LivePreview>> = Mutex::new(None);

/// Demarre l'apercu si la source est Parakeet Unified et que l'option
/// "texte en direct" est active. Renvoie le callback a donner au recorder,
/// ou None (enregistrement normal, sans apercu).
pub fn start(app: &AppHandle) -> Option<ChunkCallback> {
    stop();
    if !crate::commands::settings::get_show_live_transcript(app.clone()) {
        return None;
    }
    let store = app.store("parla.settings.json").ok()?;
    if store.get("transcription_source_kind")?.as_str()? != "parakeet" {
        return None;
    }
    let model_id = store.get("selected_parakeet_model")?.as_str()?.to_string();
    let kind = find_variant(&model_id)?.kind;
    if !matches!(kind, ParakeetKind::Unified | ParakeetKind::Nemotron) {
        return None;
    }
    // Nemotron : meme langue que le passage final (sinon detection auto).
    let language = crate::transcription::pipeline::get_language(app);
    let model_dir = app
        .state::<crate::commands::parakeet::ParakeetModelManagerState>()
        .0
        .path_for_id(&model_id)?;
    let engine = app.state::<ParakeetEngineState>().0.clone();

    let (tx, rx) = mpsc::channel::<Vec<i16>>();
    let stop_flag = Arc::new(AtomicBool::new(false));
    let thread_stop = stop_flag.clone();
    let app_bg = app.clone();
    let spawned = std::thread::Builder::new()
        .name("parakeet-live".into())
        .spawn(move || {
            if let Err(e) = engine.ensure_loaded(&model_dir, kind) {
                warn!("apercu Parakeet : chargement impossible : {e}");
                return;
            }
            engine.stream_reset(language.as_deref());
            let mut shown = String::new();
            let mut pcm: Vec<f32> = Vec::new();
            loop {
                match rx.recv_timeout(Duration::from_millis(200)) {
                    Ok(chunk) => push_f32(&mut pcm, &chunk),
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => break,
                }
                if thread_stop.load(Ordering::Relaxed) {
                    break;
                }
                // Tout ce qui est arrive pendant l'inference precedente part
                // d'un coup : le modele traite toutes les fenetres pretes.
                while let Ok(chunk) = rx.try_recv() {
                    push_f32(&mut pcm, &chunk);
                }
                if pcm.is_empty() {
                    continue;
                }
                let Some(result) = engine.stream_chunk(&pcm) else {
                    // Un autre modele a ete charge (mode Power Mode) : on arrete.
                    break;
                };
                pcm.clear();
                match result {
                    Ok(text) => {
                        let text = text.trim().to_string();
                        if text != shown && !thread_stop.load(Ordering::Relaxed) {
                            shown = text.clone();
                            let _ =
                                app_bg.emit("streaming:event", StreamingEvent::Partial { text });
                        }
                    }
                    Err(e) => {
                        warn!("apercu Parakeet : {e}");
                        break;
                    }
                }
            }
            debug!("apercu Parakeet termine");
        });
    if let Err(e) = spawned {
        warn!("apercu Parakeet : thread impossible : {e}");
        return None;
    }

    let cb_tx = Mutex::new(tx.clone());
    *CURRENT.lock() = Some(LivePreview {
        stop: stop_flag,
        _tx: tx,
    });
    // Le recorder appelle ce callback sur son thread de capture : un simple
    // envoi non bloquant, jamais d'inference ici.
    Some(Arc::new(move |chunk: Vec<i16>| {
        let _ = cb_tx.lock().send(chunk);
    }))
}

/// Arrete l'apercu en cours (fin ou annulation de l'enregistrement). Le
/// thread finit au plus apres le morceau en cours de traitement.
pub fn stop() {
    if let Some(live) = CURRENT.lock().take() {
        live.stop.store(true, Ordering::Relaxed);
    }
}

fn push_f32(out: &mut Vec<f32>, chunk: &[i16]) {
    out.extend(chunk.iter().map(|&s| s as f32 / 32768.0));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_pcm16_to_unit_range() {
        let mut out = Vec::new();
        push_f32(&mut out, &[0, 16384, -32768]);
        assert_eq!(out, vec![0.0, 0.5, -1.0]);
    }
}
