// Client HTTP partage pour tous les providers "OpenAI chat/completions".
//
// Reference VoiceInk : LLMkit/OpenAILLMClient.swift.
// Shape requete :
//   POST {endpoint}
//   Header : Authorization: Bearer {api_key}
//   Body   : {
//     "model": ...,
//     "messages": [
//       {"role":"system","content":system},
//       {"role":"user","content":user}
//     ],
//     "temperature": f32,
//     ["reasoning_effort": "none"|"minimal"|"low"],
//     [...extra_body merge]
//   }
// Reponse : choices[0].message.content.
//
// Providers compatibles : OpenAI, Gemini (shim /v1beta/openai),
// Mistral, Groq, Cerebras, OpenRouter, Custom.

use anyhow::{anyhow, Result};
use serde_json::{json, Value};

use crate::enhancement::provider::{EnhancementRequest, EnhancementResponse};

/// Appel chat completion OpenAI-compatible.
pub async fn chat_completion(
    endpoint: &str,
    api_key: &str,
    req: &EnhancementRequest,
) -> Result<EnhancementResponse> {
    let mut body = serde_json::Map::new();
    body.insert("model".into(), json!(req.model));
    body.insert(
        "messages".into(),
        json!([
            {"role": "system", "content": req.system_prompt},
            {"role": "user", "content": req.user_message},
        ]),
    );
    if let Some(temperature) = req.temperature {
        body.insert("temperature".into(), json!(temperature));
    }
    if let Some(effort) = req.reasoning.effort.as_ref() {
        body.insert("reasoning_effort".into(), json!(effort));
    }
    if let Some(extra) = req.reasoning.extra_body.as_ref() {
        for (k, v) in extra {
            body.insert(k.clone(), v.clone());
        }
    }

    let client = reqwest::Client::builder()
        .timeout(req.timeout)
        .build()
        .map_err(|e| anyhow!("http client: {e}"))?;

    let mut builder = client.post(endpoint).json(&Value::Object(body));
    if !api_key.is_empty() {
        builder = builder.bearer_auth(api_key);
    }

    let resp = builder.send().await.map_err(map_http_err)?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        let truncated: String = body.chars().take(500).collect();
        if status.as_u16() == 429 {
            return Err(anyhow!("rate_limit ({status}): {truncated}"));
        }
        if status.is_server_error() {
            return Err(anyhow!("server_error ({status}): {truncated}"));
        }
        return Err(anyhow!("http {status}: {truncated}"));
    }

    let json: Value = resp
        .json()
        .await
        .map_err(|e| anyhow!("json parse: {e}"))?;
    let content = json
        .pointer("/choices/0/message/content")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("reponse sans choices[0].message.content"))?;
    Ok(EnhancementResponse {
        text: content.to_string(),
    })
}

/// Parlato : appel de verification d'une cle (GET sans cout). Seul un
/// refus explicite (401 / 403) bloque l'enregistrement : hors ligne ou
/// endpoint inattendu, la cle est gardee et l'erreur viendra a l'usage.
pub async fn check_key(url: &str, headers: &[(&str, &str)]) -> Result<()> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| anyhow!("http client: {e}"))?;
    let mut builder = client.get(url);
    for (name, value) in headers {
        builder = builder.header(*name, *value);
    }
    match builder.send().await {
        Ok(resp) if matches!(resp.status().as_u16(), 401 | 403) => {
            Err(anyhow!("PARLA_ERR:apiKeyRejected"))
        }
        // Gemini repond 400 "API key not valid" a une mauvaise cle.
        Ok(resp) if resp.status().as_u16() == 400 => {
            let body = resp.text().await.unwrap_or_default().to_lowercase();
            if body.contains("api key not valid") || body.contains("api_key_invalid") {
                Err(anyhow!("PARLA_ERR:apiKeyRejected"))
            } else {
                Ok(())
            }
        }
        Ok(resp) => {
            if !resp.status().is_success() {
                tracing::warn!(status = %resp.status(), "verification de cle non concluante");
            }
            Ok(())
        }
        Err(e) => {
            tracing::warn!("verification de cle impossible: {e}");
            Ok(())
        }
    }
}

fn map_http_err(e: reqwest::Error) -> anyhow::Error {
    if e.is_timeout() {
        return anyhow!("timeout: {e}");
    }
    if e.is_connect() {
        return anyhow!("network_error: {e}");
    }
    anyhow!("http: {e}")
}
