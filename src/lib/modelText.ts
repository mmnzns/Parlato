// Parlato: localized model names and descriptions.
//
// The Rust catalogs (transcription/model.rs, parakeet_model_manager.rs,
// cloud/catalog.rs, enhancement/model_manager.rs) carry upstream's French
// display names and notes. The UI shows the `modelText.<id>` locale entry
// instead, and falls back to the backend string for anything not listed
// (new upstream models, user imports).

import type { TFunction } from "i18next";

/// Backend notes attached to user-imported models (whisper / llama.cpp).
const IMPORTED_NOTES = [
  "Modele Whisper GGML importe par l'utilisateur",
  "Modele GGUF importe par l'utilisateur",
];

/// i18next splits keys on "." : model ids like "ggml-tiny.en" become
/// "ggml_tiny_en".
function key(id: string): string {
  return id.replace(/[^a-zA-Z0-9]/g, "_");
}

export function modelName(t: TFunction, id: string, fallback: string): string {
  return t(`modelText.${key(id)}.name`, { defaultValue: fallback });
}

export function modelNotes(t: TFunction, id: string, fallback: string | null | undefined): string {
  if (fallback && IMPORTED_NOTES.includes(fallback)) return t("modelText.imported");
  return t(`modelText.${key(id)}.notes`, { defaultValue: fallback ?? "" });
}
