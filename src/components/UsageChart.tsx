// Parlato: Home statistics. Words per day for the last 14 days, or per week
// for the last 12 weeks, drawn with plain divs (no chart library) so it
// follows the theme tokens, scales with the window and stays readable to
// screen readers: the group carries the total, each bar its own label and is
// focusable to show its value.

import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { Segmented } from "@/components/ui/section";
import { api, type UsagePoint } from "@/lib/tauri";
import { cn } from "@/lib/utils";

type Range = "day" | "week";

const DAYS = 14;
const WEEKS = 12;

/** "2026-11-13" as a local date (new Date("...") would read it as UTC). */
function parseDay(iso: string): Date {
  const [y, m, d] = iso.split("-").map(Number);
  return new Date(y, (m || 1) - 1, d || 1);
}

export function UsageChart() {
  const { t, i18n } = useTranslation();
  const [range, setRange] = useState<Range>("day");
  const [points, setPoints] = useState<UsagePoint[] | null>(null);

  useEffect(() => {
    let alive = true;
    const load = () =>
      (range === "day" ? api.getUsageByDay(DAYS) : api.getUsageByWeek(WEEKS))
        .then((p) => alive && setPoints(p))
        .catch(console.error);
    load();
    const un1 = listen("history:updated", load);
    const un2 = listen("history:created", load);
    const un3 = listen("history:cleaned", load);
    return () => {
      alive = false;
      un1.then((fn) => fn());
      un2.then((fn) => fn());
      un3.then((fn) => fn());
    };
  }, [range]);

  const lang = i18n.language;
  const shortDate = (iso: string) =>
    parseDay(iso).toLocaleDateString(lang, { month: "short", day: "numeric" });
  const longDate = (iso: string) =>
    parseDay(iso).toLocaleDateString(lang, { weekday: "long", month: "long", day: "numeric" });

  const data = points ?? [];
  const total = data.reduce((sum, p) => sum + p.words, 0);
  const max = Math.max(1, ...data.map((p) => p.words));
  const empty = points !== null && total === 0;

  const title = range === "day" ? t("dashboard.chartDayTitle") : t("dashboard.chartWeekTitle");
  const groupLabel = t(range === "day" ? "dashboard.chartDayLabel" : "dashboard.chartWeekLabel", {
    n: data.length,
    total: total.toLocaleString(lang),
  });
  const barLabel = (p: UsagePoint) =>
    range === "day"
      ? t("dashboard.chartBarDay", { date: longDate(p.date), count: p.words, words: p.words.toLocaleString(lang) })
      : t("dashboard.chartBarWeek", { date: shortDate(p.date), count: p.words, words: p.words.toLocaleString(lang) });

  return (
    <div className="flex flex-col gap-3 rounded-lg border-[1.5px] border-edge bg-card px-[18px] py-4">
      <div className="flex items-center justify-between gap-3">
        <div className="flex min-w-0 flex-col gap-0.5">
          <span className="font-mono text-[11px] text-muted-foreground">{title}</span>
          <span className="text-xs leading-4 text-muted-foreground">
            {empty
              ? t("dashboard.chartEmpty")
              : t(range === "day" ? "dashboard.chartDayTotal" : "dashboard.chartWeekTotal", {
                  n: data.length,
                  total: total.toLocaleString(lang),
                })}
          </span>
        </div>
        <Segmented<Range>
          value={range}
          onChange={setRange}
          options={[
            { value: "day", label: t("dashboard.chartDays") },
            { value: "week", label: t("dashboard.chartWeeks") },
          ]}
        />
      </div>

      {empty ? (
        <div className="flex h-[104px] items-center justify-center rounded-sm bg-muted text-sm text-muted-foreground">
          {t("dashboard.chartEmpty")}
        </div>
      ) : (
        <div role="group" aria-label={groupLabel} className="flex h-[104px] items-stretch gap-1.5">
          {data.map((p, i) => {
            const last = i === data.length - 1;
            // Week view has room for a label under every other bar only.
            const showLabel = range === "day" || (data.length - 1 - i) % 2 === 0;
            const pct = p.words === 0 ? 0 : Math.max(4, Math.round((p.words / max) * 100));
            return (
              <div key={p.date} className="group relative flex min-w-0 flex-1 flex-col items-center gap-1">
                <div
                  tabIndex={0}
                  role="img"
                  aria-label={barLabel(p)}
                  title={barLabel(p)}
                  className="relative flex w-full flex-1 items-end rounded-sm border-b border-border outline-none focus-visible:ring-2 focus-visible:ring-ring"
                >
                  <span
                    className={cn(
                      "pointer-events-none absolute bottom-full left-1/2 z-10 mb-1 hidden -translate-x-1/2 rounded-sm border border-edge bg-popover px-1.5 py-0.5 font-mono text-[11px] whitespace-nowrap text-popover-foreground",
                      "group-hover:block group-focus-within:block",
                    )}
                  >
                    {p.words.toLocaleString(lang)}
                  </span>
                  <div
                    className={cn(
                      "w-full rounded-t-[3px] border-[1.5px] border-b-0",
                      p.words === 0 ? "h-0 border-0" : "border-edge bg-primary",
                      last && p.words > 0 && "shadow-[2px_2px_0_var(--edge)]",
                    )}
                    style={{ height: `${pct}%` }}
                  />
                </div>
                <span
                  aria-hidden="true"
                  className={cn(
                    "h-3.5 font-mono text-[10px] leading-[14px] whitespace-nowrap text-muted-foreground",
                    last && "font-semibold text-foreground",
                    !showLabel && "invisible",
                  )}
                >
                  {range === "day" ? parseDay(p.date).getDate() : shortDate(p.date)}
                </span>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
