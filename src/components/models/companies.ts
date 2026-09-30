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
    family: "Parakeet",
    picks: [
      { label: "speech.pickBestEnglish", id: "parakeet-unified-en-0.6b-int8" },
      { label: "speech.pickManyLanguages", id: "parakeet-tdt-0.6b-v3-int8" },
    ],
  },
  {
    id: "openai",
    name: "OpenAI",
    family: "Whisper",
    picks: [
      { label: "speech.pickFastest", id: "ggml-base", en: "ggml-base.en" },
      { label: "speech.pickBalanced", id: "ggml-large-v3-turbo-q5_0" },
      { label: "speech.pickMostAccurate", id: "ggml-large-v3-q5_0" },
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
