// Model downloads on the shared store, with the events the UI already
// listens to.
//
// `SharedStore::download` is blocking and reports progress over the whole
// model, so it runs on a blocking thread with a cancel flag, and each
// progress report becomes one of the existing payloads:
//
//   whisper   model:download:progress {id, downloaded, total}, :complete {id, path}, :error {id, message}
//   parakeet  parakeet_model:download:progress {id, downloaded, total, current_file}, :complete, :error
//   local AI  llm_model:download:progress {id, downloaded, total}, :complete, :error
//   vad       vad:download:progress {downloaded, total}, :complete {path} (no error event)
//
// Errors reach the UI as stable `PARLA_ERR:<code>` strings that
// `src/lib/translateError.ts` turns into the person's language. A cancel is
// `PARLA_ERR:cancelled`, as before.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::json;
use tauri::{AppHandle, Emitter};
use tracing::{info, warn};
use voice_engine_models::{catalog, DownloadError, StoreError};

use super::store::ModelStore;

pub const CANCELLED: &str = "PARLA_ERR:cancelled";

/// Which screen asked, which decides the event names and payloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flavor {
    Whisper,
    Parakeet,
    Llm,
    Vad,
}

impl Flavor {
    fn events(self) -> (&'static str, &'static str, Option<&'static str>) {
        match self {
            Flavor::Whisper => (
                "model:download:progress",
                "model:download:complete",
                Some("model:download:error"),
            ),
            Flavor::Parakeet => (
                "parakeet_model:download:progress",
                "parakeet_model:download:complete",
                Some("parakeet_model:download:error"),
            ),
            Flavor::Llm => (
                "llm_model:download:progress",
                "llm_model:download:complete",
                Some("llm_model:download:error"),
            ),
            Flavor::Vad => ("vad:download:progress", "vad:download:complete", None),
        }
    }

    fn progress_payload(self, id: &str, downloaded: u64, total: u64) -> serde_json::Value {
        match self {
            Flavor::Vad => json!({ "downloaded": downloaded, "total": total }),
            // The engine reports over the whole model, not per file.
            Flavor::Parakeet => {
                json!({ "id": id, "downloaded": downloaded, "total": total, "current_file": "" })
            }
            _ => json!({ "id": id, "downloaded": downloaded, "total": total }),
        }
    }

    fn complete_payload(self, id: &str, path: &str) -> serde_json::Value {
        match self {
            Flavor::Vad => json!({ "path": path }),
            _ => json!({ "id": id, "path": path }),
        }
    }
}

/// The message the UI shows for a failed download. Never includes file
/// contents or paths.
pub fn error_code(e: &StoreError) -> String {
    match e {
        StoreError::DownloadingElsewhere => "PARLA_ERR:downloadingElsewhere".into(),
        StoreError::Download(d) => match d {
            DownloadError::Cancelled => CANCELLED.into(),
            DownloadError::Offline => "PARLA_ERR:downloadOffline".into(),
            DownloadError::Refused { status } => format!("PARLA_ERR:downloadRefused:{status}"),
            DownloadError::DiskFull => "PARLA_ERR:downloadDiskFull".into(),
            DownloadError::Checksum { .. } => "PARLA_ERR:downloadChecksum".into(),
            DownloadError::Stalled => "PARLA_ERR:downloadStalled".into(),
            DownloadError::Io(_) => "PARLA_ERR:downloadFailed".into(),
        },
        StoreError::UnknownModel(_) | StoreError::NotInstalled(_) => {
            "PARLA_ERR:downloadFailed".into()
        }
        StoreError::Io(_) => "PARLA_ERR:downloadFailed".into(),
    }
}

