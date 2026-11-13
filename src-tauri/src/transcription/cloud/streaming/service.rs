// Service orchestrant une session streaming : spawn la task provider,
// retourne un handle utilisable par le recorder et le pipeline.

use anyhow::{anyhow, Result};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::sync::{mpsc, oneshot};
use tracing::warn;

use super::registry::StreamingRegistry;
use super::session::{StreamingChannels, StreamingConfig, StreamingEvent, StreamingHandle};

pub async fn start_streaming(
    app: AppHandle,
    registry: Arc<StreamingRegistry>,
    provider_id: String,
    api_key: String,
    config: StreamingConfig,
) -> Result<StreamingHandle> {
    let provider = registry
        .find(&provider_id)
        .ok_or_else(|| anyhow!("provider streaming inconnu: {provider_id}"))?;

    let (audio_tx, audio_rx) = mpsc::unbounded_channel::<Vec<i16>>();
    let (finalize_tx, finalize_rx) = oneshot::channel::<()>();
    let (done_tx, done_rx) = oneshot::channel::<Result<String>>();

    let channels = StreamingChannels {
        audio_rx,
        finalize_rx,
    };

    let on_event_app = app.clone();
    let on_event: Box<dyn Fn(StreamingEvent) + Send + Sync> = Box::new(move |e| {
        let _ = on_event_app.emit("streaming:event", e);
    });

    let task = tokio::spawn(async move {
        let result = provider.run(api_key, config, channels, on_event).await;
        if let Err(ref e) = result {
            // Parlato : jamais le texte brut du provider (corps de reponse,
            // URL, mots dictes) dans les logs, seulement le code court.
            warn!(code = %crate::transcription::error_code::storable_error_code(&e.to_string()), "Streaming session a echoue");
        }
        let _ = done_tx.send(result);
    });

    Ok(StreamingHandle::new(audio_tx, finalize_tx, done_rx).with_task(task.abort_handle()))
}
