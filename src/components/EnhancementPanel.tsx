// Parlato: AI cleanup page, rebuilt to the Workbench design (docs/design/v1,
// screen "ai"). The page header carries the On/Off switch; below it:
//   [01] where the AI runs (this PC vs an online service)
//   [02] the engine / service, model and account key for that choice
//   [03] writing style (prompts)
//   then the screen-context toggle.
// Provider, key and prompt logic is unchanged from upstream.

import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { ChevronRight, Cloud, KeyRound, Laptop, Loader2, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Block, Row, Section, SectionHeading, Switch, selectClass } from "@/components/ui/section";
import { CompactHero } from "@/components/CompactHero";
import { EnhancementScreenContext } from "@/components/EnhancementScreenContext";
import { LlmLocalPanel } from "@/components/LlmLocalPanel";
import { PromptEditor } from "@/components/PromptEditor";
import { cn } from "@/lib/utils";
import {
  api,
  type CustomPrompt,
  type LLMProviderInfo,
  type LLMSelection,
} from "@/lib/tauri";

/** Providers that run on this PC. Everything else is an online service. */
const LOCAL_IDS = ["llamacpp", "ollama", "localcli"];

const inputClass = "h-[34px] w-full rounded-sm border-[1.5px] border-input bg-background px-3 text-sm";

