// Cerebras chat completions.
//
// Reference VoiceInk : AIService.swift (case .cerebras) - baseURL
// https://api.cerebras.ai/v1/chat/completions.

use anyhow::Result;
use async_trait::async_trait;

use crate::enhancement::provider::{EnhancementRequest, EnhancementResponse, LLMProvider};

use super::openai_compat;

pub struct CerebrasProvider;

// VoiceInk 2.13 AIService.availableModels (.cerebras).
// Parlato : gemma-4-31b et zai-glm-4.7 retires (plus sur l'inference
// partagee), qwen-3.8-27b ajoute, d'apres inference-docs.cerebras.ai (2026-09).
const MODELS: &[&str] = &[
    "gpt-oss-120b",
    "qwen-3.8-27b",
];

#[async_trait]
impl LLMProvider for CerebrasProvider {
    fn id(&self) -> &'static str {
        "cerebras"
    }
    fn label(&self) -> &'static str {
        "Cerebras"
    }
    fn default_models(&self) -> &'static [&'static str] {
        MODELS
    }
    fn default_model(&self) -> &'static str {
        "qwen-3.8-27b"
    }
    fn endpoint(&self) -> &'static str {
        "https://api.cerebras.ai/v1/chat/completions"
    }

    async fn chat_completion(
        &self,
        api_key: &str,
        req: &EnhancementRequest,
    ) -> Result<EnhancementResponse> {
        openai_compat::chat_completion(self.endpoint(), api_key, req).await
    }
}