/// Downloads `id` into the shared store. Returns the path the runtime loads
/// (a file, or the folder for the ONNX families).
pub async fn run(
    app: &AppHandle,
    store: Arc<ModelStore>,
    id: &str,
    flavor: Flavor,
) -> Result<String, String> {
    let Some(model) = catalog::find(id) else {
        return Err("PARLA_ERR:downloadFailed".into());
    };
    let (progress_event, complete_event, error_event) = flavor.events();
    // A second click while one runs returns at once, without an error event
    // that would clear the first one's progress.
    let Some(cancel) = store.begin_download(id) else {
        return Err("PARLA_ERR:downloadRunning".into());
    };

    let worker = {
        let (app, store, id) = (app.clone(), store.clone(), id.to_string());
        tokio::task::spawn_blocking(move || {
            // ~20 Hz at most, so the IPC bridge is not flooded.
            let mut last = Instant::now() - Duration::from_secs(1);
            store.shared().download(&id, &cancel, &mut |p| {
                let done = p.bytes >= p.total;
                if done || last.elapsed() >= Duration::from_millis(50) {
                    last = Instant::now();
                    let _ = app.emit(
                        progress_event,
                        flavor.progress_payload(&id, p.bytes, p.total),
                    );
                }
            })
        })
        .await
    };
    store.end_download(id);

    let result: Result<PathBuf, String> = match worker {
        Ok(Ok(dir)) => Ok(ModelStore::installed_path(model, &dir)),
        Ok(Err(e)) => {
            if !matches!(e, StoreError::Download(DownloadError::Cancelled)) {
                warn!(id, error = %e, "model download failed");
            }
            Err(error_code(&e))
        }
        Err(e) => Err(format!("download task: {e}")),
    };

    match result {
        Ok(path) => {
            let path = path.to_string_lossy().into_owned();
            info!(id, "model downloaded");
            let _ = app.emit(complete_event, flavor.complete_payload(id, &path));
            Ok(path)
        }
        Err(message) => {
            if let Some(event) = error_event {
                let _ = app.emit(event, json!({ "id": id, "message": message }));
            }
            Err(message)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_become_stable_codes() {
        let code = |e: StoreError| error_code(&e);
        assert_eq!(
            code(StoreError::Download(DownloadError::Cancelled)),
            "PARLA_ERR:cancelled"
        );
        assert_eq!(
            code(StoreError::Download(DownloadError::Offline)),
            "PARLA_ERR:downloadOffline"
        );
        assert_eq!(
            code(StoreError::Download(DownloadError::Refused { status: 503 })),
            "PARLA_ERR:downloadRefused:503"
        );
        assert_eq!(
            code(StoreError::Download(DownloadError::DiskFull)),
            "PARLA_ERR:downloadDiskFull"
        );
        assert_eq!(
            code(StoreError::Download(DownloadError::Checksum {
                file: "x.bin".into()
            })),
            "PARLA_ERR:downloadChecksum"
        );
        assert_eq!(
            code(StoreError::Download(DownloadError::Stalled)),
            "PARLA_ERR:downloadStalled"
        );
        assert_eq!(
            code(StoreError::Download(DownloadError::Io(
                "/secret/path".into()
            ))),
            "PARLA_ERR:downloadFailed"
        );
        assert_eq!(
            code(StoreError::DownloadingElsewhere),
            "PARLA_ERR:downloadingElsewhere"
        );
        assert_eq!(code(StoreError::Io("x".into())), "PARLA_ERR:downloadFailed");
    }

    #[test]
    fn cancelled_message_matches_the_old_one() {
        // src/lib/translateError.ts looks for exactly this.
        assert_eq!(CANCELLED, "PARLA_ERR:cancelled");
    }

    #[test]
    fn payloads_keep_their_old_shape() {
        let p = Flavor::Whisper.progress_payload("a", 1, 2);
        assert_eq!(p, json!({"id": "a", "downloaded": 1, "total": 2}));
        let p = Flavor::Parakeet.progress_payload("a", 1, 2);
        assert_eq!(p["current_file"], "");
        let p = Flavor::Vad.progress_payload("vad", 1, 2);
        assert_eq!(p, json!({"downloaded": 1, "total": 2}));
        assert_eq!(
            Flavor::Vad.complete_payload("vad", "/p"),
            json!({"path": "/p"})
        );
        assert_eq!(
            Flavor::Llm.complete_payload("a", "/p"),
            json!({"id": "a", "path": "/p"})
        );
        assert!(Flavor::Vad.events().2.is_none());
    }
}
