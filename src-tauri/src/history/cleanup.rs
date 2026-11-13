// Purge time-based des transcriptions (audio uniquement OU ligne complete).
//
// Reference VoiceInk :
// - Services/AudioCleanupManager.swift : supprime le WAV N jours apres.
// - Services/TranscriptionAutoCleanupService.swift : supprime la ligne + WAV
//   N minutes apres (retentionMinutes=0 = ephemere sur completion).
// VoiceInk active au plus un des deux modes : si transcription cleanup est
// on, audio cleanup ne tourne pas.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{anyhow, Result};
use chrono::Utc;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_store::StoreExt;
use tracing::{info, warn};

use crate::db::{transcription as repo, Database};

const STORE_FILE: &str = "parla.settings.json";
const KEY_TRANSC_CLEAN: &str = "history_transcription_cleanup_enabled";
const KEY_TRANSC_MINUTES: &str = "history_transcription_retention_minutes";
const KEY_AUDIO_CLEAN: &str = "history_audio_cleanup_enabled";
const KEY_AUDIO_DAYS: &str = "history_audio_retention_days";

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RetentionSettings {
    pub transcription_cleanup: bool,
    pub transcription_retention_minutes: i64,
    pub audio_cleanup: bool,
    pub audio_retention_days: i64,
}

impl Default for RetentionSettings {
    fn default() -> Self {
        // Aligne avec VoiceInk AppDefaults : tout desactive par defaut,
        // transcription retention = 24h (1440 min), audio retention = 7 jours.
        Self {
            transcription_cleanup: false,
            transcription_retention_minutes: 1440,
            audio_cleanup: false,
            audio_retention_days: 7,
        }
    }
}

pub fn load(app: &AppHandle) -> RetentionSettings {
    let Some(store) = app.store(STORE_FILE).ok() else {
        return RetentionSettings::default();
    };
    RetentionSettings {
        transcription_cleanup: store
            .get(KEY_TRANSC_CLEAN)
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        transcription_retention_minutes: store
            .get(KEY_TRANSC_MINUTES)
            .and_then(|v| v.as_i64())
            .unwrap_or(1440),
        audio_cleanup: store
            .get(KEY_AUDIO_CLEAN)
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        audio_retention_days: store
            .get(KEY_AUDIO_DAYS)
            .and_then(|v| v.as_i64())
            .unwrap_or(7),
    }
}

pub fn save(app: &AppHandle, s: &RetentionSettings) -> Result<()> {
    let store = app
        .store(STORE_FILE)
        .map_err(|e| anyhow!("store: {e}"))?;
    store.set(
        KEY_TRANSC_CLEAN,
        serde_json::Value::Bool(s.transcription_cleanup),
    );
    store.set(
        KEY_TRANSC_MINUTES,
        serde_json::Value::Number(serde_json::Number::from(s.transcription_retention_minutes.max(0))),
    );
    store.set(KEY_AUDIO_CLEAN, serde_json::Value::Bool(s.audio_cleanup));
    store.set(
        KEY_AUDIO_DAYS,
        serde_json::Value::Number(serde_json::Number::from(s.audio_retention_days.max(0))),
    );
    store.save().map_err(|e| anyhow!("store save: {e}"))
}

fn recordings_dir(app: &AppHandle) -> Option<PathBuf> {
    let base = app.path().app_local_data_dir().ok()?;
    Some(base.join("Recordings"))
}

fn remove_wav(app: &AppHandle, audio_file_name: &str) {
    if let Some(dir) = recordings_dir(app) {
        let p = dir.join(audio_file_name);
        if p.exists() {
            if let Err(e) = std::fs::remove_file(&p) {
                warn!("cleanup remove {}: {e}", p.display());
            }
        }
    }
}

