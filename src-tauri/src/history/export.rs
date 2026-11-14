// Export CSV des transcriptions.
//
// Reference VoiceInk : Services/VoiceInkCSVExportService.swift. Colonnes
// dans l'ordre: Original Transcript, Enhanced Transcript, Enhancement
// Model, Prompt Name, Transcription Model, Power Mode, Enhancement Time,
// Transcription Time, Timestamp, Duration. Power Mode = "<emoji> <name>".

use crate::db::transcription::TranscriptionRecord;

pub fn to_csv(records: &[TranscriptionRecord]) -> String {
    let mut out = String::new();
    out.push_str("Original Transcript,Enhanced Transcript,Enhancement Model,Prompt Name,Transcription Model,Power Mode,Enhancement Time,Transcription Time,Timestamp,Duration\n");
    for r in records {
        out.push_str(&format!(
            "{},{},{},{},{},{},{},{},{},{}\n",
            csv_escape(&r.text),
            csv_escape(r.enhanced_text.as_deref().unwrap_or("")),
            csv_escape(r.ai_enhancement_model_name.as_deref().unwrap_or("")),
            csv_escape(r.prompt_name.as_deref().unwrap_or("")),
            csv_escape(r.transcription_model_name.as_deref().unwrap_or("")),
            csv_escape(&power_mode_label(r)),
            r.enhancement_duration_sec.unwrap_or(0.0),
            r.transcription_duration_sec.unwrap_or(0.0),
            r.timestamp.to_rfc3339(),
            r.duration_sec.unwrap_or(0.0),
        ));
    }
    out
}

/// Parlato : un champ qui commence par `=`, `+`, `-`, `@`, tabulation ou
/// retour chariot serait lu comme une formule par Excel / Sheets (injection
/// de formule). On le neutralise avec une apostrophe en tete, puis on cite
/// le champ selon les regles CSV (virgule, guillemet, \n et \r).
fn csv_escape(s: &str) -> String {
    let neutralized;
    let s = if s.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        neutralized = format!("'{s}");
        neutralized.as_str()
    } else {
        s
    };
    if s.contains([',', '"', '\n', '\r']) {
        let escaped = s.replace('"', "\"\"");
        format!("\"{escaped}\"")
    } else {
        s.to_string()
    }
}

fn power_mode_label(r: &TranscriptionRecord) -> String {
    match (&r.power_mode_emoji, &r.power_mode_name) {
        (Some(e), Some(n)) => format!("{e} {n}"),
        (_, Some(n)) => n.clone(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::csv_escape;

    #[test]
    fn plain_text_is_untouched() {
        assert_eq!(csv_escape("hello world"), "hello world");
        assert_eq!(csv_escape(""), "");
    }

    #[test]
    fn quotes_commas_and_every_line_break() {
        assert_eq!(csv_escape("a,b"), "\"a,b\"");
        assert_eq!(csv_escape("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(csv_escape("a\nb"), "\"a\nb\"");
        assert_eq!(csv_escape("a\rb"), "\"a\rb\"");
    }

    #[test]
    fn formula_starts_are_neutralized() {
        assert_eq!(csv_escape("=1+1"), "'=1+1");
        assert_eq!(csv_escape("+cmd"), "'+cmd");
        assert_eq!(csv_escape("-5"), "'-5");
        assert_eq!(csv_escape("@SUM(A1)"), "'@SUM(A1)");
        assert_eq!(csv_escape("\tx"), "'\tx");
        assert_eq!(csv_escape("\rx"), "\"'\rx\"");
        assert_eq!(csv_escape("=HYPERLINK(\"u\",\"v\")"), "\"'=HYPERLINK(\"\"u\"\",\"\"v\"\")\"");
        // Only a leading character counts.
        assert_eq!(csv_escape("a=b"), "a=b");
    }
}
