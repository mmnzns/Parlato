// Parlato : installation d'une mise a jour, partagee par le bandeau
// (UpdateChecker) et le bouton "Mettre a jour" des reglages.

import { check, type Update } from "@tauri-apps/plugin-updater";
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

// -- Parlato : etat partage ---------------------------------------------------
//
// Le bandeau (UpdateChecker) et le bouton des reglages lisent le meme etat,
// donc une seule verification et un seul telechargement a la fois.

export type UpdateStatus =
  | "idle"
  | "checking"
  | "current"
  | "available"
  | "failed"
  | "downloading"
  | "installed"
  | "installFailed";

export type UpdaterState = {
  status: UpdateStatus;
  update: Update | null;
  percent: number | null;
  error: string | null;
};

let state: UpdaterState = { status: "idle", update: null, percent: null, error: null };
const listeners = new Set<() => void>();

function setState(patch: Partial<UpdaterState>) {
  state = { ...state, ...patch };
  listeners.forEach((l) => l());
}

export function subscribeUpdater(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export function getUpdaterState(): UpdaterState {
  return state;
}

/** `silent` (check at startup): a failure goes back to idle instead of
 *  showing "Check failed". Ignored while a download is running. */
export async function checkForUpdate(silent = false): Promise<void> {
  if (state.status === "checking" || state.status === "downloading") return;
  setState({ status: "checking", error: null });
  try {
    const u = await check();
    setState(u ? { status: "available", update: u } : { status: "current", update: null });
  } catch (e) {
    console.warn("updater check:", e);
    setState({ status: silent ? "idle" : "failed" });
  }
}

export async function installPendingUpdate(): Promise<void> {
  const u = state.update;
  if (!u || state.status === "downloading") return;
  setState({ status: "downloading", percent: null, error: null });
  try {
    await installUpdate(
      u,
      (percent) => setState({ percent }),
      () => setState({ status: "installed" }),
    );
  } catch (e) {
    console.error("updater install:", e);
    setState({ status: "installFailed", error: String(e) });
  }
}
