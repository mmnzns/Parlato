// Mappe les codes d'erreur stables emis par le backend Rust vers des
// chaines i18n traduites cote frontend.
//
// Convention : le backend prefixe ses erreurs user-facing par `PARLA_ERR:`
// suivi d'un identifiant snake-or-camelCase et d'arguments optionnels
// separes par `:`. Exemple : `PARLA_ERR:apiKey:groq` -> i18n key
// `errors.apiKey` interpole avec `{ provider: "groq" }`.
//
// Toute erreur qui ne match pas un code stable est retournee telle quelle :
// les utilisateurs verront le message technique brut (anglais en general
// puisque les wrappers comme reqwest, anyhow, windows-rs sont anglais).
// C'est moins joli mais ca evite de masquer un probleme inattendu derriere
// un libelle generique.

import type { TFunction } from "i18next";

type ErrorMapping = {
  i18nKey: string;
  /** Builds interpolation args from the regex match (g[0] is the whole match,
   *  g[1] the first captured group). */
  args?: (groups: string[], t: TFunction) => Record<string, unknown>;
};

/** Parlato: a cancelled download or import (PARLA_ERR:cancelled) is not a
 *  real error. The French wording is the older backend message. */
export function isCancelled(e: unknown): boolean {
  const s = String(e);
  return s.includes("PARLA_ERR:cancelled") || s.includes("annule");
}

const ERROR_MAP: Array<[RegExp, ErrorMapping]> = [
  [/^PARLA_ERR:noWhisper$/, { i18nKey: "errors.noWhisper" }],
  [/^PARLA_ERR:interrupted$/, { i18nKey: "errors.interrupted" }],
  [/^PARLA_ERR:pasteFailed$/, { i18nKey: "errors.pasteFailed" }],
  [/^PARLA_ERR:apiKeyRejected$/, { i18nKey: "errors.apiKeyRejected" }],
  [/^PARLA_ERR:aiNotConfigured$/, { i18nKey: "errors.aiNotConfigured" }],
  [/^PARLA_ERR:fileUnsupported$/, { i18nKey: "errors.fileUnsupported" }],
  [/^PARLA_ERR:fileNoAudio$/, { i18nKey: "errors.fileNoAudio" }],
  [/^PARLA_ERR:fileTooLong$/, { i18nKey: "errors.fileTooLong" }],
  [/^PARLA_ERR:noParakeet$/, { i18nKey: "errors.noParakeet" }],
  // Model downloads (shared model folder, models/download.rs).
  [/^PARLA_ERR:downloadOffline$/, { i18nKey: "errors.downloadOffline" }],
  [/^PARLA_ERR:downloadDiskFull$/, { i18nKey: "errors.downloadDiskFull" }],
  [/^PARLA_ERR:downloadChecksum$/, { i18nKey: "errors.downloadChecksum" }],
  [/^PARLA_ERR:downloadStalled$/, { i18nKey: "errors.downloadStalled" }],
  [/^PARLA_ERR:downloadFailed$/, { i18nKey: "errors.downloadFailed" }],
  [/^PARLA_ERR:downloadRunning$/, { i18nKey: "errors.downloadRunning" }],
  [/^PARLA_ERR:downloadingElsewhere$/, { i18nKey: "errors.downloadingElsewhere" }],
  [
    /^PARLA_ERR:downloadRefused:(\d+)$/,
    {
      i18nKey: "errors.downloadRefused",
      args: (g) => ({ status: g[1] }),
    },
  ],
  [
    /^PARLA_ERR:modelNotDownloaded:(.+)$/,
    {
      i18nKey: "errors.modelNotDownloaded",
      args: (g) => ({ model: g[1] }),
    },
  ],
  [
    /^PARLA_ERR:parakeetIncomplete:(.+)$/,
    {
      i18nKey: "errors.parakeetIncomplete",
      args: (g) => ({ model: g[1] }),
    },
  ],
  [
    /^PARLA_ERR:apiKey:(.+)$/,
    {
      i18nKey: "errors.apiKeyMissing",
      args: (g) => ({ provider: g[1] }),
    },
  ],
  [/^PARLA_ERR:cloudUnconfigured$/, { i18nKey: "errors.cloudUnconfigured" }],
  [/^PARLA_ERR:providerTimeout$/, { i18nKey: "errors.providerTimeout" }],
  [/^PARLA_ERR:providerNetwork$/, { i18nKey: "errors.providerNetwork" }],
  [
    /^PARLA_ERR:providerHttp:(\d+)$/,
    {
      i18nKey: "errors.providerHttp",
      args: (g) => ({ status: g[1] }),
    },
  ],
  [/^PARLA_ERR:transcriptionFailed$/, { i18nKey: "errors.transcriptionFailed" }],
  [/^PARLA_ERR:cliNotFound$/, { i18nKey: "errors.cliNotFound" }],
  [
    /^PARLA_ERR:cliFailed:(-?\d+)$/,
    {
      i18nKey: "errors.cliFailed",
      args: (g) => ({ code: g[1] }),
    },
  ],
  [/^PARLA_ERR:cliEmpty$/, { i18nKey: "errors.cliEmpty" }],
  // Raccourcis (commands/hotkey.rs validate). Les noms d'action sont
  // traduits via hotkey.additional.actionNames.*.
  [
    /^PARLA_ERR:shortcutConflict:([^:]+):([^:]+)$/,
    {
      i18nKey: "errors.shortcutConflict",
      args: (g, t) => ({
        a: t(`hotkey.additional.actionNames.${g[1]}`),
        b: t(`hotkey.additional.actionNames.${g[2]}`),
      }),
    },
  ],
  [
    /^PARLA_ERR:shortcutReserved:(.+)$/,
    {
      i18nKey: "errors.shortcutReserved",
      args: (g, t) => ({ action: t(`hotkey.additional.actionNames.${g[1]}`) }),
    },
  ],
  [
    /^PARLA_ERR:shortcutNeedsModifier:(.+)$/,
    {
      i18nKey: "errors.shortcutNeedsModifier",
      args: (g, t) => ({ action: t(`hotkey.additional.actionNames.${g[1]}`) }),
    },
  ],
  [/^PARLA_ERR:historyEmpty$/, { i18nKey: "errors.historyEmpty" }],
  [/^PARLA_ERR:lastTranscriptionEmpty$/, { i18nKey: "errors.lastTranscriptionEmpty" }],
  [/^PARLA_ERR:audioMissing$/, { i18nKey: "errors.audioMissing" }],
  [/^PARLA_ERR:recordingInProgress$/, { i18nKey: "errors.recordingInProgress" }],
  [/^PARLA_ERR:assemblyaiRealtimeOnly$/, { i18nKey: "errors.assemblyaiRealtimeOnly" }],
];

/// Traduit un message d'erreur backend en chaine localisee. Si le code
/// n'est pas reconnu, retourne le message d'origine sans interpretation.
export function translateError(t: TFunction, raw: string | null | undefined): string {
  if (!raw) return "";
  for (const [pattern, mapping] of ERROR_MAP) {
    const match = pattern.exec(raw);
    if (!match) continue;
    const args = mapping.args ? mapping.args(match, t) : undefined;
    return t(mapping.i18nKey, args);
  }
  return raw;
}
