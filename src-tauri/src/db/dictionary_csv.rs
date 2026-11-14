// Parlato : import et export du dictionnaire en CSV.
//
// Format : UTF-8, une ligne d'en-tete `original,replacement`, puis une ligne
// par entree. La premiere colonne est le mot (ou les variantes separees par
// des virgules, donc entre guillemets), la deuxieme le texte ecrit. Une
// entree "mot a connaitre" a les deux colonnes identiques. A la lecture, on
// accepte aussi un BOM, des fins de ligne CRLF, des champs entre guillemets
// (guillemet double = "") avec virgules et retours a la ligne, et le point
// virgule comme separateur (Excel en francais et en espagnol).

use crate::db::word_replacement::WordReplacement;

pub const HEADER: &str = "original,replacement";

/// Columns written by `serialize` and recognised as a header by `parse`.
const HEADER_FIRST: &[&str] = &["original", "word", "words", "say", "original_text", "trigger"];

fn quote(field: &str) -> String {
    if field.contains([',', '"', '\n', '\r', ';']) || field != field.trim() {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_string()
    }
}

/// CSV text for the entries, header first, LF line ends, no BOM (the file is
/// plain UTF-8; spreadsheets that want a BOM still read it correctly).
pub fn serialize(entries: &[WordReplacement]) -> String {
    let mut out = String::from(HEADER);
    out.push('\n');
    for e in entries {
        out.push_str(&quote(&e.original_text));
        out.push(',');
        out.push_str(&quote(&e.replacement_text));
        out.push('\n');
    }
    out
}

/// Header of the Snippets file (`trigger,text`).
pub const SNIPPET_HEADER: &str = "trigger,text";

/// CSV text for the snippets: `trigger,text`, the text quoted when it has
/// commas, quotes or line breaks (multi-line snippets are one record).
pub fn serialize_snippets(entries: &[crate::db::snippet::Snippet]) -> String {
    let mut out = String::from(SNIPPET_HEADER);
    out.push('\n');
    for e in entries {
        out.push_str(&quote(&e.trigger_text));
        out.push(',');
        out.push_str(&quote(&e.content));
        out.push('\n');
    }
    out
}

/// Parse a Snippets file. Same format rules as the Dictionary; a row without
/// a text column is not a snippet and counts as invalid.
pub fn parse_snippets(text: &str) -> (Vec<Row>, usize) {
    let (rows, mut invalid) = parse(text);
    let kept: Vec<Row> = rows
        .into_iter()
        .filter(|(trigger, content)| {
            let ok = trigger != content;
            if !ok {
                invalid += 1;
            }
            ok
        })
        .collect();
    (kept, invalid)
}

/// One parsed row: (original, replacement). A one-column row is a word, so
/// its replacement is itself.
pub type Row = (String, String);

/// Parse CSV text into rows. Skips the header row and blank rows. Rows with
/// an empty first column are dropped (reported through the second value).
pub fn parse(text: &str) -> (Vec<Row>, usize) {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let delim = detect_delimiter(text);
    let records = split_records(text, delim);

    let mut rows = Vec::new();
    let mut invalid = 0;
    for (n, rec) in records.into_iter().enumerate() {
        if rec.iter().all(|f| f.trim().is_empty()) {
            continue;
        }
        if n == 0 && is_header(&rec) {
            continue;
        }
        let original = rec.first().map(|s| s.trim()).unwrap_or("");
        if original.is_empty() {
            invalid += 1;
            continue;
        }
        let replacement = rec.get(1).map(|s| s.trim()).filter(|s| !s.is_empty()).unwrap_or(original);
        rows.push((original.to_string(), replacement.to_string()));
    }
    (rows, invalid)
}

fn is_header(rec: &[String]) -> bool {
    let first = rec.first().map(|s| s.trim().to_lowercase()).unwrap_or_default();
    HEADER_FIRST.contains(&first.as_str())
}