export function EnhancementPanel({ crumb }: { crumb?: string }) {
  const { t } = useTranslation();
  const [enabled, setEnabled] = useState(false);
  const [providers, setProviders] = useState<LLMProviderInfo[]>([]);
  const [selection, setSelection] = useState<LLMSelection | null>(null);
  const [prompts, setPrompts] = useState<CustomPrompt[]>([]);
  const [activePromptId, setActivePromptId] = useState<string | null>(null);
  const [apiKeyInputs, setApiKeyInputs] = useState<Record<string, string>>({});
  const [verifying, setVerifying] = useState<Record<string, boolean>>({});
  const [status, setStatus] = useState<Record<string, string>>({});
  const [ollamaBaseUrl, setOllamaBaseUrl] = useState("");
  const [ollamaModels, setOllamaModels] = useState<string[]>([]);
  const [ollamaStatus, setOllamaStatus] = useState("");
  const [customBaseUrl, setCustomBaseUrl] = useState("");
  const [customModel, setCustomModel] = useState("");
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
      setStatus((s) => ({ ...s, custom: t("enhancement.errorPrefix", { message: String(e) }) }));
    }
  }

  async function selectCustomModel(model: string) {
    setCustomModel(model);
    await api.setLlmSelection("custom", model);
    setSelection({ provider_id: "custom", model });
  }

  async function toggleEnabled(v: boolean) {
    setEnabled(v);
    await api.setEnhancementEnabled(v);
  }

  async function selectProvider(providerId: string) {
    const p = providers.find((x) => x.id === providerId);
    if (!p) return;
    const model = p.default_model || "";
    setSelection({ provider_id: providerId, model });
    await api.setLlmSelection(providerId, model);
    if (providerId === "ollama") {
      await refreshOllamaModels();
    }
  }

  async function selectModel(model: string) {
    if (!selection) return;
    setSelection({ ...selection, model });
    await api.setLlmSelection(selection.provider_id, model);
  }

  async function selectPrompt(id: string) {
    setActivePromptId(id);
    await api.setActivePromptId(id);
  }

  async function saveApiKey(providerId: string) {
    const k = (apiKeyInputs[providerId] ?? "").trim();
    if (!k) return;
    setVerifying((v) => ({ ...v, [providerId]: true }));
    setStatus((s) => ({ ...s, [providerId]: t("enhancement.saveKeyInProgress") }));
    try {
      await api.setApiKey(providerId, k);
      setStatus((s) => ({ ...s, [providerId]: t("enhancement.keySavedOk") }));
      setApiKeyInputs((i) => ({ ...i, [providerId]: "" }));
      await refresh();
    } catch (e) {
      setStatus((s) => ({ ...s, [providerId]: t("enhancement.errorPrefix", { message: String(e) }) }));
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
      setStatus((s) => ({ ...s, [providerId]: t("enhancement.errorPrefix", { message: String(e) }) }));
    }
  }

  const currentProvider = providers.find(
    (p) => p.id === selection?.provider_id,
  );
  const isOllama = selection?.provider_id === "ollama";
  const isCustom = selection?.provider_id === "custom";
  const isLocalcli = selection?.provider_id === "localcli";
  const isLocalcliCustom = isLocalcli && selection?.model === "custom";

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
  const where = !selection?.provider_id
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

  const whereCards = [
    { id: "local" as const, Icon: Laptop, title: t("ai.localTitle"), desc: t("ai.localDesc"), eg: t("ai.localEg"), rec: true },
    { id: "cloud" as const, Icon: Cloud, title: t("ai.cloudTitle"), desc: t("ai.cloudDesc"), eg: t("ai.cloudEg"), rec: false },
  ];

  const keyStatus = currentProvider ? status[currentProvider.id] : undefined;
  const keyIsError = !!keyStatus && keyStatus.startsWith(t("common.error"));

  return (
    <>
      <CompactHero
        crumb={crumb}
        title={t("hero.enhancementTitle")}
        description={t("hero.enhancementDescription")}
        action={
          <>
            <span className="text-[13px] font-semibold">{enabled ? t("ai.on") : t("ai.off")}</span>
            <Switch checked={enabled} onChange={toggleEnabled} label={t("ai.toggleLabel")} />
          </>
        }
      />

      <div
        className={cn(
          "flex flex-col gap-[22px] transition-opacity",
          !enabled && "pointer-events-none opacity-45",
        )}
        aria-disabled={!enabled}
      >
        <section className="flex flex-col gap-2.5">
          <SectionHeading title={t("ai.whereTitle")} />
          <div role="radiogroup" aria-label={t("ai.whereTitle")} className="grid gap-3 sm:grid-cols-2">
            {whereCards.map((w) => {
              const sel = where === w.id;
              return (
                <button
                  key={w.id}
                  type="button"
                  role="radio"
                  aria-checked={sel}
                  onClick={() => chooseWhere(w.id)}
                  className={cn(
                    "flex items-start gap-3.5 rounded-lg border-[1.5px] bg-card p-4 text-left transition-colors",
                    sel ? "border-edge bg-accent shadow-[var(--sel-shadow)]" : "border-input hover:border-edge",
                  )}
                >
                  <span
                    className={cn(
                      "mt-0.5 flex h-[18px] w-[18px] flex-none items-center justify-center rounded-full border-[1.5px] bg-card",
                      sel ? "border-foreground" : "border-input",
                    )}
                  >
                    <span className={cn("h-2 w-2 rounded-full bg-foreground", !sel && "opacity-0")} />
                  </span>
                  <span className="flex min-w-0 flex-col gap-1">
                    <span className="flex flex-wrap items-center gap-2 font-display font-bold">
                      <w.Icon className="h-4 w-4" />
                      {w.title}
                      {w.rec && (
                        <span className="rounded-full bg-highlight px-2 py-px font-sans text-[11px] font-semibold text-[#141416]">
                          {t("ai.recommended")}
                        </span>
                      )}
                    </span>
                    <span className="text-[13px] leading-[18px] text-pretty text-muted-foreground">{w.desc}</span>
                    <span className="font-mono text-[11px] text-muted-foreground">{w.eg}</span>
                  </span>
                </button>
              );
            })}
          </div>
        </section>

        {where === "local" && (
          <Section title={t("ai.localTitle")}>
            <Row label={t("ai.engine")}>
              <select
                aria-label={t("ai.engine")}
                value={selection?.provider_id ?? "llamacpp"}
                onChange={(e) => selectProvider(e.target.value)}
                className={selectClass}
              >
                <option value="llamacpp">{t("ai.engineBuiltin")}</option>
                <option value="ollama">{t("ai.engineOllama")}</option>
                <option value="localcli">{t("ai.engineCli")}</option>
              </select>
            </Row>

            {isBuiltin && <LlmLocalPanel />}

            {isOllama && (
              <>
                <Row label={t("enhancement.ollamaBaseUrl")} description={ollamaStatus || undefined}>
                  <input
                    type="text"
                    value={ollamaBaseUrl}
                    onChange={(e) => setOllamaBaseUrl(e.target.value)}
                    placeholder="http://localhost:11434"
                    className={cn(inputClass, "w-[220px]")}
                  />
                  <Button size="sm" variant="outline" onClick={saveOllamaBaseUrl}>
                    {t("enhancement.save")}
                  </Button>
                </Row>
                <Row label={t("ai.model")}>
                  <select
                    aria-label={t("ai.model")}
                    value={selection?.model ?? ""}
                    onChange={(e) => selectModel(e.target.value)}
                    className={selectClass}
                  >
                    {!ollamaModels.includes(selection?.model ?? "") && selection?.model && (
                      <option value={selection.model}>{selection.model}</option>
                    )}
                    {ollamaModels.map((m) => (
                      <option key={m} value={m}>
                        {m}
                      </option>
                    ))}
                  </select>
                  <Button size="sm" variant="ghost" onClick={refreshOllamaModels}>
                    {t("enhancement.refreshModels")}
                  </Button>
                </Row>
              </>
            )}

            {isLocalcli && (
              <>
                <Row label={t("enhancement.localcliTemplate")}>
                  <select
                    aria-label={t("enhancement.localcliTemplate")}
                    value={selection?.model ?? "pi"}
                    onChange={(e) => selectModel(e.target.value)}
                    className={selectClass}
                  >
                    <option value="pi">pi</option>
                    <option value="claude">claude</option>
                    <option value="codex">codex</option>
                    <option value="custom">custom</option>
                  </select>
                </Row>
                <Block className="flex flex-col gap-3">
                  <p className="text-xs text-muted-foreground">
                    {t("enhancement.localcliHelpA")} <code>powershell.exe -NoProfile -Command</code>
                    {t("enhancement.localcliHelpB")} <code>$env:PARLA_SYSTEM_PROMPT</code>,{" "}
                    <code>$env:PARLA_USER_PROMPT</code>, <code>$env:PARLA_FULL_PROMPT</code>.
                  </p>
                  {isLocalcliCustom && (
                    <div className="flex flex-col gap-2">
                      <label className="text-sm font-medium">{t("enhancement.customCommandLabel")}</label>
                      <textarea
                        value={localcliCustomCmd}
                        onChange={(e) => setLocalcliCustomCmd(e.target.value)}
                        placeholder="& mon-cli -p $env:PARLA_FULL_PROMPT"
                        className="min-h-[80px] rounded-sm border-[1.5px] border-input bg-background px-3 py-2 font-mono text-sm"
                      />
                      <Button size="sm" className="self-start" onClick={saveLocalcliCustomCmd}>
                        {t("enhancement.saveCommand")}
                      </Button>
                    </div>
                  )}
                </Block>
                <Row label={t("enhancement.timeoutLabel")} description={localcliStatus || undefined}>
                  <input
                    type="number"
                    min={5}
                    max={300}
                    value={localcliTimeout}
                    onChange={(e) => setLocalcliTimeout(Number(e.target.value) || 45)}
                    className={cn(inputClass, "w-[90px]")}
                  />
                  <Button size="sm" variant="outline" onClick={saveLocalcliTimeout}>
                    {t("enhancement.saveTimeout")}
                  </Button>
                </Row>
              </>
            )}
          </Section>
        )}

        {where === "cloud" && (
          <Section title={t("ai.cloudTitle")}>
            <Row label={t("ai.service")}>
              <select
                aria-label={t("ai.service")}
                value={selection?.provider_id ?? ""}
                onChange={(e) => selectProvider(e.target.value)}
                className={selectClass}
              >
                {cloudProviders.map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.label}
                    {p.has_api_key ? ` - ${t("ai.keyStored")}` : ""}
                  </option>
                ))}
              </select>
            </Row>

            {currentProvider && !isCustom && currentProvider.models.length > 0 && (
              <Row label={t("ai.model")}>
                <select
                  aria-label={t("ai.model")}
                  value={selection?.model ?? ""}
                  onChange={(e) => selectModel(e.target.value)}
                  className={selectClass}
                >
                  {selection?.model && !currentProvider.models.includes(selection.model) && (
                    <option value={selection.model}>
                      {t("enhancement.currentModel", { model: selection.model })}
                    </option>
                  )}
                  {currentProvider.models.map((m) => (
                    <option key={m} value={m}>
                      {m}
                    </option>
                  ))}
                </select>
              </Row>
            )}

            {isCustom && (
              <>
                <Row label={t("enhancement.customBaseUrlLabel")} description={status.custom || undefined}>
                  <input
                    type="text"
                    value={customBaseUrl}
                    onChange={(e) => setCustomBaseUrl(e.target.value)}
                    placeholder="https://my-llm.example.com/v1"
                    className={cn(inputClass, "w-[240px]")}
                  />
                  <Button size="sm" variant="outline" onClick={saveCustomBaseUrl}>
                    {t("enhancement.saveUrl")}
                  </Button>
                </Row>
                <Row label={t("ai.model")}>
                  <input
                    type="text"
                    value={customModel}
                    onChange={(e) => setCustomModel(e.target.value)}
                    onBlur={(e) => selectCustomModel(e.target.value)}
                    placeholder={t("enhancement.customModelPlaceholder")}
                    className={cn(inputClass, "w-[240px]")}
                  />
                </Row>
              </>
            )}

            {currentProvider?.requires_api_key && (
              <Block className="flex flex-col gap-2">
                <span className="font-medium">{t("ai.key")}</span>
                <div className="flex items-center gap-2">
                  <div className="flex h-[34px] min-w-0 flex-1 items-center gap-2 rounded-sm border-[1.5px] border-input bg-background px-3">
                    <KeyRound
                      className={cn(
                        "h-4 w-4 flex-none",
                        currentProvider.has_api_key ? "text-positive" : "text-muted-foreground",
                      )}
                    />
                    <input
                      type="password"
                      autoComplete="off"
                      aria-label={t("ai.key")}
                      placeholder={currentProvider.has_api_key ? t("ai.keyStored") : t("ai.keyPlaceholder")}
                      value={apiKeyInputs[currentProvider.id] ?? ""}
                      onChange={(e) =>
                        setApiKeyInputs((i) => ({ ...i, [currentProvider.id]: e.target.value }))
                      }
                      className="h-full min-w-0 flex-1 bg-transparent font-mono text-sm outline-none placeholder:font-sans placeholder:text-muted-foreground"
                    />
                  </div>
                  <Button
                    size="sm"
                    onClick={() => saveApiKey(currentProvider.id)}
                    disabled={verifying[currentProvider.id] || !(apiKeyInputs[currentProvider.id] ?? "").trim()}
                  >
                    {verifying[currentProvider.id] && <Loader2 className="h-3.5 w-3.5 animate-spin" />}
                    {t("enhancement.saveKey")}
                  </Button>
                  {currentProvider.has_api_key && (
                    <Button
                      size="icon"
                      variant="ghost"
                      className="text-muted-foreground"
                      onClick={() => deleteKey(currentProvider.id)}
                      title={t("ai.keyRemove")}
                      aria-label={t("ai.keyRemove")}
                    >
                      <Trash2 className="h-4 w-4" />
                    </Button>
                  )}
                </div>
                {keyStatus && (
                  <span
                    role={keyIsError ? "alert" : undefined}
                    className={cn("text-xs", keyIsError ? "text-destructive" : "text-muted-foreground")}
                  >
                    {keyStatus}
                  </span>
                )}
                <span className="text-xs text-muted-foreground">{t("ai.keyNote")}</span>
              </Block>
            )}

            <Block className="flex flex-col gap-2 py-3">
              <button
                type="button"
                onClick={() => setAdvanced((a) => !a)}
                aria-expanded={advanced}
                className="flex items-center gap-1.5 self-start text-[13px] font-semibold text-muted-foreground hover:text-foreground"
              >
                <ChevronRight className={cn("h-4 w-4 transition-transform", advanced && "rotate-90")} />
                {t("ai.advanced")}
              </button>
              {advanced && currentProvider && (
                <div className="flex flex-wrap items-center gap-3 pl-5 text-[13px]">
                  <span className="text-muted-foreground">{t("ai.serverAddress")}</span>
                  <code className="rounded-sm bg-muted px-2 py-1 font-mono text-xs">
                    {isCustom ? customBaseUrl || "-" : currentProvider.endpoint}
                  </code>
                </div>
              )}
            </Block>
          </Section>
        )}

        <PromptEditor
          prompts={prompts}
          activeId={activePromptId}
          onSelect={selectPrompt}
          onChange={refresh}
        />

        <EnhancementScreenContext />
      </div>
    </>
  );
}
