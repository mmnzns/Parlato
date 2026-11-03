// Parlato: shared constants for the AI cleanup page.

/** Providers that run on this PC. Everything else is an online service. */
export const LOCAL_IDS = ["llamacpp", "ollama", "localcli"];
// Parlato: services that also accept a model ID typed by hand, for models
// released after this version (VoiceInk 2.21 supportsCustomModelID).
export const TYPED_MODEL_IDS = [
  "anthropic",
  "openai",
  "gemini",
  "mistral",
  "groq",
  "cerebras",
  "openrouter",
];
export const OTHER_MODEL = "__other__";

export const inputClass = "h-[34px] w-full rounded-sm border-[1.5px] border-input bg-background px-3 text-sm";
