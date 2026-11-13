// OpenRouter speech-to-text (batch), through its own transcription endpoint.
//
// Docs: https://openrouter.ai/docs/guides/overview/multimodal/stt
// Endpoint : POST https://openrouter.ai/api/v1/audio/transcriptions
// Headers : Authorization: Bearer <apiKey>
// Body multipart (OpenAI compatible) : file, model, language?, temperature,
// response_format=json. `prompt` is accepted but ignored by OpenRouter, so
// the custom vocabulary is not sent. Limits: 25 MB per file, 60 s upstream
// timeout per request. Models are listed by
// GET /api/v1/models?output_modalities=transcription (ids like
// "mistralai/voxtral-mini-transcribe-3-0").
// The key is the same "openrouter" key AI cleanup uses.

use std::path::Path;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use reqwest::multipart::Form;
use serde::Deserialize;

use super::http::{batch_client, map_http_err, wav_part_from_path};
use super::provider::{CloudTranscriptionProvider, TranscribeRequest};

pub struct OpenRouterProvider;

#[derive(Debug, Deserialize)]
struct OpenRouterResponse {
    text: Option<String>,
}

#[async_trait]
impl CloudTranscriptionProvider for OpenRouterProvider {
    fn id(&self) -> &'static str {
        "openrouter"
    }

    async fn verify_api_key(&self, api_key: &str) -> Result<()> {
        let client = batch_client()?;
        let resp = client
            .get("https://openrouter.ai/api/v1/key")
            .bearer_auth(api_key)
            .send()
            .await
            .map_err(map_http_err)?;
        if !resp.status().is_success() {
            anyhow::bail!("HTTP {} (cle API invalide ?)", resp.status());
        }
        Ok(())
    }

    async fn transcribe(
        &self,
        wav_path: &Path,
        api_key: &str,
        request: &TranscribeRequest,
    ) -> Result<String> {
        let mut form = Form::new()
            .part("file", wav_part_from_path(wav_path).await?)
            .text("model", request.model.clone())
            .text("response_format", "json")
            .text("temperature", "0");
        if let Some(lang) = request.language.as_deref() {
            if !lang.is_empty() && lang != "auto" {
                form = form.text("language", lang.to_string());
            }
        }

        let client = batch_client()?;
        let resp = client
            .post("https://openrouter.ai/api/v1/audio/transcriptions")
            .bearer_auth(api_key)
            .multipart(form)
            .send()
            .await
            .map_err(map_http_err)?;

        let status = resp.status();
        let body = resp.bytes().await?;
        if !status.is_success() {
            let msg = String::from_utf8_lossy(&body);
            anyhow::bail!("HTTP {status}: {msg}");
        }
        serde_json::from_slice::<OpenRouterResponse>(&body)
            .map_err(|_| anyhow!("reponse illisible"))?
            .text
            .ok_or_else(|| anyhow!("reponse sans champ text"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_id_matches_the_catalog_and_the_key_name() {
        assert_eq!(OpenRouterProvider.id(), "openrouter");
        assert!(super::super::catalog::find_provider("openrouter").is_some());
    }

    #[test]
    fn response_text_is_read() {
        let r: OpenRouterResponse =
            serde_json::from_str(r#"{"text":"hello","usage":{"seconds":1.2}}"#).unwrap();
        assert_eq!(r.text.as_deref(), Some("hello"));
    }
}
