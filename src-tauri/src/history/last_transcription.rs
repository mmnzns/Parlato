// Actions sur la derniere transcription : copier, coller (originale ou
// amelioree), reessayer, ouvrir l'historique.
//
// Reference VoiceInk : Features/History/Workflows/LastTranscriptionService.swift
//  - copyLastTranscription : texte ameliore si present, sinon original,
//    copie dans le presse-papiers + notification.
//  - pasteLastTranscription : texte ORIGINAL colle au curseur apres 0.15 s.
//  - pasteLastEnhancement   : texte ameliore (repli original) colle au
//    curseur apres 0.15 s.
//  - retryLastTranscription : retranscrit le dernier fichier audio avec le
//    modele courant (enhancement compris), copie le resultat dans le
//    presse-papiers et cree une nouvelle entree d'historique (Parlato : avec
//    sa propre copie du WAV).
// Ces actions sont declenchees par le menu tray et par les raccourcis
// additionnels (hotkeys::keyboard_hook::UtilityAction).

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{anyhow, Result};
use tauri::{AppHandle, Manager};
use tracing::{info, warn};

use crate::db::{transcription as history_repo, Database};
use crate::notice::{self, Notice};
use crate::paste;
use crate::transcription::pipeline;

/// Delai avant le collage (VoiceInk `DispatchQueue.main.asyncAfter(0.15)`) :
/// laisse le temps aux touches du raccourci d'etre relachees pour que le
/// Ctrl+V synthetique ne soit pas combine avec un modificateur encore
/// enfonce.
const PASTE_DELAY: Duration = Duration::from_millis(150);
/// Max wait for the shortcut's modifier keys to be released (macOS).
const MODIFIER_RELEASE_TIMEOUT: Duration = Duration::from_secs(1);

/// Derniere transcription enregistree (la plus recente par timestamp).
pub fn last_record(app: &AppHandle) -> Result<history_repo::TranscriptionRecord> {
    let db = app
        .try_state::<Database>()
        .ok_or_else(|| anyhow!("database not initialized"))?;
    let page = {
        let conn = db.0.lock();
        history_repo::list_page(&conn, 1, None, None, history_repo::ListFilter::All)?
    };
    page.into_iter()
        .next()
        .ok_or_else(|| anyhow!("PARLA_ERR:historyEmpty"))
}

/// Texte corrige a la main, sinon ameliore, sinon original.
fn preferred_text(rec: &history_repo::TranscriptionRecord) -> String {
    history_repo::shown_text(rec).to_string()
}

/// Copie la derniere transcription (amelioree de preference) dans le
/// presse-papiers.
pub fn copy_last(app: &AppHandle) -> Result<()> {
    let rec = last_record(app)?;
    copy_text_of(app, &rec)
}

/// Parlato : copie une dictee precise (menu "Recent" de la barre) avec la
/// meme logique et la meme notice "Copied" que `copy_last`.
pub fn copy_record(app: &AppHandle, id: &str) -> Result<()> {
    let db = app
        .try_state::<Database>()
        .ok_or_else(|| anyhow!("database not initialized"))?;
    let rec = {
        let conn = db.0.lock();
        history_repo::get(&conn, id)?
    }
    .ok_or_else(|| anyhow!("PARLA_ERR:historyEmpty"))?;
    copy_text_of(app, &rec)
}

fn copy_text_of(app: &AppHandle, rec: &history_repo::TranscriptionRecord) -> Result<()> {
    let text = preferred_text(rec);
    if text.trim().is_empty() {
        anyhow::bail!("PARLA_ERR:lastTranscriptionEmpty");
    }
    paste::copy_to_clipboard(&text)?;
    info!(chars = text.len(), "Last transcription copied");
    notice::send(app, Notice::Copied);
    Ok(())
}

/// Parlato : les dernieres dictees reussies pour le menu de la barre, au
/// plus `max`, avec un libelle d'une ligne.
pub fn recent_for_menu(app: &AppHandle, max: usize) -> Vec<(String, String)> {
    let Some(db) = app.try_state::<Database>() else {
        return Vec::new();
    };
    let page = {
        let conn = db.0.lock();
        history_repo::list_page(&conn, 20, None, None, history_repo::ListFilter::All)
    };
    let Ok(page) = page else {
        return Vec::new();
    };
    page.iter()
        .filter(|r| r.status == history_repo::TranscriptionStatus::Completed)
        .filter_map(|r| menu_label(history_repo::shown_text(r)).map(|l| (r.id.clone(), l)))
        .take(max)
        .collect()
}

/// Number of characters of a dictation shown in the tray menu.
const MENU_LABEL_CHARS: usize = 40;

