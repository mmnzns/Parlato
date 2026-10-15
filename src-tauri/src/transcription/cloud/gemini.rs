// Google Gemini en batch : modele de transcription dedie
// gemini-3.5-transcribe (GA 2026-08-26) via l'API Interactions.
//
// Reference VoiceInk : LLMkit GeminiTranscriptionClient.swift (ancienne
// version generateContent + prompt). Parlato, d'apres
// ai.google.dev/gemini-api/docs/transcribe et /api/interactions-api
// (2026-10-15) : les modeles 2.5 ne sont plus servis qu'aux comptes qui les
// utilisaient deja, d'ou le passage au modele dedie.
// Endpoint : POST https://generativelanguage.googleapis.com/v1beta/interactions
// Headers : x-goog-api-key: <key>, Content-Type: application/json
// Body JSON :
//   { model: "gemini-3.5-transcribe",
//     input: [ { type: "audio", data: "<base64>", mime_type: "audio/wav" } ],
//     generation_config: { transcription_config: {
//       custom_vocabulary: [ ... ] } } }   (si vocabulaire)
// Reponse : { id, status, steps: [ { type: "model_output",
//   content: [ { type: "text", text } ] } ] }.
// Si status != completed (queued / in_progress), on relit
// GET /v1beta/{id} jusqu'a la fin.
// Pas de langue envoyee : la doc attend des locales BCP-47 avec region
// ("fr-FR"), Parla n'a que le code ISO ; le modele detecte la langue.

use std::path::Path;
use std::time::Duration;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use serde::Deserialize;
use serde_json::json;

use super::http::{batch_client, map_http_err};
use super::provider::{CloudTranscriptionProvider, TranscribeRequest};

pub struct GeminiProvider;

const API_BASE: &str = "https://generativelanguage.googleapis.com/v1beta";

/// La doc conseille au plus ~100 termes pour de bons resultats (1000 max).
const VOCABULARY_LIMIT: usize = 100;

/// Relectures d'une interaction pas encore terminee (1 s d'intervalle).
const MAX_POLLS: u32 = 120;

#[derive(Debug, Deserialize)]
struct Interaction {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    steps: Vec<Step>,
}
#[derive(Debug, Deserialize)]
struct Step {
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    content: Vec<ContentItem>,
}
#[derive(Debug, Deserialize)]
struct ContentItem {
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    text: Option<String>,
}

#[async_trait]
impl CloudTranscriptionProvider for GeminiProvider {
    fn id(&self) -> &'static str {
        "gemini"
    }

    async fn verify_api_key(&self, api_key: &str) -> Result<()> {
        let client = batch_client()?;
        let resp = client
            .get(format!("{API_BASE}/models"))
            .header("x-goog-api-key", api_key)
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
        let body = request_body(
            &request.model,
            &B64.encode(&audio_bytes),
            &request.custom_vocabulary,
        );

        let client = batch_client()?;
        let resp = client
            .post(format!("{API_BASE}/interactions"))
            .header("x-goog-api-key", api_key)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(map_http_err)?;
        let mut interaction = read_interaction(resp).await?;

        let mut polls = 0;
        while is_pending(&interaction) {
            let Some(id) = interaction.id.clone() else {
                anyhow::bail!("interaction Gemini sans id");
            };
            if polls >= MAX_POLLS {
                anyhow::bail!("Gemini : transcription toujours en cours apres {MAX_POLLS} s");
            }
            polls += 1;
            tokio::time::sleep(Duration::from_secs(1)).await;
            let resp = client
                .get(format!("{API_BASE}/{id}"))
                .header("x-goog-api-key", api_key)
                .send()
                .await
                .map_err(map_http_err)?;
            interaction = read_interaction(resp).await?;
        }

        match interaction.status.as_deref() {
            None | Some("completed") => {}
            Some(other) => anyhow::bail!("Gemini : interaction {other}"),
        }
        transcript_text(&interaction).ok_or_else(|| anyhow!("reponse Gemini sans texte"))
    }
}

fn request_body(model: &str, audio_b64: &str, vocabulary: &[String]) -> serde_json::Value {
    let mut body = json!({
        "model": model,
        "input": [ { "type": "audio", "data": audio_b64, "mime_type": "audio/wav" } ]
    });
    let terms: Vec<&str> = vocabulary
        .iter()
        .map(|t| t.trim())
        .filter(|t| !t.is_empty())
        .take(VOCABULARY_LIMIT)
        .collect();
    if !terms.is_empty() {
        body["generation_config"] = json!({
            "transcription_config": { "custom_vocabulary": terms }
        });
    }
    body
}

async fn read_interaction(resp: reqwest::Response) -> Result<Interaction> {
    let status = resp.status();
    let body = resp.bytes().await?;
    if !status.is_success() {
        anyhow::bail!("HTTP {status}: {}", String::from_utf8_lossy(&body));
    }
    serde_json::from_slice(&body).map_err(|e| anyhow!("parse JSON Gemini: {e}"))
}

fn is_pending(interaction: &Interaction) -> bool {
    matches!(
        interaction.status.as_deref(),
        Some("queued" | "in_progress")
    )
}

/// Texte des etapes `model_output`, morceaux `text` mis bout a bout.
fn transcript_text(interaction: &Interaction) -> Option<String> {
    let text: String = interaction
        .steps
        .iter()
        .filter(|s| s.kind == "model_output")
        .flat_map(|s| s.content.iter())
        .filter(|c| c.kind == "text")
        .filter_map(|c| c.text.as_deref())
        .collect();
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_has_inline_audio_and_optional_vocabulary() {
        let plain = request_body("gemini-3.5-transcribe", "QUJD", &[]);
        assert_eq!(plain["model"], "gemini-3.5-transcribe");
        assert_eq!(plain["input"][0]["type"], "audio");
        assert_eq!(plain["input"][0]["data"], "QUJD");
        assert_eq!(plain["input"][0]["mime_type"], "audio/wav");
        assert!(plain.get("generation_config").is_none());

        let mut vocab: Vec<String> = vec![" Parlato ".into(), "".into()];
        vocab.extend((0..150).map(|i| format!("t{i}")));
        let with = request_body("gemini-3.5-transcribe", "QUJD", &vocab);
        let terms = with["generation_config"]["transcription_config"]["custom_vocabulary"]
            .as_array()
            .unwrap();
        assert_eq!(terms.len(), VOCABULARY_LIMIT);
        assert_eq!(terms[0], "Parlato");
    }

    #[test]
    fn transcript_reads_model_output_text() {
        let i: Interaction = serde_json::from_str(
            r#"{"id":"interactions/abc","status":"completed","steps":[
                {"id":"s0","type":"user_input","content":[{"type":"text","text":"ignore"}]},
                {"id":"s1","type":"model_output","content":[
                    {"type":"text","text":" Hello world ","annotations":[]}]}]}"#,
        )
        .unwrap();
        assert!(!is_pending(&i));
        assert_eq!(transcript_text(&i).as_deref(), Some("Hello world"));
    }

    #[test]
    fn pending_and_empty_interactions() {
        let i: Interaction =
            serde_json::from_str(r#"{"id":"interactions/abc","status":"in_progress"}"#).unwrap();
        assert!(is_pending(&i));
        assert_eq!(transcript_text(&i), None);
    }
}
