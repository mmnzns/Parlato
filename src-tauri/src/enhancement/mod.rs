// Module enhancement : orchestration de l'amelioration LLM du texte transcrit.
//
// Reference VoiceInk : VoiceInk/Services/AIEnhancement/ (AIEnhancementService.swift,
// AIService.swift) + VoiceInk/Models/AIPrompts.swift + PredefinedPrompts.swift.
//
// Structure :
//   - provider.rs  : trait LLMProvider + EnhancementRequest/Response
//   - registry.rs  : LLMRegistry (providers concrets)
//   - service.rs   : EnhancementService orchestrator (prompts, retry, filter)
//   - prompts.rs   : AIPrompts templates + CustomPrompt + persistance store
//   - output_filter.rs : strip <thinking>/<think>/<reasoning>
//   - providers/   : implementations concretes (openai, gemini, groq, ...)

pub mod output_filter;
pub mod prompt_detection;
pub mod prompts;
pub mod provider;
pub mod providers;
pub mod registry;
pub mod service;