/// First ~40 characters on one line (every run of whitespace, newlines
/// included, becomes one space). None for an empty text. Native menus read
/// `&` as a shortcut marker, so it is doubled.
pub fn menu_label(text: &str) -> Option<String> {
    let one_line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if one_line.is_empty() {
        return None;
    }
    let mut label: String = one_line.chars().take(MENU_LABEL_CHARS).collect();
    if one_line.chars().count() > MENU_LABEL_CHARS {
        label = label.trim_end().to_string();
        label.push('\u{2026}');
    }
    Some(label.replace('&', "&&"))
}

/// Colle la derniere transcription au curseur. `enhanced` = true colle le
/// texte ameliore (repli original), sinon le texte original.
pub fn paste_last(app: &AppHandle, enhanced: bool) -> Result<()> {
    let rec = last_record(app)?;
    let text = if enhanced {
        preferred_text(&rec)
    } else {
        rec.text.clone()
    };
    if text.trim().is_empty() {
        anyhow::bail!("PARLA_ERR:lastTranscriptionEmpty");
    }
    // La fenetre au premier plan est celle ou l'utilisateur a presse le
    // raccourci : c'est elle qui doit recevoir le Ctrl+V, pas la derniere
    // cible memorisee au dernier enregistrement.
    paste::remember_foreground();
    let restore = pipeline::get_restore_clipboard(app);
    let app = app.clone();
    std::thread::Builder::new()
        .name("parla-paste-last".into())
        .spawn(move || {
            std::thread::sleep(PASTE_DELAY);
            // A still-held Control would merge into Cmd+V at the HID tap.
            paste::wait_for_modifiers_released(MODIFIER_RELEASE_TIMEOUT);
            match paste::paste_at_cursor(&text, restore, None) {
                Ok(()) => info!(enhanced, chars = text.len(), "Last transcription pasted"),
                Err(e) => {
                    warn!(error = %e, "paste last transcription failed");
                    notice::send(&app, Notice::Failed("PARLA_ERR:pasteFailed".into()));
                }
            }
        })
        .map_err(|e| anyhow!("thread: {e}"))?;
    Ok(())
}

/// Retranscrit le dernier fichier audio avec le modele courant et copie le
/// resultat. Refuse pendant un enregistrement (le pipeline partage l'etat
/// de session d'historique).
pub fn retry_last(app: &AppHandle) -> Result<()> {
    if crate::commands::recording::recording_active(app) {
        anyhow::bail!("PARLA_ERR:recordingInProgress");
    }
    let rec = last_record(app)?;
    let name = rec
        .audio_file_name
        .filter(|n| !n.is_empty())
        .ok_or_else(|| anyhow!("PARLA_ERR:audioMissing"))?;
    let path: PathBuf = app
        .path()
        .app_local_data_dir()
        .map_err(|e| anyhow!("app_local_data_dir: {e}"))?
        .join("Recordings")
        .join(name);
    if !path.exists() {
        anyhow::bail!("PARLA_ERR:audioMissing");
    }
    // Parlato : copie du WAV sous un nouveau nom, pour que la nouvelle
    // entree possede son propre fichier (supprimer l'une ne casse pas la
    // reecoute de l'autre). Meme regle que retranscribe_history_item.
    let copy = path.with_file_name(format!("{}.wav", uuid::Uuid::new_v4()));
    std::fs::copy(&path, &copy).map_err(|e| anyhow!("copy wav: {e}"))?;
    info!(path = %copy.display(), "Retrying last transcription");
    notice::send(app, Notice::RetryStarted);
    pipeline::run_retry(app.clone(), copy);
    Ok(())
}

/// Affiche la fenetre principale sur l'onglet Historique (equivalent de la
/// fenetre d'historique detachee de VoiceInk).
pub fn open_history(app: &AppHandle) {
    crate::mini_recorder::show_main_window(app.clone(), Some("history".into()));
}

#[cfg(test)]
mod tests {
    use super::menu_label;

    #[test]
    fn short_text_is_kept_on_one_line() {
        assert_eq!(menu_label("hello world").as_deref(), Some("hello world"));
        assert_eq!(menu_label("line one\n\nline  two\r\n\tthree").as_deref(), Some("line one line two three"));
        assert_eq!(menu_label("  \n ").as_deref(), None);
    }

    #[test]
    fn long_text_is_cut_at_forty_characters_with_an_ellipsis() {
        let text = "abcdefghij ".repeat(10);
        let label = menu_label(&text).unwrap();
        assert!(label.ends_with('\u{2026}'));
        assert!(label.chars().count() <= 41);
        assert!(!label.contains('\n'));
    }

    #[test]
    fn cuts_by_characters_not_bytes() {
        let text = "\u{e9}".repeat(60);
        let label = menu_label(&text).unwrap();
        assert_eq!(label.chars().count(), 41);
    }

    #[test]
    fn ampersands_are_doubled_for_the_native_menu() {
        assert_eq!(menu_label("Q&A").as_deref(), Some("Q&&A"));
    }
}
