// Parlato : "Review before pasting". Quand ce reglage est actif, la dictee
// terminee (nettoyage IA compris) n'est pas collee : une petite fenetre
// montre le texte dans un champ modifiable. Entree colle dans l'application
// ou la personne ecrivait, Maj+Entree fait un retour a la ligne, Echap (ou
// Annuler) garde le texte dans l'historique seulement.
//
// Une fenetre a part (`recorder-review`) plutot que la bulle agrandie : la
// bulle ne prend jamais le clavier (panneau non activant sur Mac,
// WS_EX_NOACTIVATE sur Windows) et sa vie est liee a la dictee, qui se
// termine bien avant que la personne ait relu. Le texte attend ici, sans
// delai, hors du pipeline (les modeles ne restent pas charges pour lui).
//
//   - macOS : la fenetre est un panneau non activant (`make_floating_panel`)
//     qui peut devenir "key" seulement pendant que l'editeur est ouvert
//     (`set_accepts_key`). Parlato ne s'active pas : l'app de la personne
//     reste devant, et le collage part vers elle.
//   - Windows : la fenetre n'a PAS WS_EX_NOACTIVATE (on ne le pose jamais sur
//     elle), donc elle s'active normalement a l'ouverture ; rien a retirer ni
//     a remettre. Au collage elle est fermee, puis `paste_at_cursor` redonne
//     le premier plan a la fenetre memorisee au debut de la dictee
//     (`remember_foreground`), comme pour un collage normal.
//
// Historique : le texte corrige a la main va dans `edited_text` (meme colonne
// que la correction depuis l'historique), seulement s'il a change.
//
// Garde-fous : si la fenetre ne peut pas etre creee, ou si sa page ne se
// signale pas (`review_text`, appelee au montage) dans les `READY_TIMEOUT`,
// le texte est colle normalement (comme sans "Review before pasting") au
// lieu de rester dans une fenetre morte qui bloquerait les dictees. Le
// bouton Coller / Entree reste inactif tant que le texte n'est pas charge.
//
// Une nouvelle dictee est refusee tant que l'editeur est ouvert (`is_open`) :
// la cible du collage memorisee ne doit pas etre ecrasee par l'editeur.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use parking_lot::Mutex;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};
use tracing::{info, warn};

use crate::audio::feedback::{self, Cue};
use crate::db::{transcription as history_repo, Database};
use crate::paste::{self, AutoSendKey};
use crate::transcription::pipeline;

pub const LABEL: &str = "recorder-review";
/// Code montre quand on tente de dicter pendant que l'editeur est ouvert.
pub const OPEN_CODE: &str = "PARLA_ERR:reviewOpen";

const WIDTH: f64 = 440.0;
const HEIGHT: f64 = 250.0;
/// Laisse la fenetre rendre le premier plan avant de coller.
const CLOSE_SETTLE: Duration = Duration::from_millis(80);
const MODIFIER_RELEASE: Duration = Duration::from_millis(500);
/// Delai accorde a la page pour monter et demander son texte.
const READY_TIMEOUT: Duration = Duration::from_secs(5);

struct Pending {
    /// Numero de cette ouverture : une fermeture tardive d'une ancienne
    /// fenetre n'efface pas la suivante.
    generation: u64,
    /// Le texte montre a l'ouverture (pour savoir s'il a ete modifie).
    text: String,
    history_id: Option<String>,
    /// Touche d'envoi du Power mode de la dictee, pressee apres le collage.
    send: AutoSendKey,
    /// La page a demande son texte (elle est montee et le montre).
    ready: bool,
}

static PENDING: Mutex<Option<Pending>> = Mutex::new(None);
static GENERATION: AtomicU64 = AtomicU64::new(0);

/// L'editeur est-il ouvert ?
pub fn is_open() -> bool {
    PENDING.lock().is_some()
}

/// Le clavier va a l'editeur seulement pendant qu'il est ouvert (macOS ; sur
/// Windows la fenetre est une fenetre ordinaire).
fn set_keyboard(on: bool) {
    #[cfg(target_os = "macos")]
    crate::mini_recorder_macos::set_accepts_key(on);
    #[cfg(not(target_os = "macos"))]
    let _ = on;
}

