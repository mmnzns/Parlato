// Parlato: History page, rebuilt to the Workbench design (docs/design/v1,
// screen "history"): search + filter toolbar, dictations grouped by day as
// cards, a details drawer per card, and a select mode for export / delete.
// The drawer can replay the kept recording and transcribe it again with the
// current speech model (commands::history read_history_audio /
// retranscribe_history_item).


import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import { ChevronDown, Loader2, Mic, ShieldCheck } from "lucide-react";
import { Button } from "@/components/ui/button";
import type { TranscriptionRecord } from "@/lib/tauri";
import { HistoryCard } from "@/components/history/HistoryCard";
import { HistorySelectBar, HistoryToolbar } from "@/components/history/HistoryToolbar";
import { useHistoryList } from "@/components/history/useHistoryList";
import { dayKey, dayLabel } from "@/components/history/utils";

export function HistoryPanel({ onOpenSettings }: { onOpenSettings?: () => void }) {
  const { t } = useTranslation();
  const {
    items,
    loading,
    search,
    setSearch,
    filter,
    setFilter,
    selectMode,
    selected,
    setSelected,
    expanded,
    setExpanded,
    count,
    hasMore,
    exported,
    loadMore,
    toggleSelect,
    toggleSelectMode,
    deleteIds,
    exportSelected,
  } = useHistoryList();

  // Parlato: the filter is applied by the query (all dictations, not only
  // the loaded page).
  const visible = items;

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
      <HistoryToolbar
        search={search}
        onSearch={setSearch}
        filter={filter}
        onFilter={setFilter}
        selectMode={selectMode}
        onToggleSelectMode={toggleSelectMode}
        hasItems={items.length > 0}
      />

      {selectMode && (
        <HistorySelectBar
          selectedCount={selectedCount}
          shownCount={visible.length}
          onSelectAll={() => setSelected(new Set(visible.map((i) => i.id)))}
          onExport={exportSelected}
          onDelete={() =>
            deleteIds([...selected], t("history.confirmDeleteCount", { count: selectedCount }))
          }
        />
      )}
      {exported && (
        <p role="status" className="truncate text-[13px] text-muted-foreground" title={exported}>
          {t("hist.exportedTo", { path: exported })}
        </p>
      )}

      {loading && items.length === 0 && (
        <div className="flex justify-center py-8">
          <Loader2 className="h-5 w-5 animate-spin text-muted-foreground" />
        </div>
      )}

      {!loading && items.length === 0 && (
        <div className="flex flex-col items-center gap-1 rounded-lg border-[1.5px] border-dashed border-input bg-card px-6 py-10 text-center">
          <Mic className="mb-1 h-5 w-5 text-muted-foreground" />
          <span className="font-display font-bold">{search || filter !== "all" ? t("hist.noMatch") : t("hist.emptyTitle")}</span>
          {!search && filter === "all" && <span className="text-[13px] text-muted-foreground">{t("hist.emptyDesc")}</span>}
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
