// Parlato : extraits. Une phrase dite (le declencheur) et le texte qu'elle
// insere, sur plusieurs lignes si besoin. Stockes a cote du dictionnaire
// (meme base, meme modele que `word_replacement`) ; le remplacement lui-meme
// est dans `text_processing/snippets.rs`.

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::text_processing::phrase;

/// Longueurs maximales (le declencheur se dit, le texte se colle).
pub const MAX_TRIGGER_CHARS: usize = 100;
pub const MAX_CONTENT_CHARS: usize = 20_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snippet {
    pub id: String,
    /// La phrase a dire ("my address").
    pub trigger_text: String,
    /// Le texte insere (retours a la ligne permis).
    pub content: String,
    pub date_added: DateTime<Utc>,
    pub is_enabled: bool,
}

#[derive(Debug, Deserialize)]
pub struct NewSnippet {
    pub trigger_text: String,
    pub content: String,
    #[serde(default = "default_true")]
    pub is_enabled: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize)]
pub struct UpdateSnippet {
    pub id: String,
    #[serde(default)]
    pub trigger_text: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub is_enabled: Option<bool>,
}

/// Clef de comparaison d'un declencheur : ses mots, sans casse ni accents.
fn trigger_key(trigger: &str) -> String {
    phrase::keys(trigger).join(" ")
}

/// Declencheur et texte propres (espaces de bord retires, fins de ligne en
/// `\n`), ou le code d'erreur a montrer.
fn clean(trigger: &str, content: &str) -> Result<(String, String)> {
    let trigger = trigger.split_whitespace().collect::<Vec<_>>().join(" ");
    let content = content.replace("\r\n", "\n").replace('\r', "\n");
    let content = content.trim().to_string();
    if trigger_key(&trigger).is_empty()
        || content.is_empty()
        || trigger.chars().count() > MAX_TRIGGER_CHARS
        || content.chars().count() > MAX_CONTENT_CHARS
    {
        return Err(anyhow!("PARLA_ERR:snippetInvalid"));
    }
    Ok((trigger, content))
}

pub fn list_all(conn: &Connection) -> Result<Vec<Snippet>> {
    query(conn, "SELECT id, trigger_text, content, date_added, is_enabled FROM snippets ORDER BY lower(trigger_text)")
}

pub fn list_enabled(conn: &Connection) -> Result<Vec<Snippet>> {
    query(
        conn,
        "SELECT id, trigger_text, content, date_added, is_enabled FROM snippets
         WHERE is_enabled = 1 ORDER BY lower(trigger_text)",
    )
}

fn query(conn: &Connection, sql: &str) -> Result<Vec<Snippet>> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([], row_to_record)?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

fn exists(conn: &Connection, trigger: &str, except_id: Option<&str>) -> Result<bool> {
    let key = trigger_key(trigger);
    Ok(list_all(conn)?
        .iter()
        .any(|s| Some(s.id.as_str()) != except_id && trigger_key(&s.trigger_text) == key))
}

pub fn insert(conn: &Connection, new: NewSnippet) -> Result<Snippet> {
    let (trigger, content) = clean(&new.trigger_text, &new.content)?;
    if exists(conn, &trigger, None)? {
        return Err(anyhow!("PARLA_ERR:snippetExists"));
    }
    let record = Snippet {
        id: Uuid::new_v4().to_string(),
        trigger_text: trigger,
        content,
        date_added: Utc::now(),
        is_enabled: new.is_enabled,
    };
    conn.execute(
        "INSERT INTO snippets (id, trigger_text, content, date_added, is_enabled)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            record.id,
            record.trigger_text,
            record.content,
            record.date_added.to_rfc3339(),
            record.is_enabled as i32,
        ],
    )
    .context("insert snippet")?;
    Ok(record)
}

pub fn update(conn: &Connection, upd: UpdateSnippet) -> Result<Snippet> {
    let existing: Snippet = conn
        .query_row(
            "SELECT id, trigger_text, content, date_added, is_enabled FROM snippets WHERE id = ?1",
            [&upd.id],
            row_to_record,
        )
        .map_err(|_| anyhow!("PARLA_ERR:snippetMissing"))?;
    let (trigger, content) = clean(
        upd.trigger_text.as_deref().unwrap_or(&existing.trigger_text),
        upd.content.as_deref().unwrap_or(&existing.content),
    )?;
    if exists(conn, &trigger, Some(&existing.id))? {
        return Err(anyhow!("PARLA_ERR:snippetExists"));
    }
    let enabled = upd.is_enabled.unwrap_or(existing.is_enabled);
    conn.execute(
        "UPDATE snippets SET trigger_text = ?1, content = ?2, is_enabled = ?3 WHERE id = ?4",
        params![trigger, content, enabled as i32, existing.id],
    )?;
    Ok(Snippet {
        id: existing.id,
        trigger_text: trigger,
        content,
        date_added: existing.date_added,
        is_enabled: enabled,
    })
}

