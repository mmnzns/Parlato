// Parlato: state and backend calls of the power modes page (modes list,
// selection, model catalog, auto-restore, delete confirmation).

import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { persist } from "@/lib/persist";
import { api, type PowerModeConfig } from "@/lib/tauri";
import type { CloudModel } from "@/components/models/types";
import { duplicateName, emptyConfig, type Catalog } from "./config";

export function usePowerModes() {
  const { t } = useTranslation();
  const [configs, setConfigs] = useState<PowerModeConfig[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [autoRestore, setAutoRestore] = useState(true);
  const [catalog, setCatalog] = useState<Catalog>({ prompts: [], providers: [], whisper: [], parakeet: [], cloud: [] });
  const [confirming, setConfirming] = useState(false);

  useEffect(() => {
    refresh(true);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Modes load on their own: a failing model or prompt list must not hide
  // them, and the backend refuses edits while they cannot be read.
  async function refresh(pickFirst = false) {
    try {
      const [cs, ar] = await Promise.all([api.listPowerConfigs(), api.getPowerAutoRestore()]);
      setConfigs(cs);
      setAutoRestore(ar);
      if (pickFirst && cs.length > 0) setSelectedId(cs[0].id);
    } catch (e) {
      console.error(e);
    }
    try {
      const [prompts, providers, whisper, parakeet, cloud] = await Promise.all([
        api.listPrompts(),
        api.listLlmProviders(),
        api.listWhisperModels(),
        api.listParakeetModels(),
        api.listCloudModels(),
      ]);
      setCatalog({ prompts, providers, whisper, parakeet, cloud: cloud as CloudModel[] });
    } catch (e) {
      console.error(e);
    }
  }

  async function createMode() {
    try {
      const created = await api.addPowerConfig(emptyConfig(t("pm.newName")));
      await refresh();
      setSelectedId(created.id);
    } catch (e) {
      console.error(e);
    }
  }

  // VoiceInk 2.21 duplicateConfiguration: same settings, no app or website
  // triggers (so the two modes never compete), never the default. Added at
  // the end so no other mode's Alt+number shortcut moves.
  async function duplicateSelected() {
    const source = configs.find((c) => c.id === selectedId);
    if (!source) return;
    try {
      const created = await api.addPowerConfig({
        ...source,
        id: "",
        name: duplicateName(source.name, configs.map((c) => c.name)),
        app_triggers: [],
        url_triggers: [],
        shortcut_slot: null,
        is_default: false,
      });
      await refresh();
      setSelectedId(created.id);
    } catch (e) {
      console.error(e);
    }
  }

  async function save(next: PowerModeConfig) {
    const prev = configs.find((c) => c.id === next.id);
    // Optimistic: the editor stays responsive, the list reflects the change.
    setConfigs((cs) => cs.map((c) => (c.id === next.id ? next : c)));
    try {
      await api.updatePowerConfig(next);
      // is_default and shortcut_slot are exclusive: the backend may have
      // cleared them on another mode.
      if (next.is_default || next.shortcut_slot !== prev?.shortcut_slot) await refresh();
    } catch (e) {
      console.error(e);
      await refresh();
    }
  }

  async function removeSelected() {
    if (!selectedId) return;
    try {
      await api.deletePowerConfig(selectedId);
      setConfirming(false);
      const rest = configs.filter((c) => c.id !== selectedId);
      setSelectedId(rest[0]?.id ?? null);
      await refresh();
    } catch (e) {
      console.error(e);
    }
  }

  async function toggleAutoRestore(v: boolean) {
    await persist(v, autoRestore, setAutoRestore, api.setPowerAutoRestore);
  }

  return {
    configs,
    selectedId,
    setSelectedId,
    autoRestore,
    catalog,
    confirming,
    setConfirming,
    createMode,
    duplicateSelected,
    save,
    removeSelected,
    toggleAutoRestore,
  };
}