/// `;` when the first line has semicolons and no commas outside quotes.
fn detect_delimiter(text: &str) -> char {
    let mut in_quotes = false;
    let (mut commas, mut semis) = (0, 0);
    for c in text.chars() {
        match c {
            '"' => in_quotes = !in_quotes,
            '\n' | '\r' if !in_quotes => break,
            ',' if !in_quotes => commas += 1,
            ';' if !in_quotes => semis += 1,
            _ => {}
        }
    }
    if semis > 0 && commas == 0 {
        ';'
    } else {
        ','
    }
}

fn split_records(text: &str, delim: char) -> Vec<Vec<String>> {
    let mut records: Vec<Vec<String>> = Vec::new();
    let mut record: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    field.push('"');
                    chars.next();
                } else {
                    in_quotes = false;
                }
            } else {
                field.push(c);
            }
            continue;
        }
        match c {
            '"' if field.is_empty() => in_quotes = true,
            c if c == delim => record.push(std::mem::take(&mut field)),
            '\r' | '\n' => {
                if c == '\r' && chars.peek() == Some(&'\n') {
                    chars.next();
                }
                record.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut record));
            }
            c => field.push(c),
        }
    }
    if !field.is_empty() || !record.is_empty() {
        record.push(field);
        records.push(record);
    }
    records
}

/// Result of merging parsed rows into the Dictionary.
#[derive(Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct ImportSummary {
    pub added: usize,
    pub skipped: usize,
}

