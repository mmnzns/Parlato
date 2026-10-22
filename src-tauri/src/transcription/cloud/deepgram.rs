// Deepgram nova-3 / nova-3-medical en batch.
//
// Reference VoiceInk : LLMkit DeepgramClient.swift.
// Endpoint : POST https://api.deepgram.com/v1/listen?model=...&smart_format=true
//   &punctuate=true&paragraphs=true&mip_opt_out=true[&language=...]
// mip_opt_out=true asks Deepgram not to use the audio for model improvement
// (LLMkit f35a17a).
// Headers : Authorization: Token <key>, Content-Type: audio/wav
// Body : raw audio bytes.
// Reponse : results.channels[0].alternatives[0].transcript.

use std::path::Path;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use serde::Deserialize;

use super::http::{batch_client, map_http_err};
use super::provider::{CloudTranscriptionProvider, TranscribeRequest};

pub struct DeepgramProvider;

/// Batch request URL. `mip_opt_out=true` keeps the audio out of Deepgram's
/// model improvement program (LLMkit DeepgramClient f35a17a).
fn listen_url(model: &str, language: Option<&str>, keyterms: &[String]) -> String {
    let mut url = format!(
        "https://api.deepgram.com/v1/listen?model={}&smart_format=true&punctuate=true&paragraphs=true&mip_opt_out=true",
        urlencoding::encode(model)
    );
    if let Some(lang) = language {
        if !lang.is_empty() && lang != "auto" {
            url.push_str(&format!("&language={}", urlencoding::encode(lang)));
        }
    }
    for term in keyterms {
        url.push_str(&format!("&keyterm={}", urlencoding::encode(term)));
    }
    url
}

#[derive(Debug, Deserialize)]
struct DeepgramResponse {
    results: Results,
}
#[derive(Debug, Deserialize)]
struct Results {
    channels: Vec<Channel>,
}
#[derive(Debug, Deserialize)]
struct Channel {
    alternatives: Vec<Alternative>,
}
#[derive(Debug, Deserialize)]
struct Alternative {
    transcript: String,
}

#[async_trait]
impl CloudTranscriptionProvider for DeepgramProvider {
    fn id(&self) -> &'static str {
        "deepgram"
    }

    async fn verify_api_key(&self, api_key: &str) -> Result<()> {
        let client = batch_client()?;
        let resp = client
            .get("https://api.deepgram.com/v1/projects")
            .header("Authorization", format!("Token {api_key}"))
            .send()
            .await
            .map_err(map_http_err)?;
        if !resp.status().is_success() {
            anyhow::bail!("HTTP {}", resp.status());
        }
        Ok(())
    }

    async fn transcribe(
        &self,
        wav_path: &Path,
        api_key: &str,
        request: &TranscribeRequest,
    ) -> Result<String> {
        let audio_bytes = tokio::fs::read(wav_path).await?;

        let url = listen_url(
            &request.model,
            request.language.as_deref(),
            &request.custom_vocabulary,
        );

        let client = batch_client()?;
        let resp = client
            .post(&url)
            .header("Authorization", format!("Token {api_key}"))
            .header("Content-Type", "audio/wav")
            .body(audio_bytes)
            .send()
            .await
            .map_err(map_http_err)?;

        let status = resp.status();
        let body = resp.bytes().await?;
        if !status.is_success() {
            anyhow::bail!("HTTP {status}: {}", String::from_utf8_lossy(&body));
        }
        let parsed: DeepgramResponse = serde_json::from_slice(&body)
            .map_err(|e| anyhow!("parse JSON Deepgram: {e}"))?;
        let transcript = parsed
            .results
            .channels
            .first()
            .and_then(|c| c.alternatives.first())
            .map(|a| a.transcript.clone())
            .unwrap_or_default();
        Ok(transcript)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batch_url_opts_out_of_training() {
        let url = listen_url("nova-3", None, &[]);
        assert!(url.contains("&mip_opt_out=true"));
        assert!(url.starts_with("https://api.deepgram.com/v1/listen?model=nova-3&"));
    }

    #[test]
    fn batch_url_keeps_language_and_keyterms() {
        let url = listen_url("nova-3", Some("fr"), &["Parlato".to_string()]);
        assert!(url.contains("&mip_opt_out=true"));
        assert!(url.ends_with("&language=fr&keyterm=Parlato"));
        assert!(!listen_url("nova-3", Some("auto"), &[]).contains("language"));
    }
}
