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

/** Where to create an account key for each online AI service. The provider
 *  data from the backend carries no URL (the speech catalog has one, this
 *  list does not), so the pages are listed here. */
export const LLM_KEY_URLS: Record<string, string> = {
  anthropic: "https://console.anthropic.com/settings/keys",
  openai: "https://platform.openai.com/api-keys",
  gemini: "https://aistudio.google.com/apikey",
  mistral: "https://console.mistral.ai/api-keys",
  groq: "https://console.groq.com/keys",
  cerebras: "https://cloud.cerebras.ai/",
  openrouter: "https://openrouter.ai/keys",
};
