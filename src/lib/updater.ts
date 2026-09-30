// Parlato : installation d'une mise a jour, partagee par le bandeau
// (UpdateChecker) et le bouton "Mettre a jour" des reglages.

import type { Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

/**
 * Telecharge et installe la mise a jour puis redemarre Parlato.
 * `onProgress` recoit le pourcentage (null tant que la taille est inconnue).
 */
export async function installUpdate(
  update: Update,
  onProgress: (percent: number | null) => void,
  onInstalled?: () => void,
): Promise<void> {
  let downloaded = 0;
  let total = 0;
  onProgress(null);
  await update.downloadAndInstall((event) => {
    if (event.event === "Started") {
      total = event.data.contentLength ?? 0;
      onProgress(total > 0 ? 0 : null);
    } else if (event.event === "Progress") {
      downloaded += event.data.chunkLength;
      onProgress(total > 0 ? Math.round((downloaded / total) * 100) : null);
    } else if (event.event === "Finished") {
      onInstalled?.();
    }
  });
  await relaunch();
}

/** Date de publication lisible dans la langue de l'app ("30 sept. 2026"). */
export function formatReleaseDate(date: string | undefined, language: string): string | null {
  if (!date) return null;
  const d = new Date(date);
  if (Number.isNaN(d.getTime())) return null;
  return d.toLocaleDateString(language, { year: "numeric", month: "short", day: "numeric" });
}
