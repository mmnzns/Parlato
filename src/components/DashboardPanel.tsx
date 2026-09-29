// Dashboard : hero + 4 metric cards (sessions, mots, WPM, frappes economisees).
//
// Reference VoiceInk Views/Metrics/MetricsContent.swift + MetricsView.swift :
// LazyVGrid .adaptive(minimum: 240) de 4 MetricCards (Sessions, Words,
// WPM, Keystrokes Saved). Hero gradient accent avec "time saved".
// Subtitle : "Dictated N words across K sessions."
//
// Parlato: rebuilt as the Workbench "Home" screen (docs/design/v1):
// [01] how to dictate + practice box, [02] your setup, [03] so far,
// [04] recent. Metrics are computed exactly as before.

import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import {
  ArrowRight,
  AudioLines,
  Check,
  CircleCheck,
  CircleDashed,
  Copy,
  Keyboard,
  Mic,
  Sparkles,
} from "lucide-react";
import type { LucideIcon } from "lucide-react";
import type { View } from "@/components/Sidebar";
import { useCopyButton } from "@/hooks/useCopyButton";
import { useHotkeyLabel } from "@/hooks/useHotkeyLabel";
import { api, type HotkeyMode, type TranscriptionRecord } from "@/lib/tauri";
import { cn } from "@/lib/utils";

type Metrics = {
  sessions: number;
  words: number;
  wpm: number;
  keystrokesSaved: number;
  timeSavedMinutes: number;
};

// Typing speed reference used below: 40 WPM.
const TYPING_WPM = 40;
const LOCAL_LLM_PROVIDERS = new Set(["llamacpp", "ollama", "local_cli"]);

function computeMetrics(items: TranscriptionRecord[]): Metrics {
  let sessions = 0;
  let words = 0;
  let durationSec = 0;
  let charCount = 0;
  for (const it of items) {
    if (it.status !== "completed") continue;
    const txt = it.enhanced_text ?? it.text;
    if (!txt) continue;
    sessions += 1;
    const w = txt.trim().split(/\s+/).filter(Boolean).length;
    words += w;
    charCount += txt.length;
    durationSec += it.duration_sec ?? 0;
  }
  const wpm = durationSec > 0 ? Math.round(words / (durationSec / 60)) : 0;
  // Typing speed ref : 40 WPM soit ~200 cpm. Temps frappe estime =
  // charCount / 200 caracteres par minute.
  const typingMinutes = charCount / 200;
  const spokenMinutes = durationSec / 60;
  const timeSavedMinutes = Math.max(0, Math.round(typingMinutes - spokenMinutes));
  return {
    sessions,
    words,
    wpm,
    keystrokesSaved: charCount,
    timeSavedMinutes,
  };
}

function formatDuration(minutes: number): string {
  const h = Math.floor(minutes / 60);
  const m = minutes % 60;
  return h > 0 ? `${h}h ${m}m` : `${m}m`;
}

function formatTime(iso: string): string {
  const d = new Date(iso);
  const sameDay = d.toDateString() === new Date().toDateString();
  return sameDay
    ? d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })
    : d.toLocaleDateString([], { month: "short", day: "numeric" });
}

type SetupState = {
  mic: string | null;
  model: string | null;
  aiEnabled: boolean;
  aiLocal: boolean;
  aiModel: string | null;
};

