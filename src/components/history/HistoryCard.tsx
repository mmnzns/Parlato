// Parlato: one dictation card of the History screen, with its details drawer.

import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Check, ChevronDown, Copy, Loader2, Mic, Sparkles, Trash2, TriangleAlert } from "lucide-react";
import { Segmented } from "@/components/ui/section";
import { cn } from "@/lib/utils";
import { useCopyButton } from "@/hooks/useCopyButton";
import type { TranscriptionRecord } from "@/lib/tauri";
import { AudioPlayer, RetranscribeButton } from "@/components/history/DrawerControls";
import {
  formatClock,
  formatSecs,
  formatTime,
  localizeFailureText,
} from "@/components/history/utils";

export function HistoryCard({
  item: it,
  selectMode,
  checked,
  onCheck,
  open,
  onToggle,
  onDelete,
}: {
  item: TranscriptionRecord;
  selectMode: boolean;
  checked: boolean;
  onCheck: () => void;
  open: boolean;
  onToggle: () => void;
  onDelete: () => void;
}) {
  const { t } = useTranslation();
  const [tab, setTab] = useState<"clean" | "raw">("clean");
  const failed = it.status === "failed";
  const pending = it.status === "pending";
  const hasAi = !!it.enhanced_text;
  const main = it.enhanced_text ?? it.text;
  const shown = open && hasAi && tab === "raw" ? it.text : main;
  const clock = formatClock(it.duration_sec);

  const took = [formatSecs(it.transcription_duration_sec), formatSecs(it.enhancement_duration_sec)]
    .filter(Boolean)
    .join(" + ");
  const details: [string, string][] = [
    [t("hist.detailModel"), it.transcription_model_name ?? "-"],
    [t("hist.detailAi"), it.ai_enhancement_model_name ?? t("hist.aiOff")],
    [t("hist.detailStyle"), it.prompt_name ?? "-"],
    [t("hist.detailTook"), took || "-"],
  ];
  if (it.power_mode_name) details.push([t("hist.detailMode"), it.power_mode_name]);
  if (it.language) details.push([t("hist.detailLanguage"), it.language]);

  return (
    <div
      className={cn(
        "rounded-lg border-[1.5px] bg-card transition-shadow",
        failed ? "border-destructive/60" : "border-edge",
        checked && "bg-accent shadow-[var(--sel-shadow)]",
      )}
    >
      <div className="flex items-start gap-4 px-5 pt-5 pb-[22px]">
        {selectMode && (
          <input
            type="checkbox"
            checked={checked}
            onChange={onCheck}
            aria-label={t("hist.select")}
            className="mt-px flex-none"
          />
        )}
        <div className="flex min-w-0 flex-1 flex-col gap-2.5">
          <div className="flex flex-wrap items-center gap-x-3.5 gap-y-1.5 text-xs leading-[18px] text-muted-foreground">
            {it.power_mode_name && (
              <span className="flex items-center gap-[7px] font-semibold text-foreground">
                <span className="h-2 w-2 rounded-full bg-highlight" />
                {it.power_mode_name}
              </span>
            )}
            <span className="font-mono">{formatTime(it.timestamp)}</span>
            {clock && (
              <span className="flex h-6 items-center gap-1.5 rounded-full border-[1.5px] border-input px-2 font-mono">
                <Mic className="h-3 w-3" />
                {clock}
              </span>
            )}
            {hasAi && (
              <span className="flex h-5 items-center gap-1 rounded-full bg-muted px-2 text-[11px] font-medium text-foreground">
                <Sparkles className="h-3 w-3" />
                {t("hist.aiBadge")}
              </span>
            )}
            {pending && (
              <span className="flex items-center gap-1">
                <Loader2 className="h-3 w-3 animate-spin" />
                {t("hist.pending")}
              </span>
            )}
          </div>

          {failed ? (
            <span className="flex items-start gap-2 text-[15px] leading-6 font-medium text-destructive">
              <TriangleAlert className="mt-1 h-4 w-4 flex-none" />
              <span>
                {t("hist.failed")}: {localizeFailureText(t, main)}
              </span>
            </span>
          ) : (
            <p
              className={cn(
                "max-w-[68ch] text-[15px] leading-6 text-pretty whitespace-pre-wrap",
                !open && "line-clamp-3",
              )}
            >
              {shown || t("history.emptyText")}
            </p>
          )}
        </div>

        <div className="flex flex-none gap-1.5">
          {!failed && <CopyButton text={shown} />}
          <button
            type="button"
            onClick={onToggle}
            aria-label={t("hist.details")}
            aria-expanded={open}
            className="flex h-[30px] w-[30px] items-center justify-center rounded-sm text-muted-foreground hover:bg-accent hover:text-foreground"
          >
            <ChevronDown className={cn("h-4 w-4 transition-transform", open && "rotate-180")} />
          </button>
        </div>
      </div>

      {open && (
        <div className="mx-5 mb-[22px] flex flex-col gap-3 border-t border-dashed pt-4">
          {hasAi && (
            <div className="self-start">
              <Segmented<"clean" | "raw">
                value={tab}
                onChange={setTab}
                options={[
                  { value: "clean", label: t("hist.tabClean") },
                  { value: "raw", label: t("hist.tabRaw") },
                ]}
              />
            </div>
          )}
          <dl className="grid grid-cols-2 gap-x-4 gap-y-2 sm:grid-cols-4">
            {details.map(([k, v]) => (
              <div key={k} className="flex min-w-0 flex-col gap-0.5">
                <dt className="text-[11px] leading-[14px] text-muted-foreground">{k}</dt>
                <dd className="truncate font-mono text-xs font-bold" title={v}>
                  {v}
                </dd>
              </div>
            ))}
          </dl>
          <div className="flex flex-wrap items-center gap-2">
            {it.audio_file_name ? (
              <>
                <AudioPlayer id={it.id} />
                <RetranscribeButton id={it.id} />
              </>
            ) : (
              <span className="text-xs text-muted-foreground">{t("hist.noAudio")}</span>
            )}
            <button
              type="button"
              onClick={onDelete}
              className="ml-auto flex h-7 items-center gap-1.5 rounded-sm px-2.5 text-xs font-semibold text-destructive hover:bg-accent"
            >
              <Trash2 className="h-3.5 w-3.5" />
              {t("hist.delete")}
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

function CopyButton({ text }: { text: string | null | undefined }) {
  const { t } = useTranslation();
  const { copied, copy } = useCopyButton();
  const Icon = copied ? Check : Copy;
  return (
    <button
      type="button"
      onClick={() => copy(text)}
      className={cn(
        "flex h-[30px] items-center gap-1.5 rounded-sm border-[1.5px] border-edge bg-card px-2.5 text-xs font-semibold hover:bg-accent",
        copied && "text-positive",
      )}
    >
      <Icon className="h-3.5 w-3.5" />
      {t("history.copy")}
    </button>
  );
}
