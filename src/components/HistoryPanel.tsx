// Parlato: History page, rebuilt to the Workbench design (docs/design/v1,
// screen "history"): search + filter toolbar, dictations grouped by day as
// cards, a details drawer per card, and a select mode for export / delete.
//
// Not wired yet (no backend command): replaying a recording and
// transcribing an item again with another model. The design shows both.

import { useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { useTranslation } from "react-i18next";
import {
  Check,
  ChevronDown,
  Copy,
  Download,
  Loader2,
  Mic,
  Search,
  ShieldCheck,
  Sparkles,
  Trash2,
  TriangleAlert,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Segmented } from "@/components/ui/section";
import { cn } from "@/lib/utils";
import { useCopyButton } from "@/hooks/useCopyButton";
import { translateError } from "@/lib/translateError";
import { api, type TranscriptionRecord } from "@/lib/tauri";

/// Si le texte est une erreur formate "Transcription Failed: PARLA_ERR:...",
/// extrait le code et le traduit. Sinon retourne le texte tel quel.
function localizeFailureText(t: ReturnType<typeof useTranslation>["t"], raw: string): string {
  const prefix = "Transcription Failed: ";
  if (!raw.startsWith(prefix)) return raw;
  const code = raw.slice(prefix.length);
  return translateError(t, code);
}

const PAGE_SIZE = 20;

type Filter = "all" | "ai" | "failed";

function formatClock(secs: number | null): string | null {
  if (!secs || secs <= 0) return null;
  const total = Math.round(secs);
  return `${Math.floor(total / 60)}:${(total % 60).toString().padStart(2, "0")}`;
}

function formatSecs(secs: number | null): string | null {
  if (!secs || secs <= 0) return null;
  return `${secs.toFixed(1)} s`;
}

function formatTime(iso: string): string {
  try {
    return new Date(iso).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  } catch {
    return iso;
  }
}

function dayKey(iso: string): string {
  const d = new Date(iso);
  return `${d.getFullYear()}-${d.getMonth()}-${d.getDate()}`;
}

function dayLabel(iso: string, t: ReturnType<typeof useTranslation>["t"]): string {
  const d = new Date(iso);
  const today = new Date();
  const yesterday = new Date();
  yesterday.setDate(today.getDate() - 1);
  if (dayKey(iso) === dayKey(today.toISOString())) return t("hist.today");
  if (dayKey(iso) === dayKey(yesterday.toISOString())) return t("hist.yesterday");
  return d.toLocaleDateString([], { weekday: "long", month: "long", day: "numeric" });
}

export function HistoryPanel({ onOpenSettings }: { onOpenSettings?: () => void }) {
  const { t } = useTranslation();
  const [items, setItems] = useState<TranscriptionRecord[]>([]);
  const [loading, setLoading] = useState(false);
  const [search, setSearch] = useState("");
  const [filter, setFilter] = useState<Filter>("all");
  const [selectMode, setSelectMode] = useState(false);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [expanded, setExpanded] = useState<string | null>(null);
  const [count, setCount] = useState<number | null>(null);
  const [hasMore, setHasMore] = useState(false);

  useEffect(() => {
    refresh();

    const unCreated = listen<string>("history:created", () => refresh());
    const unUpdated = listen<string>("history:updated", () => refresh());
    const unCleaned = listen("history:cleaned", () => refresh());
    return () => {
      unCreated.then((fn) => fn());
      unUpdated.then((fn) => fn());
      unCleaned.then((fn) => fn());
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    const t = window.setTimeout(() => refresh(), 200);
    return () => window.clearTimeout(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [search]);

  async function refresh() {
    setLoading(true);
    try {
      const [rows, total] = await Promise.all([
        api.listHistory({ limit: PAGE_SIZE, search: search || null }),
        api.countHistory(),
      ]);
      setItems(rows);
      setCount(total);
      setHasMore(rows.length === PAGE_SIZE);
    } catch (e) {
      console.error(e);
    } finally {
      setLoading(false);
    }
  }

  async function loadMore() {
    if (items.length === 0) return;
    const last = items[items.length - 1];
    setLoading(true);
    try {
      const rows = await api.listHistory({
        limit: PAGE_SIZE,
        before: last.timestamp,
        search: search || null,
      });
      setItems((prev) => [...prev, ...rows]);
      setHasMore(rows.length === PAGE_SIZE);
    } catch (e) {
      console.error(e);
    } finally {
      setLoading(false);
    }
  }

  function toggleSelect(id: string) {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  function toggleSelectMode() {
    setSelectMode((on) => !on);
    setSelected(new Set());
  }

  async function deleteIds(ids: string[], question: string) {
    if (ids.length === 0) return;
    if (!confirm(question)) return;
    for (const id of ids) {
      try {
        await api.deleteHistoryItem(id);
      } catch (e) {
        console.error(e);
      }
    }
    setSelected(new Set());
    await refresh();
  }

  async function exportSelected() {
    const ids = selected.size > 0 ? [...selected] : visible.map((i) => i.id);
    try {
      await api.exportHistoryCsv(ids);
    } catch (e) {
      console.error(e);
    }
  }

  const visible = useMemo(
    () =>
      items.filter(
        (i) =>
          filter === "all" ||
          (filter === "ai" && !!i.enhanced_text) ||
          (filter === "failed" && i.status === "failed"),
      ),
    [items, filter],
  );

  const groups = useMemo(() => {
    const out: { key: string; label: string; items: TranscriptionRecord[] }[] = [];
    for (const it of visible) {
      const key = dayKey(it.timestamp);
      const last = out[out.length - 1];
      if (last && last.key === key) last.items.push(it);
      else out.push({ key, label: dayLabel(it.timestamp, t), items: [it] });
    }
    return out;
  }, [visible, t]);

  const selectedCount = selected.size;

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-center gap-2.5">
        <label className="flex h-9 min-w-[220px] flex-1 items-center gap-2 rounded-sm border-[1.5px] border-input bg-card px-3 text-muted-foreground focus-within:border-foreground">
          <Search className="h-4 w-4 flex-none" />
          <input
            type="text"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder={t("hist.search")}
            aria-label={t("hist.search")}
            className="h-full w-full min-w-0 bg-transparent text-sm text-foreground outline-none placeholder:text-muted-foreground"
          />
        </label>
        <Segmented<Filter>
          value={filter}
          onChange={setFilter}
          options={[
            { value: "all", label: t("hist.filterAll") },
            { value: "ai", label: t("hist.filterAi") },
            { value: "failed", label: t("hist.filterFailed") },
          ]}
        />
        <Button size="sm" variant="ghost" onClick={toggleSelectMode} disabled={items.length === 0}>
          {selectMode ? t("hist.done") : t("hist.select")}
        </Button>
      </div>

      {selectMode && (
        <div className="flex flex-wrap items-center gap-2.5 rounded-lg bg-foreground py-2 pr-2 pl-3.5 text-background">
          <span className="flex-1 text-[13px] font-semibold">
            {selectedCount > 0 ? t("hist.selected", { count: selectedCount }) : t("hist.noneSelected")}
          </span>
          <button
            type="button"
            onClick={() => setSelected(new Set(visible.map((i) => i.id)))}
            className="h-[30px] rounded-sm px-3 text-[13px] font-semibold hover:bg-background/15"
          >
            {t("hist.selectAll")}
          </button>
          <button
            type="button"
            onClick={exportSelected}
            className="flex h-[30px] items-center gap-1.5 rounded-sm px-3 text-[13px] font-semibold hover:bg-background/15"
          >
            <Download className="h-3.5 w-3.5" />
            {t("hist.export")}
          </button>
          <button
            type="button"
            disabled={selectedCount === 0}
            onClick={() =>
              deleteIds([...selected], t("history.confirmDeleteCount", { count: selectedCount }))
            }
            className="flex h-[30px] items-center gap-1.5 rounded-sm bg-destructive px-3 text-[13px] font-semibold text-white disabled:opacity-50"
          >
            <Trash2 className="h-3.5 w-3.5" />
            {t("hist.delete")}
          </button>
        </div>
      )}

      {loading && items.length === 0 && (
        <div className="flex justify-center py-8">
          <Loader2 className="h-5 w-5 animate-spin text-muted-foreground" />
        </div>
      )}

      {!loading && items.length === 0 && (
        <div className="flex flex-col items-center gap-1 rounded-lg border-[1.5px] border-dashed border-input bg-card px-6 py-10 text-center">
          <Mic className="mb-1 h-5 w-5 text-muted-foreground" />
          <span className="font-display font-bold">{search ? t("hist.noMatch") : t("hist.emptyTitle")}</span>
          {!search && <span className="text-[13px] text-muted-foreground">{t("hist.emptyDesc")}</span>}
        </div>
      )}

      {items.length > 0 && visible.length === 0 && (
        <p className="py-6 text-center text-sm text-muted-foreground">{t("hist.noMatch")}</p>
      )}

      {groups.map((g) => (
        <section key={g.key} className="mt-2 flex flex-col gap-2.5">
          <h3 className="text-[13px] font-semibold capitalize">{g.label}</h3>
          {g.items.map((it) => (
            <HistoryCard
              key={it.id}
              item={it}
              selectMode={selectMode}
              checked={selected.has(it.id)}
              onCheck={() => toggleSelect(it.id)}
              open={expanded === it.id}
              onToggle={() => setExpanded(expanded === it.id ? null : it.id)}
              onDelete={() => deleteIds([it.id], t("hist.confirmDeleteOne"))}
            />
          ))}
        </section>
      ))}

      {hasMore && (
        <Button
          size="sm"
          variant="outline"
          className="self-center rounded-full px-4"
          onClick={loadMore}
          disabled={loading}
        >
          {loading ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <ChevronDown className="h-3.5 w-3.5" />}
          {t("hist.loadMore")}
        </Button>
      )}

      <p className="flex flex-wrap items-center gap-x-1.5 gap-y-1 text-xs text-muted-foreground">
        <ShieldCheck className="h-3.5 w-3.5" />
        <span>{t("hist.privacy")}</span>
        {count !== null && count > 0 && <span>{t("hist.count", { count })}</span>}
        {onOpenSettings && (
          <button
            type="button"
            onClick={onOpenSettings}
            className="font-semibold text-foreground underline underline-offset-[3px]"
          >
            {t("hist.privacyLink")}
          </button>
        )}
      </p>
    </div>
  );
}

function HistoryCard({
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
          <div className="flex justify-end">
            <button
              type="button"
              onClick={onDelete}
              className="flex h-7 items-center gap-1.5 rounded-sm px-2.5 text-xs font-semibold text-destructive hover:bg-accent"
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
