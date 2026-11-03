// Parlato: "Remove from this computer" on an installed model, next to Delete.
// Delete lets go for Parlato only; this erases the model for every app that
// shares the folder (Parlato, Talaan, Ensayo), so it asks first and names the
// other apps. The Rust side (remove_model_everywhere) unloads the model here,
// erases it, and puts it back when another app has it open.

import type { TFunction } from "i18next";
import i18n from "@/i18n";
import { confirmDelete } from "@/lib/confirmDelete";
import { formatBytes } from "@/lib/format";
import { api, type RemoveResult } from "@/lib/tauri";

export type RemoveOutcome = RemoveResult | "cancelled";

/** "Talaan and Ensayo", written the way the app's language joins a list. */
function appList(apps: string[]): string {
  try {
    // Intl.ListFormat is missing from this project's TypeScript lib.
    const ListFormat = (Intl as unknown as { ListFormat: new (l: string, o: object) => { format(a: string[]): string } })
      .ListFormat;
    return new ListFormat(i18n.resolvedLanguage ?? "en", { style: "long", type: "conjunction" }).format(apps);
  } catch {
    return apps.join(", ");
  }
}

/** Confirms, then removes. `bytes` is what the model frees. */
export async function removeFromComputer(
  t: TFunction,
  model: { id: string; name: string; bytes: number | null | undefined; otherApps?: string[] },
): Promise<RemoveOutcome> {
  const size = formatBytes(model.bytes);
  const apps = model.otherApps ?? [];
  const body = apps.length
    ? t("removeModel.bodyShared", { apps: appList(apps), size })
    : t("removeModel.bodyAlone", { size });
  const ok = await confirmDelete(
    body,
    t("removeModel.confirmButton"),
    t("removeModel.confirmTitle", { name: model.name }),
  );
  if (!ok) return "cancelled";
  return api.removeModelEverywhere(model.id);
}

/** The line to show under the model after a removal; null when there is none to show. */
export function removeNote(t: TFunction, outcome: RemoveOutcome, name: string): { text: string; failure: boolean } | null {
  if (outcome === "in_use") return { text: t("removeModel.inUse", { name }), failure: true };
  if (outcome === "old_copies_left") return { text: t("removeModel.oldCopies"), failure: false };
  return null;
}
