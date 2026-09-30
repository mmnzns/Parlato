// Panneau de performance par modele.
//
// Reference VoiceInk : Views/Metrics/ModelPerformancePanel.swift.
// Filtre temporel (7j / 30j / cette annee / tout) + 2 sections (modeles
// de transcription / d'enhancement) en grille de tuiles. Les metriques
// sont agregees cote backend a partir de la table transcriptions.

import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { Block, Section, selectClass } from "@/components/ui/section";
import { cn } from "@/lib/utils";
import {
  api,
  type EnhancementModelMetric,
  type MetricsPeriod,
  type ModelPerformanceMetrics,
  type TranscriptionModelMetric,
} from "@/lib/tauri";

export function ModelPerformancePanel() {
  const { t } = useTranslation();
  const [period, setPeriod] = useState<MetricsPeriod>("last7_days");
  const [data, setData] = useState<ModelPerformanceMetrics | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    refresh();
    const un1 = listen("history:updated", () => refresh());
    const un2 = listen("history:created", () => refresh());
    return () => {
      un1.then((fn) => fn());
      un2.then((fn) => fn());
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [period]);

  async function refresh() {
    try {
      const result = await api.getModelPerformanceMetrics(period);
      setData(result);
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }

  const isEmpty =
    !data ||
    (data.transcription_models.length === 0 && data.enhancement_models.length === 0);

  // Parlato: Workbench section (docs/design/v1, "How fast your models are").
  return (
    <Section
      title={t("speech.perfTitle")}
      description={t("speech.perfDesc")}
      action={
        <select
          aria-label={t("speech.perfTitle")}
          value={period}
          onChange={(e) => setPeriod(e.target.value as MetricsPeriod)}
          className={cn(selectClass, "min-w-0")}
        >
          <option value="last7_days">{t("modelPerformance.period.last7Days")}</option>
          <option value="last30_days">{t("modelPerformance.period.last30Days")}</option>
          <option value="this_year">{t("modelPerformance.period.thisYear")}</option>
          <option value="all_time">{t("modelPerformance.period.allTime")}</option>
        </select>
      }
    >
      {error && (
        <Block className="py-3 text-xs text-destructive">{t("common.errorPrefix", { message: error })}</Block>
      )}
      {isEmpty ? (
        <Block className="text-sm text-muted-foreground">{t("modelPerformance.empty")}</Block>
      ) : (
        <>
          {data!.transcription_models.map((m) => (
            <TranscriptionTile key={m.name} metric={m} />
          ))}
          {data!.enhancement_models.map((m) => (
            <EnhancementTile key={m.name} metric={m} />
          ))}
        </>
      )}
    </Section>
  );
}

function TranscriptionTile({ metric }: { metric: TranscriptionModelMetric }) {
  const { t } = useTranslation();
  const speedFactor =
    metric.total_processing_sec > 0 ? metric.total_audio_sec / metric.total_processing_sec : 0;
  const avgProcessing =
    metric.session_count > 0 ? metric.total_processing_sec / metric.session_count : 0;
  const fasterThanRealtime = speedFactor >= 1.0;

  return (
    <div className="flex items-center gap-5 px-5 py-[13px]">
      <div className="flex min-w-0 flex-1 flex-col gap-0.5">
        <span className="truncate font-medium">{metric.name}</span>
        <span className="text-[13px] text-muted-foreground">
          {t("speech.dictations", { count: metric.session_count })} ·{" "}
          {t("speech.each", { secs: avgProcessing.toFixed(1) })}
        </span>
      </div>
      <div className="flex flex-none flex-col items-end">
        <span className="font-display text-2xl leading-7 font-extrabold">{speedFactor.toFixed(1)}×</span>
        <span className="text-[11px] text-muted-foreground">
          {fasterThanRealtime ? t("speech.faster") : t("speech.slower")}
        </span>
      </div>
    </div>
  );
}

function EnhancementTile({ metric }: { metric: EnhancementModelMetric }) {
  const { t } = useTranslation();
  const avgDuration = metric.session_count > 0 ? metric.total_duration_sec / metric.session_count : 0;
  return (
    <div className="flex items-center gap-5 px-5 py-[13px]">
      <div className="flex min-w-0 flex-1 flex-col gap-0.5">
        <span className="truncate font-medium">{metric.name}</span>
        <span className="text-[13px] text-muted-foreground">
          {t("speech.aiTitle")} · {t("speech.cleanups", { count: metric.session_count })}
        </span>
      </div>
      <div className="flex flex-none flex-col items-end">
        <span className="font-display text-2xl leading-7 font-extrabold">{avgDuration.toFixed(1)} s</span>
        <span className="text-[11px] text-muted-foreground">{t("modelPerformance.avgEnhancement")}</span>
      </div>
    </div>
  );
}