async function loadSetup(t: (k: string) => string): Promise<SetupState> {
  const [devices, selectedDevice, source, aiEnabled, llm] = await Promise.all([
    api.listAudioDevices().catch(() => []),
    api.getSelectedInputDevice().catch(() => null),
    api.getTranscriptionSource().catch(() => null),
    api.getEnhancementEnabled().catch(() => false),
    api.getLlmSelection().catch(() => null),
  ]);

  let mic: string | null = null;
  if (devices.length > 0) {
    mic =
      selectedDevice && devices.some((d) => d.name === selectedDevice)
        ? selectedDevice
        : t("dashboard.micDefault");
  }

  let model: string | null = null;
  if (source?.kind === "local" && source.whisper_model_id) {
    const models = await api.listWhisperModels().catch(() => []);
    model = models.find((m) => m.id === source.whisper_model_id)?.display_name ?? source.whisper_model_id;
  } else if (source?.kind === "parakeet" && source.parakeet_model_id) {
    const models = await api.listParakeetModels().catch(() => []);
    model = models.find((m) => m.id === source.parakeet_model_id)?.display_name ?? source.parakeet_model_id;
  } else if (source?.kind === "cloud" && source.cloud_provider) {
    const providers = await api.listCloudProviders().catch(() => []);
    const name = providers.find((p) => p.id === source.cloud_provider)?.display_name ?? source.cloud_provider;
    model = source.cloud_model ? `${name} · ${source.cloud_model}` : name;
  }

  return {
    mic,
    model,
    aiEnabled,
    aiLocal: !!llm && LOCAL_LLM_PROVIDERS.has(llm.provider_id),
    aiModel: llm?.model ?? null,
  };
}

const MODE_KEYS: Record<HotkeyMode, string> = {
  pushToTalk: "dashboard.modeHold",
  toggle: "dashboard.modeToggle",
  hybrid: "dashboard.modeHybrid",
};

function Kbd({ children }: { children: React.ReactNode }) {
  return (
    <kbd className="mx-0.5 inline-block rounded-[3px] border border-b-2 border-input bg-background px-[7px] font-mono text-xs leading-[18px]">
      {children}
    </kbd>
  );
}

function SectionTitle({ index, title }: { index: string; title: string }) {
  return (
    <div className="flex items-baseline gap-2">
      <span className="font-mono text-[11px] text-muted-foreground">[{index}]</span>
      <h2 className="font-display text-base leading-5 font-bold">{title}</h2>
    </div>
  );
}

