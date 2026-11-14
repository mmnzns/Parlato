// Commandes Tauri pour le dictionnaire personnel (word replacements).

use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::DialogExt;

use crate::db::word_replacement::{
    self as repo, NewWordReplacement, UpdateWordReplacement, WordReplacement,
};
use crate::db::dictionary_csv::{self, ImportSummary};
use crate::db::Database;

#[tauri::command]
pub fn list_word_replacements(db: State<'_, Database>) -> Result<Vec<WordReplacement>, String> {
    let conn = db.0.lock();
    repo::list_all(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn add_word_replacement(
    db: State<'_, Database>,
    payload: NewWordReplacement,
) -> Result<WordReplacement, String> {
    let conn = db.0.lock();
    repo::insert(&conn, payload).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_word_replacement(
    db: State<'_, Database>,
    payload: UpdateWordReplacement,
) -> Result<WordReplacement, String> {
    let conn = db.0.lock();
    repo::update(&conn, payload).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_word_replacement(db: State<'_, Database>, id: String) -> Result<(), String> {
    let conn = db.0.lock();
    repo::delete(&conn, &id).map_err(|e| e.to_string())
}

/// Parlato : exporte tout le dictionnaire en CSV (UTF-8, une ligne d'en-tete).
/// Renvoie le chemin choisi, ou None si la personne annule.
#[tauri::command]
pub async fn export_dictionary_csv(app: AppHandle) -> Result<Option<String>, String> {
    let csv = {
        let db = app.try_state::<Database>().ok_or_else(|| "DB absente".to_string())?;
        let conn = db.0.lock();
        let mut all = repo::list_all(&conn).map_err(|e| e.to_string())?;
        all.sort_by_key(|w| w.original_text.to_lowercase());
        dictionary_csv::serialize(&all)
    };
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .set_file_name("parlato-dictionary.csv")
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

/// Parlato : importe un CSV dans le dictionnaire. Fusion : les doublons
/// (meme premiere colonne, casse ignoree) sont ignores. Renvoie les
/// compteurs, ou None si la personne annule.
#[tauri::command]
pub async fn import_dictionary_csv(app: AppHandle) -> Result<Option<ImportSummary>, String> {
    /// A dictionary file is a few KB; refuse anything absurd.
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
    let (rows, invalid) = dictionary_csv::parse(&text);
    if rows.is_empty() && invalid == 0 {
        return Err("PARLA_ERR:csvEmpty".into());
    }
    let db = app.try_state::<Database>().ok_or_else(|| "DB absente".to_string())?;
    let conn = db.0.lock();
    let existing: Vec<String> = repo::list_all(&conn)
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|w| w.original_text)
        .collect();
    let (to_add, summary) = dictionary_csv::merge(rows, invalid, &existing);
    for (original, replacement) in to_add {
        repo::insert(
            &conn,
            NewWordReplacement { original_text: original, replacement_text: replacement, is_enabled: true },
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(Some(summary))
}
