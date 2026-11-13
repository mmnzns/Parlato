// Parlato: the "delete everywhere" half of the one Delete button. Delete from
// Parlato only lets go for this app; this erases the model for every app that
// shares the folder (Parlato, Talaan, Ensayo). The Rust side
// (remove_model_everywhere) unloads the model here, erases it, and puts it
// back when another app has it open. The question is the DeleteModelDialog.

import type { TFunction } from "i18next";
import i18n from "@/i18n";
import { api, type RemoveResult } from "@/lib/tauri";

/** "Talaan and Ensayo", written the way the app's language joins a list. */
export function appList(apps: string[]): string {
  try {
    // Intl.ListFormat is missing from this project's TypeScript lib.
    const ListFormat = (Intl as unknown as { ListFormat: new (l: string, o: object) => { format(a: string[]): string } })
      .ListFormat;
    return new ListFormat(i18n.resolvedLanguage ?? "en", { style: "long", type: "conjunction" }).format(apps);
  } catch {
    return apps.join(", ");
  }
}

/** Erases the model for every app that shares the folder. The question was
 *  already asked (components/DeleteModelDialog.tsx). */
export function removeEverywhere(id: string): Promise<RemoveResult> {
  return api.removeModelEverywhere(id);
}

/** The line to show under the model after a removal; null when there is none to show. */
export function removeNote(t: TFunction, outcome: RemoveResult, name: string): { text: string; failure: boolean } | null {
  if (outcome === "in_use") return { text: t("removeModel.inUse", { name }), failure: true };
  if (outcome === "old_copies_left") return { text: t("removeModel.oldCopies"), failure: false };
  return null;
}
