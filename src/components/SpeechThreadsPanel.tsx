// Parlato : nombre de threads CPU utilises par Whisper et Parakeet, et
// duree pendant laquelle les modeles restent charges apres une dictee.
//
// Automatique par defaut (cf `auto_threads` dans transcription/whisper.rs,
// choisi d'apres whisper_threads_smoke et parakeet_threads_smoke). Le reglage manuel sert aux utilisateurs qui
// veulent l'ajuster a leur machine.
//
// "Utiliser la carte graphique" (reglage `use_gpu`, un seul pour la parole et
// l'IA locale, avec repli automatique sur le processeur) : affiche pour tous
// les moteurs locaux.

import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Row, Section, Switch, selectClass } from "@/components/ui/section";
import { Button } from "@/components/ui/button";
import { api, type UseGpu } from "@/lib/tauri";

export function SpeechThreadsPanel() {
  const { t } = useTranslation();
  const [info, setInfo] = useState<{ value: number; auto: number; max: number } | null>(null);
  const [keep, setKeep] = useState<{ value: number; recommended: number } | null>(null);
  const [gpu, setGpu] = useState<UseGpu | null>(null);

  useEffect(() => {
    api.getSpeechThreads().then(setInfo).catch(console.error);
    api.getModelKeepAlive().then(setKeep).catch(console.error);
  }, []);

  // The card search can take a few seconds after launch (Windows): ask again
  // until it has answered.
  useEffect(() => {
    let timer: ReturnType<typeof setTimeout> | undefined;
    let cancelled = false;
    const load = () =>
      api
        .getUseGpu()
        .then((g) => {
          if (cancelled) return;
          setGpu(g);
          if (g.default_reason === "checking") timer = setTimeout(load, 1500);
        })
        .catch(console.error);
    load();
    return () => {
      cancelled = true;
      if (timer) clearTimeout(timer);
    };
  }, []);

  async function changeGpu(enabled: boolean) {
    if (!gpu) return;
    setGpu({ ...gpu, enabled, default_reason: "set_by_person" });
    try {
      await api.setUseGpu(enabled);
      window.dispatchEvent(new Event("use-gpu-changed"));
    } catch (e) {
      console.error(e);
    }
  }

  async function changeKeep(value: number) {
    if (!keep) return;
    setKeep({ ...keep, value });
    try {
      await api.setModelKeepAlive(value);
    } catch (e) {
      console.error(e);
    }
  }

  function reset() {
    change(0);
    if (keep) changeKeep(keep.recommended);
  }

  async function change(value: number) {
    if (!info) return;
    setInfo({ ...info, value });
    try {
      await api.setSpeechThreads(value);
    } catch (e) {
      console.error(e);
    }
  }

  if (!info || !keep) return null;
  const showGpu = !!gpu && gpu.platform !== "other";
  const custom = info.value !== 0 || keep.value !== keep.recommended;
  // Choices in seconds; a value saved by hand elsewhere is kept as is.
  const keepChoices = Array.from(new Set([0, 60, 300, 900, 3600, keep.value])).sort(
    (a, b) => a - b,
  );
  const keepLabel = (secs: number) => {
    const label =
      secs === 0
        ? t("speech.keepAliveNever")
        : secs % 3600 === 0
          ? t("speech.keepAliveHours", { count: secs / 3600 })
          : t("speech.keepAliveMinutes", { count: Math.round(secs / 60) });
    return secs === keep.recommended ? t("speech.keepAliveRecommended", { label }) : label;
  };

  return (
    <Section
      title={t("speech.threadsTitle")}
      action={
        custom && (
          <Button variant="outline" size="sm" onClick={reset}>
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
      {showGpu && gpu && (
        <Row
          label={t("speech.gpuLabel")}
          description={
            <>
              {t(gpu.platform === "macos" ? "speech.gpuDescMac" : "speech.gpuDescWindows")}
              {gpu.platform === "windows" && (
                <span className="mt-1 block">
                  {gpu.card_name
                    ? t("speech.gpuCard", { name: gpu.card_name })
                    : gpu.default_reason === "checking"
                      ? t("speech.gpuChecking")
                      : t("speech.gpuNoCard")}
                </span>
              )}
              {gpu.enabled && gpu.failed && (
                <span className="mt-1 block font-medium text-foreground">
                  {t("speech.gpuFailed")}
                </span>
              )}
            </>
          }
          htmlFor="use-gpu"
        >
          <Switch
            id="use-gpu"
            checked={gpu.enabled}
            onChange={changeGpu}
            label={t("speech.gpuLabel")}
          />
        </Row>
      )}
      <Row label={t("speech.keepAliveLabel")} description={t("speech.keepAliveDesc")}>
        <select
          aria-label={t("speech.keepAliveLabel")}
          value={keep.value}
          onChange={(e) => changeKeep(Number(e.target.value))}
          className={selectClass}
        >
          {keepChoices.map((secs) => (
            <option key={secs} value={secs}>
              {keepLabel(secs)}
            </option>
          ))}
        </select>
      </Row>
    </Section>
  );
}
