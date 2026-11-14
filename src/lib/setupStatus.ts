// Parlato: "can this app dictate right now?" in one place, so the sidebar
// card, the Home header and the setup wizard never disagree.
//
// Four things must be true: a speech model is in use (on this computer and
// downloaded, or online with a saved key), a dictation shortcut is set, a
// microphone exists, and on a Mac the Microphone and Accessibility permissions
// are allowed (the shortcut and paste need Accessibility).

import type { TFunction } from "i18next";
import { modelName } from "@/lib/modelText";
import { api } from "@/lib/tauri";
import { isMac } from "@/lib/platform";
import type { View } from "@/components/Sidebar";

export type MissingItem = "model" | "shortcut" | "accessibility" | "micAccess" | "mic";

/** Page that fixes each missing item. */
export const MISSING_VIEW: Record<MissingItem, View> = {
  model: "models",
  shortcut: "audio",
  accessibility: "permissions",
  micAccess: "permissions",
  mic: "audio",
};

export type SetupStatus = {
  /** Name of the speech model in use, or null when none is. */
  model: string | null;
  shortcut: boolean;
  mic: boolean;
  /** Always true off the Mac. */
  accessibility: boolean;
  /** Microphone permission; always true off the Mac. */
  micAccess: boolean;
  /** First thing that stops dictation, in the order a person would fix them. */
  missing: MissingItem | null;
};

/** Name of the speech model in use, or null. A model counts only when it can
 *  really run: downloaded and complete, or an online model with a saved key. */
export async function speechModelInUse(t: TFunction): Promise<string | null> {
  const source = await api.getTranscriptionSource().catch(() => null);
  if (!source) return null;
  if (source.kind === "local" && source.whisper_model_id) {
    const list = await api.listWhisperModels().catch(() => []);
    const m = list.find((x) => x.id === source.whisper_model_id);
    return m?.downloaded ? modelName(t, m.id, m.display_name) : null;
  }
  if (source.kind === "parakeet" && source.parakeet_model_id) {
    const list = await api.listParakeetModels().catch(() => []);
    const m = list.find((x) => x.id === source.parakeet_model_id);
    return m?.downloaded && m.missing_files.length === 0 ? modelName(t, m.id, m.display_name) : null;
  }
  if (source.kind === "cloud" && source.cloud_provider && source.cloud_model) {
    const [providers, models] = await Promise.all([
      api.listCloudProviders().catch(() => []),
      api.listCloudModels().catch(() => []),
    ]);
    const p = providers.find((x) => x.id === source.cloud_provider);
    const m = models.find((x) => x.provider_id === source.cloud_provider && x.model_id === source.cloud_model);
    return p?.has_api_key && m ? m.display_name : null;
  }
  return null;
}

export async function loadSetupStatus(t: TFunction): Promise<SetupStatus> {
  const [model, hotkey, devices, perms] = await Promise.all([
    speechModelInUse(t),
    api.getHotkeyConfig().catch(() => null),
    api.listAudioDevices().catch(() => []),
    isMac ? api.checkPermissions().catch(() => null) : Promise.resolve(null),
  ]);
  const shortcut = !!hotkey && (hotkey.primary.trigger.kind !== "none" || hotkey.secondary.trigger.kind !== "none");
  const mic = devices.length > 0;
  // When the permission check fails, do not claim anything is missing.
  const accessibility = !isMac || !perms?.accessibility || perms.accessibility.ok;
  const micAccess = !isMac || !perms?.microphone || perms.microphone.ok;
  const missing: MissingItem | null = !model
    ? "model"
    : !shortcut
      ? "shortcut"
      : !accessibility
        ? "accessibility"
        : !micAccess
          ? "micAccess"
          : !mic
            ? "mic"
            : null;
  return { model, shortcut, mic, accessibility, micAccess, missing };
}

/** "Event" other panels fire when they change something readiness depends on
 *  and no backend event says so (the shortcut, the microphone). */
export const SETUP_CHANGED_EVENT = "parlato:setup-changed";
export const notifySetupChanged = () => window.dispatchEvent(new Event(SETUP_CHANGED_EVENT));