pub fn delete(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM snippets WHERE id = ?1", [id])?;
    Ok(())
}

fn row_to_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<Snippet> {
    let date_str: String = row.get(3)?;
    let date_added = DateTime::parse_from_rfc3339(&date_str)
        .map(|d| d.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now());
    let enabled: i32 = row.get(4)?;
    Ok(Snippet {
        id: row.get(0)?,
        trigger_text: row.get(1)?,
        content: row.get(2)?,
        date_added,
        is_enabled: enabled != 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::schema::init(&conn).unwrap();
        conn
    }

    fn new(trigger: &str, content: &str) -> NewSnippet {
        NewSnippet { trigger_text: trigger.into(), content: content.into(), is_enabled: true }
    }

    #[test]
    fn insert_list_update_delete() {
        let conn = db();
        let a = insert(&conn, new("  my   address ", "1 Main St\r\nSpringfield\n")).unwrap();
        assert_eq!(a.trigger_text, "my address");
        assert_eq!(a.content, "1 Main St\nSpringfield");
        insert(&conn, new("email", "me@example.com")).unwrap();
        let all = list_all(&conn).unwrap();
        assert_eq!(all.iter().map(|s| s.trigger_text.as_str()).collect::<Vec<_>>(), ["email", "my address"]);

        let u = update(
            &conn,
            UpdateSnippet { id: a.id.clone(), trigger_text: None, content: Some("New text".into()), is_enabled: Some(false) },
        )
        .unwrap();
        assert_eq!(u.content, "New text");
        assert!(!u.is_enabled);
        assert_eq!(list_enabled(&conn).unwrap().len(), 1);

        delete(&conn, &a.id).unwrap();
        assert_eq!(list_all(&conn).unwrap().len(), 1);
    }

    #[test]
    fn multi_line_content_survives_the_database() {
        let conn = db();
        insert(&conn, new("signature", "Best,\n\nMiguel\nParlato")).unwrap();
        assert_eq!(list_all(&conn).unwrap()[0].content, "Best,\n\nMiguel\nParlato");
    }

    #[test]
    fn invalid_snippets_are_refused() {
        let conn = db();
        let long_trigger = "a ".repeat(60);
        for (t, c) in [("", "x"), ("   ", "x"), ("!!!", "x"), ("hi", ""), ("hi", "  \n "), (long_trigger.as_str(), "x")] {
            let err = insert(&conn, new(t, c)).unwrap_err().to_string();
            assert_eq!(err, "PARLA_ERR:snippetInvalid", "{t:?} {c:?}");
        }
        let long = "x".repeat(MAX_CONTENT_CHARS + 1);
        assert!(insert(&conn, new("big", &long)).is_err());
        assert!(insert(&conn, new("big", &"x".repeat(MAX_CONTENT_CHARS))).is_ok());
    }

    #[test]
    fn a_trigger_is_unique_ignoring_case_accents_and_punctuation() {
        let conn = db();
        let a = insert(&conn, new("Mi dirección", "x")).unwrap();
        for dup in ["mi DIRECCION", "mi, direccion.", " mi  dirección "] {
            assert_eq!(insert(&conn, new(dup, "y")).unwrap_err().to_string(), "PARLA_ERR:snippetExists");
        }
        let b = insert(&conn, new("other", "y")).unwrap();
        // Renaming onto an existing trigger is refused; keeping its own is fine.
        let clash = UpdateSnippet { id: b.id.clone(), trigger_text: Some("MI dirección".into()), content: None, is_enabled: None };
        assert_eq!(update(&conn, clash).unwrap_err().to_string(), "PARLA_ERR:snippetExists");
        let same = UpdateSnippet { id: a.id, trigger_text: Some("mi direccion".into()), content: None, is_enabled: None };
        assert!(update(&conn, same).is_ok());
    }

    #[test]
    fn updating_a_missing_snippet_says_so() {
        let conn = db();
        let upd = UpdateSnippet { id: "nope".into(), trigger_text: None, content: None, is_enabled: None };
        assert_eq!(update(&conn, upd).unwrap_err().to_string(), "PARLA_ERR:snippetMissing");
    }
}
