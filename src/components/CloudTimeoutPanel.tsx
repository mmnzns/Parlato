// Delai maximum d'une transcription cloud (batch).
//
// Reference VoiceInk Features/ModelLibrary/Views/ModelSettingsPanel.swift +
// AppDefaults `CloudTranscriptionSettings` : defaut 30 s, plage 10 s a
// 30 min (commit 1bab779 "Add configurable cloud transcription timeout").

import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Row, Section } from "@/components/ui/section";
import { api } from "@/lib/tauri";

const MIN_SECS = 10;
const MAX_SECS = 30 * 60;

export function CloudTimeoutPanel() {
  const { t } = useTranslation();
  const [secs, setSecs] = useState<number>(30);

  useEffect(() => {
    api.getCloudTranscriptionTimeout().then(setSecs).catch(console.error);
  }, []);

  async function save(next: number) {
    const clamped = Math.round(
      Math.max(MIN_SECS, Math.min(MAX_SECS, Number.isFinite(next) ? next : 30)),
    );
    setSecs(clamped);
    try {
      await api.setCloudTranscriptionTimeout(clamped);
    } catch (e) {
      console.error(e);
    }
  }

  return (
    <Section title={t("speech.cloudTimeoutTitle")}>
      <Row
        label={t("speech.cloudTimeoutDesc")}
        description={`${MIN_SECS} - ${MAX_SECS} ${t("common.seconds")}`}
      >
        <input
          type="number"
          min={MIN_SECS}
          max={MAX_SECS}
          step={5}
          aria-label={t("speech.cloudTimeoutTitle")}
          value={secs}
          onChange={(e) => setSecs(Number.isFinite(e.target.valueAsNumber) ? e.target.valueAsNumber : MIN_SECS)}
          onBlur={(e) => save(e.target.valueAsNumber)}
          className="h-[34px] w-24 rounded-sm border-[1.5px] border-input bg-background px-3 text-sm"
        />
        <span className="text-xs text-muted-foreground">{t("common.seconds")}</span>
      </Row>
    </Section>
  );
}
