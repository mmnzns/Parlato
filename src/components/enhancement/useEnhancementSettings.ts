// Parlato: state and handlers for the AI cleanup page (provider, key,
// model and prompt logic is unchanged from upstream).

import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { translateError } from "@/lib/translateError";
import { persist } from "@/lib/persist";
import { fail, type Status } from "@/lib/status";
import {
  api,
  type CustomPrompt,
  type LLMProviderInfo,
  type LLMSelection,
} from "@/lib/tauri";
import { LOCAL_IDS, OTHER_MODEL, TYPED_MODEL_IDS } from "@/components/enhancement/constants";

export function useEnhancementSettings() {
  const { t } = useTranslation();
  const [enabled, setEnabled] = useState(false);
  const [providers, setProviders] = useState<LLMProviderInfo[]>([]);
  const [selection, setSelection] = useState<LLMSelection | null>(null);
  const [prompts, setPrompts] = useState<CustomPrompt[]>([]);
  const [activePromptId, setActivePromptId] = useState<string | null>(null);
  const [apiKeyInputs, setApiKeyInputs] = useState<Record<string, string>>({});
  const [verifying, setVerifying] = useState<Record<string, boolean>>({});
  const [status, setStatus] = useState<Record<string, Status>>({});
  const [ollamaBaseUrl, setOllamaBaseUrl] = useState("");
  const [ollamaModels, setOllamaModels] = useState<string[]>([]);
  const [ollamaStatus, setOllamaStatus] = useState("");
  const [customBaseUrl, setCustomBaseUrl] = useState("");
  const [customModel, setCustomModel] = useState("");
  // Parlato: text of the "Other model" box; null while a listed model is picked.
  const [typedModel, setTypedModel] = useState<string | null>(null);
  const [localcliCustomCmd, setLocalcliCustomCmd] = useState("");
  const [localcliTimeout, setLocalcliTimeout] = useState(45);
  const [localcliStatus, setLocalcliStatus] = useState("");

  useEffect(() => {
    refresh();
  }, []);

  async function refresh() {
    try {
      const [en, provs, sel, ps, act, oBase, cBase, liCmd, liTo] =
        await Promise.all([
          api.getEnhancementEnabled(),
          api.listLlmProviders(),
          api.getLlmSelection(),
          api.listPrompts(),
          api.getActivePromptId(),
          api.getOllamaBaseUrl(),
          api.getCustomBaseUrl(),
          api.getLocalcliCustomCmd(),
          api.getLocalcliTimeoutSecs(),
        ]);
      setEnabled(en);
      setProviders(provs);
      setSelection(sel);
      setPrompts(ps);
      setActivePromptId(act);
      setOllamaBaseUrl(oBase);
      setCustomBaseUrl(cBase ?? "");
      setLocalcliCustomCmd(liCmd ?? "");
      setLocalcliTimeout(liTo);
      if (sel?.provider_id === "custom") {
        setCustomModel(sel.model);
      }
      if (sel?.provider_id === "ollama") {
        refreshOllamaModels();
      }
    } catch (e) {
      console.error(e);
    }
  }

  async function refreshOllamaModels() {
    setOllamaStatus(t("enhancement.loadingModels"));
    try {
      const models = await api.listOllamaModels();
      setOllamaModels(models);
      setOllamaStatus(t("enhancement.modelsCount", { count: models.length }));
    } catch (e) {
      setOllamaModels([]);
      setOllamaStatus(t("enhancement.errorPrefix", { message: String(e) }));
    }
  }

  async function saveOllamaBaseUrl() {
    try {
      await api.setOllamaBaseUrl(ollamaBaseUrl.trim());
      setOllamaStatus(t("enhancement.urlSaved"));
      await refreshOllamaModels();
    } catch (e) {
      setOllamaStatus(t("enhancement.errorPrefix", { message: String(e) }));
    }
  }

  async function saveCustomBaseUrl() {
    try {
      await api.setCustomBaseUrl(customBaseUrl.trim());
      setStatus((s) => ({ ...s, custom: t("enhancement.urlSaved") }));
    } catch (e) {
      setStatus((s) => ({ ...s, custom: fail(t("enhancement.errorPrefix", { message: String(e) })) }));
    }
  }

  async function selectCustomModel(model: string) {
    const prev = customModel;
    if (await persist(model, prev, setCustomModel, (m) => api.setLlmSelection("custom", m))) {
      setSelection({ provider_id: "custom", model });
    }
  }

  async function toggleEnabled(v: boolean) {
    await persist(v, enabled, setEnabled, api.setEnhancementEnabled);
  }

  async function selectProvider(providerId: string) {
    const p = providers.find((x) => x.id === providerId);
    if (!p) return;
    setTypedModel(null);
    const model = p.default_model || "";
    setSelection({ provider_id: providerId, model });
    await api.setLlmSelection(providerId, model);
    if (providerId === "ollama") {
      await refreshOllamaModels();
    }
  }

  async function selectModel(model: string) {
    if (!selection) return;
    if (model === OTHER_MODEL) {
      setTypedModel("");
      return;
    }
    setTypedModel(null);
    await persist({ ...selection, model }, selection, setSelection, (s) =>
      api.setLlmSelection(s.provider_id, s.model),
    );
  }

  async function saveTypedModel() {
    const model = (typedModel ?? "").trim();
    if (!selection || !model) return;
    await persist({ ...selection, model }, selection, setSelection, (s) =>
      api.setLlmSelection(s.provider_id, s.model),
    );
  }

  async function selectPrompt(id: string) {
    await persist<string | null>(id, activePromptId, setActivePromptId, () => api.setActivePromptId(id));
  }

  async function saveApiKey(providerId: string) {
    const k = (apiKeyInputs[providerId] ?? "").trim();
    if (!k) return;
    setVerifying((v) => ({ ...v, [providerId]: true }));
    setStatus((s) => ({ ...s, [providerId]: t("enhancement.saveKeyInProgress") }));
    try {
      await api.verifyLlmApiKey(providerId, k);
      await api.setApiKey(providerId, k);
      setStatus((s) => ({ ...s, [providerId]: t("enhancement.keySavedOk") }));
      setApiKeyInputs((i) => ({ ...i, [providerId]: "" }));
      await refresh();
    } catch (e) {
      setStatus((s) => ({
        ...s,
        [providerId]: fail(t("enhancement.errorPrefix", { message: translateError(t, String(e)) })),
      }));
    } finally {
      setVerifying((v) => ({ ...v, [providerId]: false }));
    }
  }

  async function deleteKey(providerId: string) {
    try {
      await api.deleteApiKey(providerId);
      setStatus((s) => ({ ...s, [providerId]: t("enhancement.keyDeleted") }));
      await refresh();
    } catch (e) {
      setStatus((s) => ({ ...s, [providerId]: fail(t("enhancement.errorPrefix", { message: String(e) })) }));
    }
  }

  const currentProvider = providers.find(
    (p) => p.id === selection?.provider_id,
  );
  const isOllama = selection?.provider_id === "ollama";
  const isCustom = selection?.provider_id === "custom";
  const isLocalcli = selection?.provider_id === "localcli";
  const isLocalcliCustom = isLocalcli && selection?.model === "custom";
  const typesModels = TYPED_MODEL_IDS.includes(selection?.provider_id ?? "");
  // A saved model that is not in the list was typed by hand earlier.
  const showTypedModel =
    typesModels &&
    !!currentProvider &&
    (typedModel !== null ||
      (!!selection?.model && !currentProvider.models.includes(selection.model)));

  async function saveLocalcliCustomCmd() {
    try {
      await api.setLocalcliCustomCmd(localcliCustomCmd.trim());
      setLocalcliStatus(t("enhancement.commandSaved"));
    } catch (e) {
      setLocalcliStatus(t("enhancement.errorPrefix", { message: String(e) }));
    }
  }

  async function saveLocalcliTimeout() {
    try {
      await api.setLocalcliTimeoutSecs(Math.max(5, Math.floor(localcliTimeout)));
      setLocalcliStatus(t("enhancement.timeoutSaved"));
    } catch (e) {
      setLocalcliStatus(t("enhancement.errorPrefix", { message: String(e) }));
    }
  }

  const [advanced, setAdvanced] = useState(false);
  const where: "local" | "cloud" | null = !selection?.provider_id
    ? null
    : LOCAL_IDS.includes(selection.provider_id)
      ? "local"
      : "cloud";
  const cloudProviders = providers.filter((p) => !LOCAL_IDS.includes(p.id));
  const isBuiltin = selection?.provider_id === "llamacpp";

  async function chooseWhere(next: "local" | "cloud") {
    if (next === where) return;
    if (next === "local") {
      await selectProvider("llamacpp");
    } else {
      // Prefer a service the user already has a key for.
      const pick =
        cloudProviders.find((p) => p.has_api_key) ??
        cloudProviders.find((p) => p.id === "anthropic") ??
        cloudProviders[0];
      if (pick) await selectProvider(pick.id);
    }
  }

  const keyStatus = currentProvider ? status[currentProvider.id] : undefined;

  return {
    enabled,
    providers,
    selection,
    prompts,
    activePromptId,
    apiKeyInputs,
    setApiKeyInputs,
    verifying,
    status,
    ollamaBaseUrl,
    setOllamaBaseUrl,
    ollamaModels,
    ollamaStatus,
    customBaseUrl,
    setCustomBaseUrl,
    customModel,
    setCustomModel,
    typedModel,
    setTypedModel,
    localcliCustomCmd,
    setLocalcliCustomCmd,
    localcliTimeout,
    setLocalcliTimeout,
    localcliStatus,
    advanced,
    setAdvanced,
    currentProvider,
    isOllama,
    isCustom,
    isLocalcli,
    isLocalcliCustom,
    typesModels,
    showTypedModel,
    where,
    cloudProviders,
    isBuiltin,
    keyStatus,
    refresh,
    refreshOllamaModels,
    saveOllamaBaseUrl,
    saveCustomBaseUrl,
    selectCustomModel,
    toggleEnabled,
    selectProvider,
    selectModel,
    saveTypedModel,
    selectPrompt,
    saveApiKey,
    deleteKey,
    saveLocalcliCustomCmd,
    saveLocalcliTimeout,
    chooseWhere,
  };
}

export type EnhancementSettings = ReturnType<typeof useEnhancementSettings>;
