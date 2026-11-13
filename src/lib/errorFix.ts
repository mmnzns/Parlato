// Parlato: where "Fix it" in the recorder pill takes the person, by the stable
// error code (PARLA_ERR:<code>) that the pill is showing. null = nothing to
// fix there (a network hiccup, a file problem), so no button is shown.

import type { View } from "@/components/Sidebar";
import { isMac } from "@/lib/platform";

const RULES: Array<[RegExp, View | (() => View | null)]> = [
  // No speech model, or the online service is not usable: Speech model.
  [/^PARLA_ERR:(noWhisper|noParakeet|modelNotDownloaded|parakeetIncomplete|apiKey|cloudUnconfigured)\b/, "models"],
  // The AI cleanup provider rejected the key, or no model is chosen: AI cleanup.
  [/^PARLA_ERR:(aiNotConfigured|apiKeyRejected)\b/, "enhancement"],
  // Microphone permission, and Accessibility (the shortcut and paste): Permissions.
  [/^PARLA_ERR:(micPermission|micDenied|micAccess|accessibility\w*)\b/, "permissions"],
  // The microphone went away or cannot be opened: Microphone & shortcut.
  [/^PARLA_ERR:mic\w*/, "audio"],
  // On a Mac a failed paste is almost always the Accessibility permission.
  [/^PARLA_ERR:pasteFailed$/, () => (isMac ? "permissions" : null)],
];

export function fixViewFor(raw: string | null | undefined): View | null {
  if (!raw) return null;
  for (const [pattern, target] of RULES) {
    if (pattern.test(raw)) return typeof target === "function" ? target() : target;
  }
  return null;
}
