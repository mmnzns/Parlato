// Parlato: left column of the power modes page, the list of modes.

import { useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { MoreHorizontal, Plus } from "lucide-react";
import { Button } from "@/components/ui/button";
import { cn, powerShortcutLabel } from "@/lib/utils";
import type { PowerModeConfig } from "@/lib/tauri";
import { ModeMenu, type ModeAction } from "./ModeMenu";

export function ModeList({
  configs,
  selectedId,
  onSelect,
  onCreate,
  onDuplicate,
  onRename,
  onDelete,
}: {
  configs: PowerModeConfig[];
  selectedId: string | null;
  onSelect: (id: string) => void;
  onCreate: () => void;
  onDuplicate: (id: string) => void;
  onRename: (id: string) => void;
  onDelete: (id: string) => void;
}) {
  const { t } = useTranslation();
  // Parlato: the open right-click menu and the element to give focus back to.
  const [menu, setMenu] = useState<{ id: string; x: number; y: number } | null>(null);
  const returnTo = useRef<HTMLElement | null>(null);

  function openMenu(id: string, e: React.MouseEvent<HTMLElement>, anchor: HTMLElement) {
    e.preventDefault();
    returnTo.current = anchor;
    // The keyboard's context menu key reports 0,0: open under the row instead.
    const r = anchor.getBoundingClientRect();
    const keyboard = e.clientX === 0 && e.clientY === 0;
    setMenu({ id, x: keyboard ? r.left + 8 : e.clientX, y: keyboard ? r.bottom : e.clientY });
  }

  function closeMenu(refocus = true) {
    setMenu(null);
    if (refocus) returnTo.current?.focus();
  }

  function pick(action: ModeAction) {
    const id = menu?.id;
    closeMenu(action === "duplicate");
    if (!id) return;
    if (action === "duplicate") onDuplicate(id);
    else if (action === "rename") onRename(id);
    else onDelete(id);
  }

  const shortcutFor = (c: PowerModeConfig) => (c.is_enabled ? powerShortcutLabel(c.shortcut_slot) : null);

  return (
    <nav className="flex flex-col gap-1.5" aria-label={t("pm.yourModes")}>
      <span className="px-1 font-mono text-[11px] text-muted-foreground">{t("pm.yourModes")}</span>
      {configs.map((c) => {
        const active = c.id === selectedId;
        const paired = c.app_triggers.length + c.url_triggers.length;
        const summary = !c.is_enabled
          ? t("pm.summaryOff")
          : c.is_default
            ? t("pm.summaryEverywhere")
            : t("pm.summaryApps", { count: paired });
        const key = shortcutFor(c);
        return (
          <div
            key={c.id}
            onContextMenu={(e) => openMenu(c.id, e, e.currentTarget.querySelector("button") ?? e.currentTarget)}
            className={cn(
              "group flex items-center gap-1 rounded-md border-[1.5px] pr-1.5 transition-colors",
              active ? "border-edge bg-accent shadow-[var(--sel-shadow)]" : "border-transparent hover:bg-muted/70",
            )}
          >
            <button
              type="button"
              onClick={() => onSelect(c.id)}
              aria-current={active ? "true" : undefined}
              className="flex min-w-0 flex-1 items-center gap-2.5 rounded-md py-2.5 pl-3 text-left"
            >
              <span
                className={cn(
                  "h-2 w-2 flex-none rounded-full",
                  c.is_enabled ? "bg-highlight" : "bg-border",
                )}
              />
              <span className="flex min-w-0 flex-1 flex-col">
                <span className="truncate text-sm font-semibold">{c.name}</span>
                <span className="flex min-w-0 items-center gap-1.5">
                  <span className="min-w-0 truncate text-xs text-muted-foreground">{summary}</span>
                  {key && (
                    <kbd className="flex-none rounded-sm border px-1.5 py-px font-mono text-[10px] text-muted-foreground">
                      {key}
                    </kbd>
                  )}
                </span>
              </span>
            </button>
            <button
              type="button"
              aria-label={t("pm.moreFor", { name: c.name })}
              aria-haspopup="menu"
              aria-expanded={menu?.id === c.id}
              onClick={(e) => openMenu(c.id, e, e.currentTarget)}
              className={cn(
                "flex h-7 w-7 flex-none items-center justify-center rounded-sm text-muted-foreground hover:bg-muted hover:text-foreground focus-visible:opacity-100",
                active || menu?.id === c.id ? "opacity-100" : "opacity-0 group-hover:opacity-100",
              )}
            >
              <MoreHorizontal className="h-4 w-4" />
            </button>
          </div>
        );
      })}
      {configs.length === 0 && (
        <div className="flex flex-col gap-1 rounded-md border-[1.5px] border-dashed border-input p-3 text-xs text-muted-foreground">
          <span className="font-semibold text-foreground">{t("pm.emptyTitle")}</span>
          {t("pm.emptyDesc")}
        </div>
      )}
      {menu && <ModeMenu x={menu.x} y={menu.y} onPick={pick} onClose={closeMenu} />}
      <Button size="sm" variant="outline" className="mt-1 self-start" onClick={onCreate}>
        <Plus className="h-3.5 w-3.5" />
        {t("pm.newMode")}
      </Button>
    </nav>
  );
}
