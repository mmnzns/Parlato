// Parlato: "what this mode does" section: speech model, language, auto
// send and the AI cleanup settings.

import { useTranslation } from "react-i18next";
import { Row, Section, Switch, selectClass } from "@/components/ui/section";
import { cn } from "@/lib/utils";
import { isMac } from "@/lib/platform";
import { promptTitle } from "@/lib/promptLabels";
import { modelName } from "@/lib/modelText";
import { LANGUAGE_NAMES, languageLabel } from "@/lib/languages";
import type { AutoSendKey, PowerModeConfig } from "@/lib/tauri";
import { FreeTextModel } from "./FreeTextModel";
import type { Catalog } from "./config";

export function DoesSection({
  config: c,
  catalog,
  onChange,
  set,
}: {
  config: PowerModeConfig;
  catalog: Catalog;
  onChange: (c: PowerModeConfig) => void;
  set: <K extends keyof PowerModeConfig>(key: K, value: PowerModeConfig[K]) => void;
}) {
  const { t, i18n } = useTranslation();
  const languageCodes = Object.keys(LANGUAGE_NAMES).sort((a, b) =>
    languageLabel(a, i18n.resolvedLanguage).localeCompare(
      languageLabel(b, i18n.resolvedLanguage),
      i18n.resolvedLanguage,
    ),
  );

  // Speech model: one select combining kind + model id ("" = keep usual).
  const speechValue =
    c.transcription_kind === "local" && c.whisper_model_id
      ? `local:${c.whisper_model_id}`
      : c.transcription_kind === "parakeet" && c.parakeet_model_id
        ? `parakeet:${c.parakeet_model_id}`
        : c.transcription_kind === "cloud" && c.cloud_provider && c.cloud_model
          ? `cloud:${c.cloud_provider}:${c.cloud_model}`
          : "";

  function setSpeech(v: string) {
    const base = {
      ...c,
      transcription_kind: null,
      whisper_model_id: null,
      parakeet_model_id: null,
      cloud_provider: null,
      cloud_model: null,
    } as PowerModeConfig;
    if (v.startsWith("local:")) onChange({ ...base, transcription_kind: "local", whisper_model_id: v.slice(6) });
    else if (v.startsWith("parakeet:")) onChange({ ...base, transcription_kind: "parakeet", parakeet_model_id: v.slice(9) });
    else if (v.startsWith("cloud:")) {
      const [, provider, ...model] = v.split(":");
      onChange({ ...base, transcription_kind: "cloud", cloud_provider: provider, cloud_model: model.join(":") });
    } else onChange(base);
  }

  const provider = catalog.providers.find((p) => p.id === c.selected_llm_provider);
  const whisper = catalog.whisper.filter((m) => m.downloaded);
  const parakeet = catalog.parakeet.filter((m) => m.downloaded);
  const cloud = catalog.cloud.filter((m) => m.supports_batch);

  return (
    <Section title={t("pm.doesTitle")} description={t("pm.doesDesc")}>
      <Row label={t("pm.speechModel")} description={t("pm.speechModelDesc")}>
        <select
          aria-label={t("pm.speechModel")}
          value={speechValue}
          onChange={(e) => setSpeech(e.target.value)}
          className={cn(selectClass, "max-w-[240px]")}
        >
          <option value="">{t("pm.keep")}</option>
          {whisper.length > 0 && (
            <optgroup label={t("pm.localWhisper")}>
              {whisper.map((m) => (
                <option key={m.id} value={`local:${m.id}`}>
                  {modelName(t, m.id, m.display_name)}
                </option>
              ))}
            </optgroup>
          )}
          {parakeet.length > 0 && (
            <optgroup label={t("pm.localParakeet")}>
              {parakeet.map((m) => (
                <option key={m.id} value={`parakeet:${m.id}`}>
                  {modelName(t, m.id, m.display_name)}
                </option>
              ))}
            </optgroup>
          )}
          <optgroup label={t("pm.online")}>
            {cloud.map((m) => (
              <option key={`${m.provider_id}:${m.model_id}`} value={`cloud:${m.provider_id}:${m.model_id}`}>
                {m.display_name}
              </option>
            ))}
          </optgroup>
        </select>
      </Row>
      <Row label={t("pm.language")} description={t("pm.languageDesc")}>
        <select
          aria-label={t("pm.language")}
          value={c.language ?? ""}
          onChange={(e) => set("language", e.target.value || null)}
          className={cn(selectClass, "max-w-[240px]")}
        >
          <option value="">{t("pm.keep")}</option>
          <option value="auto">{t("dictationLanguage.auto")}</option>
          {languageCodes.map((code) => (
            <option key={code} value={code}>
              {languageLabel(code, i18n.resolvedLanguage)}
            </option>
          ))}
        </select>
      </Row>
      <Row label={t("pm.autoSend")} description={t("pm.autoSendDesc")}>
        <select
          aria-label={t("pm.autoSend")}
          value={c.auto_send_key}
          onChange={(e) => set("auto_send_key", e.target.value as AutoSendKey)}
          className={selectClass}
        >
          <option value="none">{t("pm.autoSendNone")}</option>
          <option value="enter">{t("pm.autoSendEnter")}</option>
          <option value="shift_enter">{t("pm.autoSendShiftEnter")}</option>
          <option value="ctrl_enter">{t("pm.autoSendCtrlEnter")}</option>
        </select>
      </Row>
      <Row label={t("pm.aiCleanup")} description={t("pm.aiCleanupDesc")} htmlFor="pm-ai">
        <Switch id="pm-ai" checked={c.is_enhancement_enabled} onChange={(v) => set("is_enhancement_enabled", v)} />
      </Row>
      {c.is_enhancement_enabled && (
        <>
          <Row label={t("pm.aiService")} description={t("pm.aiServiceDesc")}>
            <select
              aria-label={t("pm.aiService")}
              value={c.selected_llm_provider ?? ""}
              onChange={(e) => {
                const id = e.target.value || null;
                const p = catalog.providers.find((x) => x.id === id);
                onChange({ ...c, selected_llm_provider: id, selected_llm_model: id ? (p?.default_model ?? null) : null });
              }}
              className={cn(selectClass, "max-w-[240px]")}
            >
              <option value="">{t("pm.keep")}</option>
              {/* Parlato: the command-line engine is hidden on Mac, as on
                  the AI cleanup page (kept if a mode already uses it). */}
              {catalog.providers
                .filter((p) => !isMac || p.id !== "localcli" || c.selected_llm_provider === "localcli")
                .map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.label}
                  </option>
                ))}
            </select>
          </Row>
          {provider && (
            <Row label={t("pm.aiModel")}>
              {provider.models.length > 0 ? (
                <select
                  aria-label={t("pm.aiModel")}
                  value={c.selected_llm_model ?? ""}
                  onChange={(e) => set("selected_llm_model", e.target.value || null)}
                  className={cn(selectClass, "max-w-[240px]")}
                >
                  {c.selected_llm_model && !provider.models.includes(c.selected_llm_model) && (
                    <option value={c.selected_llm_model}>{c.selected_llm_model}</option>
                  )}
                  {provider.models.map((m) => (
                    <option key={m} value={m}>
                      {m}
                    </option>
                  ))}
                </select>
              ) : (
                <FreeTextModel value={c.selected_llm_model ?? ""} onCommit={(v) => set("selected_llm_model", v || null)} />
              )}
            </Row>
          )}
          <Row label={t("pm.style")} description={t("pm.styleDesc")}>
            <select
              aria-label={t("pm.style")}
              value={c.selected_prompt_id ?? ""}
              onChange={(e) => set("selected_prompt_id", e.target.value || null)}
              className={cn(selectClass, "max-w-[240px]")}
            >
              <option value="">{t("pm.keep")}</option>
              {catalog.prompts.map((p) => (
                <option key={p.id} value={p.id}>
                  {promptTitle(t, p)}
                </option>
              ))}
            </select>
          </Row>
          {/* Parlato: reading the screen is not available on Mac yet. */}
          {!isMac && (
            <Row label={t("pm.screen")}>
              <select
                aria-label={t("pm.screen")}
                value={c.use_screen_capture === null ? "" : c.use_screen_capture ? "on" : "off"}
                onChange={(e) =>
                  set("use_screen_capture", e.target.value === "" ? null : e.target.value === "on")
                }
                className={selectClass}
              >
                <option value="">{t("pm.keep")}</option>
                <option value="on">{t("pm.screenOn")}</option>
                <option value="off">{t("pm.screenOff")}</option>
              </select>
            </Row>
          )}
        </>
      )}
    </Section>
  );
}
