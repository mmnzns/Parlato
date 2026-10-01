// Deepgram streaming (nova-3, nova-3-medical).
//
// Reference VoiceInk : LLMkit DeepgramStreamingClient.swift.
// WSS : wss://api.deepgram.com/v1/listen?model=...&encoding=linear16
//       &sample_rate=16000&channels=1&smart_format=true&numerals=true
//       &interim_results=true[&language=...][&keyterm=...]
// Auth header : Authorization: Token <key>
// Audio : frames binaires LE PCM.
// Keepalive : toutes les 5 s, {"type":"KeepAlive"}
// Commit : {"type":"Finalize"}
// Close : {"type":"CloseStream"}
// Transcripts : channel.alternatives[0].transcript + is_final + speech_final.

use std::time::Duration;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tokio_tungstenite::tungstenite::{client::IntoClientRequest, Message};

use super::session::{
    connect_ws, drain_ws_messages, i16_to_le_bytes, StreamingChannels, StreamingConfig,
    StreamingEvent, StreamingProvider,
};

pub struct DeepgramStreaming;

#[async_trait]
impl StreamingProvider for DeepgramStreaming {
    fn id(&self) -> &'static str {
        "deepgram"
    }

    async fn run(
        &self,
        api_key: String,
        config: StreamingConfig,
        channels: StreamingChannels,
        on_event: Box<dyn Fn(StreamingEvent) + Send + Sync>,
    ) -> Result<String> {
        let mut url = format!(
            "wss://api.deepgram.com/v1/listen?model={}&encoding=linear16&sample_rate=16000&channels=1&smart_format=true&numerals=true&interim_results=true",
            urlencoding::encode(&config.model)
        );
        if let Some(lang) = config.language.as_deref() {
            if !lang.is_empty() && lang != "auto" {
                url.push_str(&format!("&language={}", urlencoding::encode(lang)));
            }
        }
        // Cap comme VoiceInk DeepgramStreamingProvider L107 : 50 keyterms max.
        for term in config.custom_vocabulary.iter().take(50) {
            url.push_str(&format!("&keyterm={}", urlencoding::encode(term)));
        }

        let mut req = url.into_client_request()?;
        req.headers_mut()
            .insert("Authorization", format!("Token {api_key}").parse()?);

        let ws_stream = connect_ws(req).await?;
        let (mut write, mut read) = ws_stream.split();

        on_event(StreamingEvent::SessionStarted);

        let StreamingChannels {
            mut audio_rx,
            mut finalize_rx,
        } = channels;

        let mut accumulated_final = String::new();
        let mut keepalive = tokio::time::interval(Duration::from_secs(5));
        keepalive.tick().await; // consomme le premier tick immediat

        loop {
            tokio::select! {
                biased;
                _ = &mut finalize_rx => {
                    // Drain tout audio restant avant de commit.
                    while let Ok(chunk) = audio_rx.try_recv() {
                        let _ = write.send(Message::Binary(i16_to_le_bytes(&chunk).into())).await;
                    }
                    let _ = write.send(Message::Text("{\"type\":\"Finalize\"}".into())).await;
                    // On attend les derniers transcripts quelques secondes.
                    let final_text = drain_remaining(&mut read, &mut accumulated_final, &on_event).await;
                    let _ = write.send(Message::Text("{\"type\":\"CloseStream\"}".into())).await;
                    let _ = write.close().await;
                    return Ok(final_text);
                }
                _ = keepalive.tick() => {
                    let _ = write.send(Message::Text("{\"type\":\"KeepAlive\"}".into())).await;
                }
                chunk = audio_rx.recv() => {
                    match chunk {
                        Some(c) => {
                            if let Err(e) = write.send(Message::Binary(i16_to_le_bytes(&c).into())).await {
                                on_event(StreamingEvent::Error { message: format!("ws send: {e}") });
                                return Err(anyhow!("ws send: {e}"));
                            }
                        }
                        None => {
                            // Plus de chunks : on se comporte comme si finalize.
                            let _ = write.send(Message::Text("{\"type\":\"Finalize\"}".into())).await;
                            let final_text = drain_remaining(&mut read, &mut accumulated_final, &on_event).await;
                            let _ = write.send(Message::Text("{\"type\":\"CloseStream\"}".into())).await;
                            let _ = write.close().await;
                            return Ok(final_text);
                        }
                    }
                }
                msg = read.next() => {
                    match msg {
                        Some(Ok(Message::Text(t))) => handle_text(&t, &mut accumulated_final, &on_event),
                        Some(Ok(Message::Binary(_))) => {}
                        Some(Ok(Message::Close(_))) => return Ok(accumulated_final),
                        Some(Ok(_)) => {}
                        Some(Err(e)) => {
                            on_event(StreamingEvent::Error { message: format!("ws read: {e}") });
                            return Err(anyhow!("ws read: {e}"));
                        }
                        None => return Ok(accumulated_final),
                    }
                }
            }
        }
    }
}

async fn drain_remaining<S>(
    read: &mut S,
    accumulated: &mut String,
    on_event: &(dyn Fn(StreamingEvent) + Send + Sync),
) -> String
where
    S: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    drain_ws_messages(read, Duration::from_secs(5), |t| {
        handle_text(t, accumulated, on_event)
    })
    .await;
    accumulated.trim().to_string()
}

fn handle_text(
    t: &str,
    accumulated: &mut String,
    on_event: &(dyn Fn(StreamingEvent) + Send + Sync),
) {
    let Ok(json) = serde_json::from_str::<Value>(t) else {
        return;
    };

    // Control / metadata : type == "Metadata" / "SpeechStarted" / "UtteranceEnd"
    if let Some("Metadata" | "SpeechStarted" | "UtteranceEnd") =
        json.get("type").and_then(|v| v.as_str())
    {
        return;
    }
    if let Some(err) = json.get("error").and_then(|v| v.as_str()) {
        on_event(StreamingEvent::Error {
            message: err.to_string(),
        });
        return;
    }

    let channel = json.get("channel");
    let Some(transcript) = channel
        .and_then(|c| c.get("alternatives"))
        .and_then(|a| a.as_array())
        .and_then(|a| a.first())
        .and_then(|alt| alt.get("transcript"))
        .and_then(|t| t.as_str())
    else {
        return;
    };

    if transcript.is_empty() {
        return;
    }

    let is_final = json
        .get("is_final")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let speech_final = json
        .get("speech_final")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if is_final || speech_final {
        if !accumulated.is_empty() {
            accumulated.push(' ');
        }
        accumulated.push_str(transcript);
        on_event(StreamingEvent::Committed {
            text: accumulated.clone(),
        });
    } else {
        let mut preview = accumulated.clone();
        if !preview.is_empty() {
            preview.push(' ');
        }
        preview.push_str(transcript);
        on_event(StreamingEvent::Partial { text: preview });
    }
}
