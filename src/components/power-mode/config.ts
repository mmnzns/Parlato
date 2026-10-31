// Parlato: shared types and helpers for the power modes page.

import type { CloudModel } from "@/components/models/types";
import type {
  CustomPrompt,
  LLMProviderInfo,
  ParakeetModelState,
  PowerModeConfig,
  WhisperModelState,
} from "@/lib/tauri";

export const inputClass = "h-[34px] min-w-0 rounded-sm border-[1.5px] border-input bg-background px-3 text-sm";

// Alt+1 .. Alt+9, then Alt+0 (the hook's digit order).
export const SHORTCUT_SLOTS = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9];

export function emptyConfig(name: string): PowerModeConfig {
  return {
    id: "",
    name,
    emoji: "*",
    app_triggers: [],
    url_triggers: [],
    is_enhancement_enabled: false,
    use_screen_capture: null,
    selected_prompt_id: null,
    selected_llm_provider: null,
    selected_llm_model: null,
    transcription_kind: null,
    whisper_model_id: null,
    cloud_provider: null,
    cloud_model: null,
    parakeet_model_id: null,
    language: null,
    auto_send_key: "none",
    shortcut_slot: null,
    is_enabled: true,
    is_default: false,
  };
}

export type Catalog = {
  prompts: CustomPrompt[];
  providers: LLMProviderInfo[];
  whisper: WhisperModelState[];
  parakeet: ParakeetModelState[];
  cloud: CloudModel[];
};

// VoiceInk 2.21 nextDuplicateName: "Email" -> "Email 1", "Email 1" -> "Email 2".
export function duplicateName(name: string, taken: string[]): string {
  const names = new Set(taken);
  let base = name;
  const m = name.match(/^(.*) (\d+)$/);
  if (m && Number(m[2]) > 0 && names.has(m[1])) base = m[1];
  let n = 1;
  while (names.has(`${base} ${n}`)) n += 1;
  return `${base} ${n}`;
}
