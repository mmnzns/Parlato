// Parlato: [02] service, model and account key when the AI runs online.

import { useTranslation } from "react-i18next";
import { ChevronRight, KeyRound, Loader2, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Block, Row, Section, selectClass } from "@/components/ui/section";
import { cn } from "@/lib/utils";
import { isFailure, statusText } from "@/lib/status";
import { OTHER_MODEL, inputClass } from "@/components/enhancement/constants";
import type { EnhancementSettings } from "@/components/enhancement/useEnhancementSettings";

export function CloudSection({ s }: { s: EnhancementSettings }) {
  const { t } = useTranslation();
  const {
    selection,
    status,
    cloudProviders,
    currentProvider,
    isCustom,
    typesModels,
    showTypedModel,
    typedModel,
    setTypedModel,
    customBaseUrl,
    setCustomBaseUrl,
    customModel,
    setCustomModel,
    apiKeyInputs,
    setApiKeyInputs,
    verifying,
    advanced,
    setAdvanced,
    keyStatus,
    selectProvider,
    selectModel,
    saveTypedModel,
    saveCustomBaseUrl,
    selectCustomModel,
    saveApiKey,
    deleteKey,
  } = s;
  const keyIsError = isFailure(keyStatus);

  return (
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
            value={showTypedModel ? OTHER_MODEL : (selection?.model ?? "")}
            onChange={(e) => selectModel(e.target.value)}
            className={selectClass}
          >
            {!typesModels &&
              selection?.model &&
              !currentProvider.models.includes(selection.model) && (
                <option value={selection.model}>
                  {t("enhancement.currentModel", { model: selection.model })}
                </option>
              )}
            {currentProvider.models.map((m) => (
              <option key={m} value={m}>
                {m}
              </option>
            ))}
            {typesModels && <option value={OTHER_MODEL}>{t("enhancement.otherModel")}</option>}
          </select>
        </Row>
      )}

      {showTypedModel && currentProvider && (
        <Row
          label={t("enhancement.modelIdLabel")}
          description={t("enhancement.modelIdHint", { provider: currentProvider.label })}
        >
          <input
            type="text"
            aria-label={t("enhancement.modelIdLabel")}
            value={typedModel ?? selection?.model ?? ""}
            onChange={(e) => setTypedModel(e.target.value)}
            onBlur={saveTypedModel}
            onKeyDown={(e) => {
              if (e.key === "Enter") saveTypedModel();
            }}
            placeholder={t("enhancement.customModelPlaceholder")}
            className={cn(inputClass, "w-[240px]")}
          />
        </Row>
      )}

      {isCustom && (
        <>
          <Row label={t("enhancement.customBaseUrlLabel")} description={status.custom ? statusText(status.custom) : undefined}>
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
            <div className="flex h-[34px] min-w-0 flex-1 items-center gap-2 rounded-sm border-[1.5px] border-input bg-background px-3 focus-within:border-foreground">
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
              {statusText(keyStatus)}
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
  );
}
