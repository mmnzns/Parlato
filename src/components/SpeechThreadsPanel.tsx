// Parlato : nombre de threads CPU utilises par Whisper et Parakeet.
//
// Automatique par defaut (cf `auto_threads` dans transcription/whisper.rs,
// choisi d'apres whisper_threads_smoke et parakeet_threads_smoke). Le reglage manuel sert aux utilisateurs qui
// veulent l'ajuster a leur machine.

import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Row, Section, selectClass } from "@/components/ui/section";
import { Button } from "@/components/ui/button";
import { api } from "@/lib/tauri";

export function SpeechThreadsPanel() {
  const { t } = useTranslation();
  const [info, setInfo] = useState<{ value: number; auto: number; max: number } | null>(null);

  useEffect(() => {
    api.getSpeechThreads().then(setInfo).catch(console.error);
  }, []);

  async function change(value: number) {
    if (!info) return;
    setInfo({ ...info, value });
    try {
      await api.setSpeechThreads(value);
    } catch (e) {
      console.error(e);
    }
  }

  if (!info) return null;

  return (
    <Section
      title={t("speech.threadsTitle")}
      action={
        info.value !== 0 && (
          <Button variant="outline" size="sm" onClick={() => change(0)}>
            {t("speech.threadsReset")}
          </Button>
        )
      }
    >
      <Row label={t("speech.threadsLabel")} description={t("speech.threadsDesc")}>
        <select
          aria-label={t("speech.threadsLabel")}
          value={info.value}
          onChange={(e) => change(Number(e.target.value))}
          className={selectClass}
        >
          <option value={0}>{t("speech.threadsAuto", { count: info.auto })}</option>
          {Array.from({ length: info.max }, (_, i) => i + 1).map((n) => (
            <option key={n} value={n}>
              {n}
            </option>
          ))}
        </select>
      </Row>
    </Section>
  );
}