/// Rows to add and the counts, given the existing originals. A row is a
/// duplicate when its first column matches (ignoring case and edge spaces) an
/// existing entry or an earlier row of the same file.
pub fn merge(rows: Vec<Row>, invalid: usize, existing: &[String]) -> (Vec<Row>, ImportSummary) {
    let mut seen: std::collections::HashSet<String> =
        existing.iter().map(|s| s.trim().to_lowercase()).collect();
    let mut add = Vec::new();
    let mut skipped = invalid;
    for (o, r) in rows {
        if seen.insert(o.to_lowercase()) {
            add.push((o, r));
        } else {
            skipped += 1;
        }
    }
    let added = add.len();
    (add, ImportSummary { added, skipped })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn entry(o: &str, r: &str) -> WordReplacement {
        WordReplacement {
            id: "x".into(),
            original_text: o.into(),
            replacement_text: r.into(),
            date_added: Utc::now(),
            is_enabled: true,
        }
    }

    fn rows(pairs: &[(&str, &str)]) -> Vec<Row> {
        pairs.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect()
    }

    #[test]
    fn serialize_writes_header_and_quotes_special_fields() {
        let out = serialize(&[
            entry("Parlato", "Parlato"),
            entry("my address", "1 Main St, Springfield"),
            entry("say \"hi\"", "hello"),
            entry("multi\nline", "x"),
            entry("cooper nettys, cooper netties", "Kubernetes"),
        ]);
        assert_eq!(
            out,
            "original,replacement\nParlato,Parlato\nmy address,\"1 Main St, Springfield\"\n\"say \"\"hi\"\"\",hello\n\"multi\nline\",x\n\"cooper nettys, cooper netties\",Kubernetes\n"
        );
    }

    #[test]
    fn round_trip_keeps_commas_quotes_newlines_and_unicode() {
        let entries = vec![
            entry("a,b", "c\"d"),
            entry("jos\u{e9}, jose", "Jos\u{e9}"),
            entry("line1\r\nline2", "x"),
            entry(" padded ", "y"),
        ];
        let (parsed, invalid) = parse(&serialize(&entries));
        assert_eq!(invalid, 0);
        assert_eq!(
            parsed,
            rows(&[
                ("a,b", "c\"d"),
                ("jos\u{e9}, jose", "Jos\u{e9}"),
                ("line1\r\nline2", "x"),
                ("padded", "y"),
            ])
        );
    }

    #[test]
    fn parse_strips_bom_and_header_and_handles_crlf() {
        let (r, inv) = parse("\u{feff}original,replacement\r\nfoo,bar\r\nbaz,qux\r\n");
        assert_eq!((r, inv), (rows(&[("foo", "bar"), ("baz", "qux")]), 0));
    }

    #[test]
    fn parse_without_header_keeps_the_first_row() {
        let (r, _) = parse("foo,bar\nbaz,qux");
        assert_eq!(r, rows(&[("foo", "bar"), ("baz", "qux")]));
    }

    #[test]
    fn parse_accepts_other_header_names() {
        let (r, _) = parse("Word,Replacement\nfoo,bar\n");
        assert_eq!(r, rows(&[("foo", "bar")]));
    }

    #[test]
    fn parse_handles_quoted_commas_escaped_quotes_and_newlines() {
        let (r, _) = parse("original,replacement\n\"a, b\",\"he said \"\"yes\"\"\"\n\"two\nlines\",z\n");
        assert_eq!(r, rows(&[("a, b", "he said \"yes\""), ("two\nlines", "z")]));
    }

    #[test]
    fn a_single_column_row_is_a_word() {
        let (r, _) = parse("original,replacement\nKubernetes\nAcme,\n");
        assert_eq!(r, rows(&[("Kubernetes", "Kubernetes"), ("Acme", "Acme")]));
    }

    #[test]
    fn blank_rows_are_ignored_and_empty_originals_counted() {
        let (r, inv) = parse("original,replacement\n\n,orphan\nok,fine\n   ,  \n");
        assert_eq!(r, rows(&[("ok", "fine")]));
        assert_eq!(inv, 1);
    }

    #[test]
    fn semicolon_files_from_spreadsheets_are_read() {
        let (r, _) = parse("original;replacement\nfoo;bar\n\"a;b\";c\n");
        assert_eq!(r, rows(&[("foo", "bar"), ("a;b", "c")]));
    }

    #[test]
    fn last_line_without_a_newline_is_kept() {
        let (r, _) = parse("original,replacement\nfoo,bar");
        assert_eq!(r, rows(&[("foo", "bar")]));
        assert_eq!(parse("").0, Vec::<Row>::new());
    }

    #[test]
    fn merge_skips_duplicates_of_existing_entries_and_of_the_file() {
        let existing = vec!["Parlato".to_string(), " my address ".to_string()];
        let (add, summary) = merge(
            rows(&[
                ("parlato", "Parlato"),
                ("Acme", "Acme"),
                ("acme", "ACME"),
                ("My Address", "x"),
                ("Zed", "Zed"),
            ]),
            1,
            &existing,
        );
        assert_eq!(add, rows(&[("Acme", "Acme"), ("Zed", "Zed")]));
        assert_eq!(summary, ImportSummary { added: 2, skipped: 4 });
    }

    // -- snippets ---------------------------------------------------------

    fn snippet(trigger: &str, content: &str) -> crate::db::snippet::Snippet {
        crate::db::snippet::Snippet {
            id: "x".into(),
            trigger_text: trigger.into(),
            content: content.into(),
            date_added: Utc::now(),
            is_enabled: true,
        }
    }

    #[test]
    fn snippets_round_trip_with_multi_line_text() {
        let list = vec![
            snippet("my address", "1 Main St,\nSpringfield"),
            snippet("sig", "Best,\n\nMiguel \"M\""),
            snippet("mi dirección", "Calle 1; piso 2"),
        ];
        let csv = serialize_snippets(&list);
        assert!(csv.starts_with("trigger,text\n"));
        let (rows_back, invalid) = parse_snippets(&csv);
        assert_eq!(invalid, 0);
        assert_eq!(
            rows_back,
            rows(&[
                ("my address", "1 Main St,\nSpringfield"),
                ("sig", "Best,\n\nMiguel \"M\""),
                ("mi dirección", "Calle 1; piso 2"),
            ])
        );
    }

    #[test]
    fn a_snippet_row_without_text_is_invalid() {
        let (r, invalid) = parse_snippets("trigger,text\nsig,Best\nlonely\nalso,\n");
        assert_eq!(r, rows(&[("sig", "Best")]));
        assert_eq!(invalid, 2);
    }
}
