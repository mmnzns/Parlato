// Parlato: History toolbar (search, filter, select mode), the selection
// action bar and the export status line.

import { useTranslation } from "react-i18next";
import { Download, Search, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Segmented } from "@/components/ui/section";
import type { Filter } from "@/components/history/utils";

export function HistoryToolbar({
  search,
  onSearch,
  filter,
  onFilter,
  selectMode,
  onToggleSelectMode,
  hasItems,
}: {
  search: string;
  onSearch: (value: string) => void;
  filter: Filter;
  onFilter: (value: Filter) => void;
  selectMode: boolean;
  onToggleSelectMode: () => void;
  hasItems: boolean;
}) {
  const { t } = useTranslation();
  return (
    <div className="flex flex-wrap items-center gap-2.5">
      <label className="flex h-9 min-w-[220px] flex-1 items-center gap-2 rounded-sm border-[1.5px] border-input bg-card px-3 text-muted-foreground focus-within:border-foreground">
        <Search className="h-4 w-4 flex-none" />
        <input
          type="text"
          value={search}
          onChange={(e) => onSearch(e.target.value)}
          placeholder={t("hist.search")}
          aria-label={t("hist.search")}
          className="h-full w-full min-w-0 bg-transparent text-sm text-foreground outline-none placeholder:text-muted-foreground"
        />
      </label>
      <Segmented<Filter>
        value={filter}
        onChange={onFilter}
        options={[
          { value: "all", label: t("hist.filterAll") },
          { value: "ai", label: t("hist.filterAi") },
          { value: "failed", label: t("hist.filterFailed") },
        ]}
      />
      <Button size="sm" variant="ghost" onClick={onToggleSelectMode} disabled={!hasItems}>
        {selectMode ? t("hist.done") : t("hist.select")}
      </Button>
    </div>
  );
}

export function HistorySelectBar({
  selectedCount,
  shownCount,
  onSelectAll,
  onExport,
  onDelete,
}: {
  selectedCount: number;
  shownCount: number;
  onSelectAll: () => void;
  onExport: () => void;
  onDelete: () => void;
}) {
  const { t } = useTranslation();
  return (
  <div className="flex flex-wrap items-center gap-2.5 rounded-lg bg-foreground py-2 pr-2 pl-3.5 text-background">
    <span className="flex-1 text-[13px] font-semibold">
      {selectedCount > 0 ? t("hist.selected", { count: selectedCount }) : t("hist.noneSelected")}
    </span>
    <button
      type="button"
      onClick={onSelectAll}
      className="h-[30px] rounded-sm px-3 text-[13px] font-semibold hover:bg-background/15"
    >
      {t("hist.selectAll")}
    </button>
    <button
      type="button"
      onClick={onExport}
      className="flex h-[30px] items-center gap-1.5 rounded-sm px-3 text-[13px] font-semibold hover:bg-background/15"
    >
      <Download className="h-3.5 w-3.5" />
      {selectedCount > 0
        ? t("hist.exportSelected", { count: selectedCount })
        : t("hist.exportShown", { count: shownCount })}
    </button>
    <button
      type="button"
      disabled={selectedCount === 0}
      onClick={onDelete}
      className="flex h-[30px] items-center gap-1.5 rounded-sm bg-destructive px-3 text-[13px] font-semibold text-white disabled:opacity-50"
    >
      <Trash2 className="h-3.5 w-3.5" />
      {t("hist.delete")}
    </button>
  </div>
  );
}
