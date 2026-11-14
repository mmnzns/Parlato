// Parlato: the Speech model page is organised by company (the developer for
// models that run on this PC, the service you sign up with for online ones).
// See docs/superpowers/specs/2026-09-29-speech-model-by-company-design.md.
//
// Local companies show a few plain "picks"; every other model of that
// company sits behind "Show all versions". Adding a model to a pick means
// one line here plus its en/fr/es text (speech.pick* and modelText.*).

import type { TranscriptionSource } from "@/lib/tauri";

export type LocalCompanyId = "nvidia" | "openai";

/// A plain choice inside a local company. `en` is the English-only version
/// used instead of `id` when the dictation language is English.
export type ModelPick = { label: string; id: string; en?: string };

export const LOCAL_COMPANIES: {
  id: LocalCompanyId;
  name: string;
  family: string;
  picks: ModelPick[];
}[] = [
  {
    id: "nvidia",
    name: "NVIDIA",
    family: "Parakeet, Nemotron",
    picks: [
      { label: "speech.pickBestEnglish", id: "parakeet-unified-en" },
      { label: "speech.pickManyLanguages", id: "parakeet-ultra" },
      { label: "speech.pickMostLanguages", id: "nemotron-3.5" },
    ],
  },
  {
    id: "openai",
    name: "OpenAI",
    family: "Whisper",
    picks: [
      { label: "speech.pickFastest", id: "whisper-base", en: "whisper-base-en" },
      { label: "speech.pickBalanced", id: "whisper-turbo" },
      { label: "speech.pickMostAccurate", id: "whisper-large-v3" },
    ],
  },
];

/// Online services that run another company's model: provider id -> note key.
export const RUNS_NOTE: Record<string, string> = {
  groq: "speech.runsWhisper",
};

/// Resolves a pick to the model id to show for the dictation language.
export function pickModelId(p: ModelPick, language: string): string {
  return language === "en" && p.en ? p.en : p.id;
}

/// Company id ("nvidia", "openai" or a cloud provider id) of the active
/// source, or null when nothing is set up yet.
export function companyOfSource(
  source: TranscriptionSource | null,
  hasActiveModel: boolean,
): string | null {
  if (!source || !hasActiveModel) return null;
  if (source.kind === "parakeet") return "nvidia";
  if (source.kind === "local") return "openai";
  if (source.kind === "cloud") return source.cloud_provider ?? null;
  return null;
}

/// Parlato: the model Parlato tags "Recommended" for the language the person
/// speaks. English: Parakeet Unified (accurate, with punctuation). Another
/// language Parakeet Ultra covers (the European ones, and "detect it for me"):
/// Parakeet Ultra. Any other language: Whisper Turbo, the best Whisper
/// balance. `ultraCodes` is Parakeet Ultra's language list from the catalog.
export const RECOMMENDED_ENGLISH = "parakeet-unified-en";
export const RECOMMENDED_EUROPEAN = "parakeet-ultra";
export const RECOMMENDED_OTHER = "whisper-turbo";

export function recommendedSpeechModel(language: string, ultraCodes?: string[]): string {
  const code = language.split("_")[0];
  if (code === "en") return RECOMMENDED_ENGLISH;
  if (code === "auto") return RECOMMENDED_EUROPEAN;
  return (ultraCodes ?? []).includes(code) ? RECOMMENDED_EUROPEAN : RECOMMENDED_OTHER;
}

/// Local company whose list holds a model id, or null.
export function companyOfModel(id: string): LocalCompanyId | null {
  for (const c of LOCAL_COMPANIES) {
    if (c.picks.some((p) => p.id === id || p.en === id)) return c.id;
  }
  return null;
}

/// Local AI model Parlato tags "Recommended" in AI cleanup (IBM Granite 4.2 3B;
/// the engine's `RECOMMENDED_LOCAL_AI`).
export const RECOMMENDED_LOCAL_AI = "granite-4.2-3b";
