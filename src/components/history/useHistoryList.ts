// Parlato: History data and selection state (list loading, paging, select
// mode, export, delete). Moved as is from HistoryPanel.

import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, type TranscriptionRecord } from "@/lib/tauri";
import { confirmDelete } from "@/lib/confirmDelete";
import { PAGE_SIZE, type Filter } from "@/components/history/utils";

export function useHistoryList() {
  const [items, setItems] = useState<TranscriptionRecord[]>([]);
  const [loading, setLoading] = useState(false);
  const [search, setSearch] = useState("");
  const [filter, setFilter] = useState<Filter>("all");
  const [selectMode, setSelectMode] = useState(false);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [expanded, setExpanded] = useState<string | null>(null);
  const [count, setCount] = useState<number | null>(null);
  const [hasMore, setHasMore] = useState(false);
  // Parlato: where the last export went (shown under the toolbar).
  const [exported, setExported] = useState<string | null>(null);
  // Parlato: the history events below are registered once; refs give them
  // the current search and filter instead of the first render's.
  const searchRef = useRef(search);
  const filterRef = useRef(filter);
  searchRef.current = search;
  filterRef.current = filter;

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
  }, [search, filter]);

  async function refresh() {
    setLoading(true);
    try {
      const [rows, total] = await Promise.all([
        api.listHistory({
          limit: PAGE_SIZE,
          search: searchRef.current || null,
          filter: filterRef.current,
        }),
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
        search: searchRef.current || null,
        filter: filterRef.current,
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
    setExported(null);
    setSelected(new Set());
  }

  async function deleteIds(ids: string[], question: string) {
    if (ids.length === 0) return;
    if (!(await confirmDelete(question))) return;
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
      const path = await api.exportHistoryCsv(ids);
      if (path) setExported(path);
    } catch (e) {
      console.error(e);
    }
  }

  // Parlato: the filter is applied by the query (all dictations, not only
  // the loaded page).
  const visible = items;

  return {
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
  };
}