/// Lance un passage de nettoyage. A appeler au demarrage + periodiquement.
pub fn run_once(app: &AppHandle) {
    let s = load(app);
    let Some(db) = app.try_state::<Database>() else {
        return;
    };

    if s.transcription_cleanup {
        let older = Utc::now() - chrono::Duration::minutes(s.transcription_retention_minutes.max(0));
        let wavs = match repo::delete_older_than(&db.0.lock(), older) {
            Ok(v) => v,
            Err(e) => {
                warn!("cleanup delete_older_than: {e}");
                Vec::new()
            }
        };
        for w in &wavs {
            remove_wav(app, w);
        }
        if !wavs.is_empty() {
            info!(count = wavs.len(), "transcriptions anciennes purgees");
            let _ = app.emit("history:cleaned", serde_json::Value::Null);
        }
    } else if s.audio_cleanup {
        let older = Utc::now() - chrono::Duration::days(s.audio_retention_days.max(0));
        let wavs = match repo::clear_audio_older_than(&db.0.lock(), older) {
            Ok(v) => v,
            Err(e) => {
                warn!("cleanup clear_audio_older_than: {e}");
                Vec::new()
            }
        };
        for w in &wavs {
            remove_wav(app, w);
        }
        if !wavs.is_empty() {
            info!(count = wavs.len(), "fichiers audio anciens purges");
            let _ = app.emit("history:cleaned", serde_json::Value::Null);
        }
    }

    // Orphan sweep : si transcription cleanup est actif, on supprime les WAV
    // orphelins (plus references par aucune ligne). VoiceInk fait ca au
    // launch (TranscriptionAutoCleanupService).
    if s.transcription_cleanup {
        if let Some(dir) = recordings_dir(app) {
            if dir.exists() {
                let referenced: std::collections::HashSet<String> =
                    repo::all_audio_file_names(&db.0.lock())
                        .unwrap_or_default()
                        .into_iter()
                        .collect();
                let removed = sweep_orphans(&dir, &referenced, ORPHAN_MIN_AGE);
                if removed > 0 {
                    info!(count = removed, "fichiers audio orphelins purges");
                }
            }
        }
    }
}

/// Parlato : age minimum d'un fichier avant d'etre juge orphelin. Un
/// enregistrement en cours (une longue dictee, un streaming) a un fichier
/// recent que rien ne reference encore ou deja : il ne doit jamais etre
/// supprime sous ses pieds.
const ORPHAN_MIN_AGE: Duration = Duration::from_secs(60 * 60);

/// Supprime les fichiers de `dir` que `referenced` ne cite pas et qui n'ont
/// pas ete modifies depuis `min_age`. Renvoie le nombre de fichiers
/// supprimes. Un fichier dont la date est illisible est conserve.
fn sweep_orphans(
    dir: &std::path::Path,
    referenced: &std::collections::HashSet<String>,
    min_age: Duration,
) -> usize {
    let Ok(iter) = std::fs::read_dir(dir) else {
        return 0;
    };
    let now = std::time::SystemTime::now();
    let mut removed = 0;
    for entry in iter.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if referenced.contains(&name) {
            continue;
        }
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        let old_enough = meta
            .modified()
            .ok()
            .and_then(|m| now.duration_since(m).ok())
            .map(|age| age >= min_age)
            .unwrap_or(false);
        if old_enough && std::fs::remove_file(entry.path()).is_ok() {
            removed += 1;
        }
    }
    removed
}

/// Timer de cleanup quotidien. A spawn dans setup() apres l'init DB.
pub fn spawn_daily_timer(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        // Un premier passage au demarrage (orphan sweep + prune).
        run_once(&app);
        loop {
            tokio::time::sleep(Duration::from_secs(24 * 3600)).await;
            run_once(&app);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn touch(dir: &std::path::Path, name: &str, age: Duration) {
        let f = std::fs::File::create(dir.join(name)).unwrap();
        f.set_modified(std::time::SystemTime::now() - age).unwrap();
    }

    #[test]
    fn sweep_keeps_recent_and_referenced_files() {
        let dir = tempfile::tempdir().unwrap();
        touch(dir.path(), "recording-now.wav", Duration::from_secs(30));
        touch(dir.path(), "old-orphan.wav", Duration::from_secs(2 * 3600));
        touch(dir.path(), "old-kept.wav", Duration::from_secs(2 * 3600));
        let referenced: HashSet<String> = ["old-kept.wav".to_string()].into();
        let removed = sweep_orphans(dir.path(), &referenced, ORPHAN_MIN_AGE);
        assert_eq!(removed, 1);
        assert!(dir.path().join("recording-now.wav").exists());
        assert!(dir.path().join("old-kept.wav").exists());
        assert!(!dir.path().join("old-orphan.wav").exists());
    }

    #[test]
    fn sweep_leaves_subfolders_alone() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        assert_eq!(sweep_orphans(dir.path(), &HashSet::new(), Duration::ZERO), 0);
        assert!(dir.path().join("sub").exists());
    }
}
