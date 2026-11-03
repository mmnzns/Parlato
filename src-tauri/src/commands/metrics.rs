// Commandes Tauri pour le panneau de performance par modele.
//
// Reference VoiceInk : Views/Metrics/ModelPerformancePanel.swift.
// Plutot que de maintenir une table SessionMetric separee comme VoiceInk
// (qui a fait une migration depuis Transcription), on agrege directement la
// table `transcriptions` car Parla persiste deja tous les champs requis
// (transcription_model_name, transcription_duration_sec, duration_sec,
// ai_enhancement_model_name, enhancement_duration_sec).

use chrono::{DateTime, Datelike, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::transcription::{
    aggregate_enhancement_metrics, aggregate_transcription_metrics, EnhancementModelMetric,
    TranscriptionModelMetric,
};
use crate::db::Database;

/// Plage temporelle a agreger. Aligne sur VoiceInk TimeFilter (ModelPerformancePanel.swift L8-30).
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetricsPeriod {
    Last7Days,
    Last30Days,
    ThisYear,
    AllTime,
}

fn period_cutoff(period: MetricsPeriod) -> Option<DateTime<Utc>> {
    let now = Utc::now();
    match period {
        MetricsPeriod::Last7Days => Some(now - chrono::Duration::days(7)),
        MetricsPeriod::Last30Days => Some(now - chrono::Duration::days(30)),
        MetricsPeriod::ThisYear => NaiveDate::from_ymd_opt(now.year(), 1, 1)
            .and_then(|d| d.and_hms_opt(0, 0, 0))
            .map(|ndt| DateTime::<Utc>::from_naive_utc_and_offset(ndt, Utc)),
        MetricsPeriod::AllTime => None,
    }
}

#[derive(Debug, Serialize)]
pub struct ModelPerformanceMetrics {
    pub transcription_models: Vec<TranscriptionModelMetric>,
    pub enhancement_models: Vec<EnhancementModelMetric>,
}

#[tauri::command]
pub fn get_model_performance_metrics(
    db: State<'_, Database>,
    period: MetricsPeriod,
) -> Result<ModelPerformanceMetrics, String> {
    let cutoff = period_cutoff(period);
    let conn = db.0.lock();
    let transcription_models = merge_renamed(
        aggregate_transcription_metrics(&conn, cutoff).map_err(|e| e.to_string())?,
    );
    let enhancement_models =
        aggregate_enhancement_metrics(&conn, cutoff).map_err(|e| e.to_string())?;
    Ok(ModelPerformanceMetrics {
        transcription_models,
        enhancement_models,
    })
}

/// Rows saved before the move to the engine's model ids carry the old id;
/// show them under the catalog id and add them to the new rows of the same
/// model.
fn merge_renamed(rows: Vec<TranscriptionModelMetric>) -> Vec<TranscriptionModelMetric> {
    let mut out: Vec<TranscriptionModelMetric> = Vec::with_capacity(rows.len());
    for mut row in rows {
        row.name = crate::models::ids::display_label(&row.name);
        match out.iter_mut().find(|m| m.name == row.name) {
            Some(existing) => {
                existing.session_count += row.session_count;
                existing.total_audio_sec += row.total_audio_sec;
                existing.total_processing_sec += row.total_processing_sec;
            }
            None => out.push(row),
        }
    }
    // Same order as the query: fastest average first.
    let avg = |m: &TranscriptionModelMetric| m.total_processing_sec / m.session_count.max(1) as f64;
    out.sort_by(|a, b| avg(a).total_cmp(&avg(b)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metric(name: &str, n: i64, audio: f64, proc_: f64) -> TranscriptionModelMetric {
        TranscriptionModelMetric {
            name: name.into(),
            session_count: n,
            total_audio_sec: audio,
            total_processing_sec: proc_,
        }
    }

    #[test]
    fn old_and_new_ids_add_up() {
        let rows = vec![
            metric("local / ggml-large-v3-turbo-q5_0", 2, 20.0, 4.0),
            metric("groq / whisper", 1, 5.0, 1.0),
            metric("local / whisper-turbo", 3, 30.0, 6.0),
        ];
        let merged = merge_renamed(rows);
        assert_eq!(merged.len(), 2);
        let turbo = merged.iter().find(|m| m.name == "local / whisper-turbo").unwrap();
        assert_eq!(turbo.session_count, 5);
        assert_eq!(turbo.total_audio_sec, 50.0);
        assert_eq!(turbo.total_processing_sec, 10.0);
    }
}