export function DashboardPanel({ onNavigate }: { onNavigate: (v: View) => void }) {
  const { t } = useTranslation();
  const [metrics, setMetrics] = useState<Metrics | null>(null);
  const [recent, setRecent] = useState<TranscriptionRecord[]>([]);
  const [setup, setSetup] = useState<SetupState | null>(null);
  const { label: hotkeyLabel, mode } = useHotkeyLabel();

  useEffect(() => {
    load();
    loadSetup(t).then(setSetup).catch(console.error);
    const un1 = listen("history:updated", () => load());
    const un2 = listen("history:created", () => load());
    return () => {
      un1.then((fn) => fn());
      un2.then((fn) => fn());
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  async function load() {
    try {
      const items = await api.listHistory({ limit: 200 });
      setMetrics(computeMetrics(items));
      setRecent(items.filter((it) => it.status === "completed" && (it.enhanced_text ?? it.text)).slice(0, 3));
    } catch (e) {
      console.error(e);
    }
  }

  const setupCards: {
    icon: LucideIcon;
    label: string;
    value: string;
    ok: boolean;
    optional?: boolean;
    go: View;
  }[] = [
    {
      icon: Mic,
      label: t("dashboard.setupMic"),
      value: setup?.mic ?? t("dashboard.micNone"),
      ok: !!setup?.mic,
      go: "audio",
    },
    {
      icon: AudioLines,
      label: t("dashboard.setupModel"),
      value: setup?.model ?? t("dashboard.statusMissing"),
      ok: !!setup?.model,
      go: "models",
    },
    {
      icon: Keyboard,
      label: t("dashboard.setupShortcut"),
      value: hotkeyLabel
        ? mode
          ? `${hotkeyLabel} · ${t(MODE_KEYS[mode])}`
          : hotkeyLabel
        : t("dashboard.statusMissing"),
      ok: !!hotkeyLabel,
      go: "audio",
    },
    {
      icon: Sparkles,
      label: t("dashboard.setupAi"),
      value: !setup?.aiEnabled
        ? t("dashboard.aiOff")
        : setup.aiLocal
          ? t("dashboard.aiLocal")
          : t("dashboard.aiCloud", { model: setup.aiModel ?? "" }),
      ok: !!setup?.aiEnabled,
      optional: true,
      go: "enhancement",
    },
  ];

  const times = metrics && metrics.wpm > 0 ? Math.round((metrics.wpm / TYPING_WPM) * 10) / 10 : 0;

  return (
    <div className="flex flex-col gap-[22px]">
      {/* [01] how to dictate */}
      <section className="grid grid-cols-[minmax(0,1fr)_270px] overflow-hidden rounded-lg border-[1.5px] border-edge bg-card shadow-pop">
        <div className="flex flex-col gap-3.5 px-6 py-[22px]">
          <span className="font-mono text-[11px] text-muted-foreground">[01] {t("dashboard.howLabel")}</span>
          <h2 className="font-display text-2xl leading-7 font-extrabold tracking-[-0.035em]">
            {t("dashboard.howTitle")}
          </h2>
          <ol className="flex flex-col gap-2.5">
            {[
              t("dashboard.step1"),
              hotkeyLabel ? (
                <>
                  {t("dashboard.step2Before")} <Kbd>{hotkeyLabel}</Kbd> {t("dashboard.step2After")}
                </>
              ) : (
                `${t("dashboard.step2Before")} ... ${t("dashboard.step2After")}`
              ),
              t("dashboard.step3"),
            ].map((step, i) => (
              <li key={i} className="flex items-start gap-3">
                <span className="flex h-[22px] w-[22px] flex-none items-center justify-center rounded-full bg-highlight font-mono text-xs font-semibold text-[#141416]">
                  {i + 1}
                </span>
                <span className="min-w-0 flex-1">{step}</span>
              </li>
            ))}
          </ol>
        </div>
        <div className="flex flex-col gap-2 border-l bg-muted p-4">
          <span className="font-mono text-[11px] text-muted-foreground">{t("dashboard.practiceLabel")}</span>
          <textarea
            placeholder={
              hotkeyLabel
                ? t("dashboard.practicePlaceholder", { key: hotkeyLabel })
                : t("dashboard.practicePlaceholderNoKey")
            }
            className="min-h-[120px] flex-1 resize-none rounded-sm border-[1.5px] border-input bg-card p-3 text-sm"
          />
        </div>
      </section>

      {/* [02] your setup */}
      <section className="flex flex-col gap-2.5">
        <SectionTitle index="02" title={t("dashboard.setupTitle")} />
        <div className="grid grid-cols-4 gap-2.5">
          {setupCards.map((s) => {
            const Icon = s.icon;
            const StatusIcon = s.ok ? CircleCheck : CircleDashed;
            const status = s.ok
              ? t("dashboard.statusReady")
              : s.optional
                ? t("dashboard.statusOptional")
                : t("dashboard.statusMissing");
            return (
              <button
                key={s.label}
                type="button"
                onClick={() => onNavigate(s.go)}
                className="flex min-w-0 flex-col items-start gap-1.5 rounded-lg border-[1.5px] border-edge bg-card px-3.5 py-3 text-left transition-colors hover:bg-accent"
              >
                <span className="flex w-full items-center justify-between">
                  <Icon className="h-4 w-4 text-muted-foreground" />
                  <span
                    className={cn(
                      "flex items-center gap-1 text-xs font-medium",
                      s.ok ? "text-positive" : s.optional ? "text-muted-foreground" : "text-warn-foreground",
                    )}
                  >
                    <StatusIcon className="h-[13px] w-[13px]" />
                    {status}
                  </span>
                </span>
                <span className="text-xs leading-4 text-muted-foreground">{s.label}</span>
                <span className="w-full truncate text-sm leading-[18px] font-semibold">{s.value}</span>
              </button>
            );
          })}
        </div>
      </section>

      {/* [03] so far */}
      <section className="flex flex-col gap-2.5">
        <SectionTitle index="03" title={t("dashboard.soFarTitle")} />
        <div className="grid grid-cols-[minmax(0,1.3fr)_repeat(3,minmax(0,1fr))] gap-2.5">
          <div className="flex flex-col gap-1.5 rounded-lg border-[1.5px] border-edge bg-primary px-[18px] py-4 text-primary-foreground dark:border-primary">
            <span className="font-mono text-[11px]">{t("dashboard.timeSaved")}</span>
            <span className="font-display text-[38px] leading-10 font-extrabold tracking-[-0.035em] tabular-nums">
              {metrics ? formatDuration(metrics.timeSavedMinutes) : "-"}
            </span>
            <span className="text-xs leading-4">{t("dashboard.timeSavedCaption")}</span>
          </div>
          <Stat value={metrics?.sessions ?? 0} caption={t("dashboard.dictations")} />
          <Stat value={metrics?.words ?? 0} caption={t("dashboard.words")} />
          <Stat
            value={metrics?.wpm ?? 0}
            caption={times >= 1.5 ? t("dashboard.wpmVsTyping", { times }) : t("dashboard.wpm")}
          />
        </div>
      </section>

      {/* [04] recent */}
      <section className="flex flex-col gap-2.5">
        <div className="flex items-baseline justify-between gap-2">
          <SectionTitle index="04" title={t("dashboard.recentTitle")} />
          <button
            type="button"
            onClick={() => onNavigate("history")}
            className="flex items-center gap-1 text-[13px] font-semibold underline underline-offset-[3px]"
          >
            {t("dashboard.seeAllHistory")} <ArrowRight className="h-3.5 w-3.5" />
          </button>
        </div>
        <div className="overflow-hidden rounded-lg border-[1.5px] border-edge bg-card">
          {recent.length === 0 ? (
            <p className="px-4 py-3.5 text-sm text-muted-foreground">{t("dashboard.recentEmpty")}</p>
          ) : (
            recent.map((r, i) => <RecentRow key={r.id} record={r} divider={i > 0} />)
          )}
        </div>
      </section>
    </div>
  );
}

function Stat({ value, caption }: { value: number; caption: string }) {
  return (
    <div className="flex min-w-0 flex-col gap-1 rounded-lg border-[1.5px] border-edge bg-card px-[18px] py-4">
      <span className="font-display text-[26px] leading-[30px] font-bold tracking-[-0.02em] tabular-nums">
        {value.toLocaleString()}
      </span>
      <span className="text-xs leading-4 text-muted-foreground">{caption}</span>
    </div>
  );
}

function RecentRow({ record, divider }: { record: TranscriptionRecord; divider: boolean }) {
  const { t } = useTranslation();
  const { copied, copy } = useCopyButton();
  const text = record.enhanced_text ?? record.text;
  const context = record.power_mode_name
    ? `${record.power_mode_emoji ? record.power_mode_emoji + " " : ""}${record.power_mode_name}`
    : null;
  return (
    <div
      className={cn(
        "grid grid-cols-[52px_minmax(0,1fr)_auto_auto] items-center gap-3.5 py-2.5 pr-3.5 pl-4",
        divider && "border-t",
      )}
    >
      <span className="font-mono text-xs text-muted-foreground">{formatTime(record.timestamp)}</span>
      <span className="truncate">{text}</span>
      <span className="text-xs text-muted-foreground">{context}</span>
      <button
        type="button"
        onClick={() => copy(text)}
        className="flex h-7 items-center gap-1.5 rounded-sm border-[1.5px] border-edge bg-card px-2.5 text-xs font-semibold transition-colors hover:bg-accent"
      >
        {copied ? <Check className="h-[13px] w-[13px]" /> : <Copy className="h-[13px] w-[13px]" />}
        {copied ? t("dashboard.copied") : t("dashboard.copy")}
      </button>
    </div>
  );
}
