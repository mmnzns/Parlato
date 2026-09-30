// OpenAI chat completions.
//
// Reference VoiceInk : AIService.swift (case .openAI) - baseURL
// https://api.openai.com/v1/chat/completions.

use anyhow::Result;
use async_trait::async_trait;

use crate::enhancement::provider::{EnhancementRequest, EnhancementResponse, LLMProvider};

use super::openai_compat;

pub struct OpenAIProvider;

// VoiceInk 2.13 AIService.availableModels (.openAI), commit af1c1bb
// "feat: add gpt-5.6 models".
// Parlato : GPT-6 ajoutes, gpt-4.1-mini / -nano retires (deprecies),
// d'apres developers.openai.com/api/docs/models (2026-09).
const MODELS: &[&str] = &[
    "gpt-6-luna",
    "gpt-6-sol",
    "gpt-6.1-sol",
    "gpt-6-astra",
    "gpt-5.6-luna",
    "gpt-5.6-terra",
    "gpt-5.6-sol",
    "gpt-5.5",
    "gpt-5.4",
    "gpt-5.4-mini",
    "gpt-5.4-nano",
    "gpt-4.1",
];

#[async_trait]
impl LLMProvider for OpenAIProvider {
    fn id(&self) -> &'static str {
        "openai"
    }
    fn label(&self) -> &'static str {
        "OpenAI"
    }
    fn default_models(&self) -> &'static [&'static str] {
        MODELS
    }
    fn default_model(&self) -> &'static str {
        "gpt-6-luna"
    }
    fn endpoint(&self) -> &'static str {
        "https://api.openai.com/v1/chat/completions"
    }

    async fn chat_completion(
        &self,
        api_key: &str,
        req: &EnhancementRequest,
    ) -> Result<EnhancementResponse> {
        openai_compat::chat_completion(self.endpoint(), api_key, req).await
    }
}
