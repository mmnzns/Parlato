// Commandes Tauri pour les extraits (Dictionnaire > Extraits).

use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::DialogExt;

use crate::db::dictionary_csv::{self, ImportSummary};
use crate::db::snippet::{self as repo, NewSnippet, Snippet, UpdateSnippet};
use crate::db::Database;

#[tauri::command]
pub fn list_snippets(db: State<'_, Database>) -> Result<Vec<Snippet>, String> {
    repo::list_all(&db.0.lock()).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn add_snippet(db: State<'_, Database>, payload: NewSnippet) -> Result<Snippet, String> {
    repo::insert(&db.0.lock(), payload).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_snippet(db: State<'_, Database>, payload: UpdateSnippet) -> Result<Snippet, String> {
    repo::update(&db.0.lock(), payload).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_snippet(db: State<'_, Database>, id: String) -> Result<(), String> {
    repo::delete(&db.0.lock(), &id).map_err(|e| e.to_string())
}

/// Exporte les extraits en CSV (`trigger,text`). Renvoie le chemin choisi, ou
/// None si la personne annule. Un fichier a part du dictionnaire : les deux
/// listes n'ont pas le meme sens.
#[tauri::command]
pub async fn export_snippets_csv(app: AppHandle) -> Result<Option<String>, String> {
    let csv = {
        let db = app.try_state::<Database>().ok_or_else(|| "DB absente".to_string())?;
        let all = repo::list_all(&db.0.lock()).map_err(|e| e.to_string())?;
        dictionary_csv::serialize_snippets(&all)
    };
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .set_file_name("parlato-snippets.csv")
        .add_filter("CSV", &["csv"])
        .save_file(move |path| {
            let _ = tx.send(path);
        });
    let Some(path) = rx.await.map_err(|e| e.to_string())? else {
        return Ok(None);
    };
    let p = path.as_path().ok_or_else(|| "chemin invalide".to_string())?.to_path_buf();
    std::fs::write(&p, csv).map_err(|e| e.to_string())?;
    Ok(Some(p.to_string_lossy().into_owned()))
}

/// Importe un CSV d'extraits. Fusion : un declencheur deja present (casse,
/// accents et ponctuation ignores) est ignore, comme les lignes invalides.
#[tauri::command]
pub async fn import_snippets_csv(app: AppHandle) -> Result<Option<ImportSummary>, String> {
    const MAX_BYTES: u64 = 5 * 1024 * 1024;
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog().file().add_filter("CSV", &["csv", "txt"]).pick_file(move |path| {
        let _ = tx.send(path);
    });
    let Some(path) = rx.await.map_err(|e| e.to_string())? else {
        return Ok(None);
    };
    let p = path.as_path().ok_or_else(|| "chemin invalide".to_string())?.to_path_buf();
    let meta = std::fs::metadata(&p).map_err(|e| e.to_string())?;
    if meta.len() > MAX_BYTES {
        return Err("PARLA_ERR:csvTooBig".into());
    }
    let bytes = std::fs::read(&p).map_err(|e| e.to_string())?;
    let text = String::from_utf8(bytes).map_err(|_| "PARLA_ERR:csvNotUtf8".to_string())?;
    let (rows, invalid) = dictionary_csv::parse_snippets(&text);
    if rows.is_empty() && invalid == 0 {
        return Err("PARLA_ERR:csvEmpty".into());
    }
    let db = app.try_state::<Database>().ok_or_else(|| "DB absente".to_string())?;
    let conn = db.0.lock();
    let existing: Vec<String> = repo::list_all(&conn)
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|s| s.trigger_text)
        .collect();
    let (to_add, mut summary) = dictionary_csv::merge(rows, invalid, &existing);
    // A row the store refuses (too long, no words, same trigger by accents)
    // is skipped, not fatal.
    let mut added = 0;
    for (trigger, content) in to_add {
        match repo::insert(&conn, NewSnippet { trigger_text: trigger, content, is_enabled: true }) {
            Ok(_) => added += 1,
            Err(_) => summary.skipped += 1,
        }
    }
    summary.added = added;
    Ok(Some(summary))
}
