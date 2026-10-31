// Parlato: [02] engine settings when the AI runs on this PC.

import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Block, Row, Section, selectClass } from "@/components/ui/section";
import { LlmLocalPanel } from "@/components/LlmLocalPanel";
import { cn } from "@/lib/utils";
import { isMac } from "@/lib/platform";
import { inputClass } from "@/components/enhancement/constants";
import type { EnhancementSettings } from "@/components/enhancement/useEnhancementSettings";

export function LocalSection({ s }: { s: EnhancementSettings }) {
  const { t } = useTranslation();
  const {
    selection,
    isBuiltin,
    isOllama,
    isLocalcli,
    isLocalcliCustom,
    ollamaBaseUrl,
    setOllamaBaseUrl,
    ollamaModels,
    ollamaStatus,
    localcliCustomCmd,
    setLocalcliCustomCmd,
    localcliTimeout,
    setLocalcliTimeout,
    localcliStatus,
    selectProvider,
    selectModel,
    saveOllamaBaseUrl,
    refreshOllamaModels,
    saveLocalcliCustomCmd,
    saveLocalcliTimeout,
  } = s;

  return (
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
          {/* Parlato: the command-line engine runs PowerShell, Windows only for now. */}
          {!isMac && <option value="localcli">{t("ai.engineCli")}</option>}
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
                  placeholder="& my-cli -p $env:PARLA_FULL_PROMPT"
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
  );
}