/// Ouvre l'editeur avec `text`. Appelee a la place du collage.
pub fn open(app: &AppHandle, text: String, history_id: Option<String>, send: AutoSendKey) {
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    // Par securite : un editeur deja ouvert est remplace (son texte est dans
    // l'historique).
    if let Some(old) = app.get_webview_window(LABEL) {
        let _ = old.close();
    }
    *PENDING.lock() = Some(Pending { generation, text, history_id, send, ready: false });
    set_keyboard(true);

    let url = WebviewUrl::App("index.html#recorder-review".into());
    let builder = WebviewWindowBuilder::new(app, LABEL, url)
        .title("Parlato Review")
        .inner_size(WIDTH, HEIGHT)
        .resizable(false)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .shadow(false)
        .transparent(true)
        .visible(false);
    let window = match builder.build() {
        Ok(w) => w,
        Err(e) => {
            warn!("Impossible de creer la fenetre de relecture: {e}");
            // Rien ne s'affiche : repli sur un collage normal (le texte est
            // aussi dans l'historique) ; les dictees suivantes ne sont pas bloquees.
            fall_back_to_paste(app, generation);
            return;
        }
    };
    #[cfg(target_os = "macos")]
    {
        crate::mini_recorder_macos::make_floating_panel(&window, true, true);
        // If the panel class could not be installed, fall back to tao's own
        // flag so the field can still take keys.
        let _ = window.set_focusable(true);
    }
    crate::mini_recorder::place_review(&window, WIDTH, HEIGHT);
    let _ = window.show();
    #[cfg(target_os = "macos")]
    {
        crate::mini_recorder_macos::make_key(&window);
        let webview: &tauri::Webview = window.as_ref();
        let _ = webview.set_focus();
    }
    #[cfg(not(target_os = "macos"))]
    let _ = window.set_focus();

    window.on_window_event(move |event| {
        // Closed by the system (Alt+F4): same as Cancel.
        if matches!(event, WindowEvent::Destroyed) {
            let mut slot = PENDING.lock();
            if slot.as_ref().is_some_and(|p| p.generation == generation) {
                slot.take();
                drop(slot);
                set_keyboard(false);
            }
        }
    });
    info!("editeur de relecture ouvert");

    // Failsafe: a page that never mounts must not block dictation forever.
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(READY_TIMEOUT);
        let stuck = PENDING
            .lock()
            .as_ref()
            .is_some_and(|p| p.generation == generation && !p.ready);
        if stuck {
            warn!("la fenetre de relecture ne s'est pas signalee, repli sur le collage");
            fall_back_to_paste(&app, generation);
        }
    });
}

/// Ferme l'editeur de l'ouverture `generation` (s'il est encore la) et colle
/// son texte normalement.
fn fall_back_to_paste(app: &AppHandle, generation: u64) {
    let pending = {
        let mut slot = PENDING.lock();
        if slot.as_ref().is_some_and(|p| p.generation == generation) {
            slot.take()
        } else {
            None
        }
    };
    let Some(pending) = pending else {
        return;
    };
    close_window(app);
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        deliver(&app, pending.text, pending.send).await;
    });
}

fn close_window(app: &AppHandle) {
    set_keyboard(false);
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.close();
    }
}

/// Le texte a relire (la fenetre le demande au montage).
#[tauri::command]
pub fn review_text() -> Option<String> {
    let mut slot = PENDING.lock();
    let pending = slot.as_mut()?;
    pending.ready = true;
    Some(pending.text.clone())
}

/// Entree / bouton Coller : colle `text` dans l'application d'origine.
#[tauri::command]
pub async fn review_paste(app: AppHandle, text: String) -> Result<(), String> {
    let Some(pending) = PENDING.lock().take() else {
        return Ok(());
    };
    close_window(&app);
    if text.trim().is_empty() {
        // Nothing to paste: the same as Cancel.
        return Ok(());
    }
    if text.trim() != pending.text.trim() {
        save_edit(&app, pending.history_id.as_deref(), &text);
    }
    deliver(&app, text, pending.send).await;
    Ok(())
}

/// Colle `text` dans l'application d'origine (la fenetre est deja fermee).
async fn deliver(app: &AppHandle, text: String, send: AutoSendKey) {
    let restore = pipeline::get_restore_clipboard(app);
    let to_paste = if pipeline::get_append_trailing_space(app) {
        format!("{text} ")
    } else {
        text
    };
    let pasted = tauri::async_runtime::spawn_blocking(move || -> anyhow::Result<()> {
        std::thread::sleep(CLOSE_SETTLE);
        // A still-held modifier (Cmd+Enter) would merge into the paste.
        paste::wait_for_modifiers_released(MODIFIER_RELEASE);
        paste::paste_at_cursor(&to_paste, restore, None)?;
        if send != AutoSendKey::None {
            if let Err(e) = paste::send_after_paste(send) {
                warn!("auto-send: {e}");
            }
        }
        Ok(())
    })
    .await;
    match pasted {
        Ok(Ok(())) => feedback::play(app, Cue::Stop),
        Ok(Err(e)) => {
            warn!("review paste: {e}");
            crate::notice::send(app, crate::notice::Notice::Failed("PARLA_ERR:pasteFailed".into()));
        }
        Err(e) => warn!("review paste join: {e}"),
    }
}

/// Echap / bouton Annuler : rien n'est colle, le texte reste dans l'historique.
#[tauri::command]
pub fn review_cancel(app: AppHandle) {
    if PENDING.lock().take().is_some() {
        feedback::play(&app, Cue::Cancel);
    }
    close_window(&app);
}

/// Garde le texte corrige comme `edited_text` de la dictee (jamais journalise).
fn save_edit(app: &AppHandle, history_id: Option<&str>, text: &str) {
    let (Some(id), Some(db)) = (history_id, app.try_state::<Database>()) else {
        return;
    };
    let saved = history_repo::set_edited_text(&db.0.lock(), id, text);
    match saved {
        Ok(_) => {
            let _ = app.emit("history:updated", id);
        }
        Err(e) => warn!(
            code = %crate::transcription::error_code::storable_error_code(&e.to_string()),
            "review: texte corrige non garde"
        ),
    }
}
