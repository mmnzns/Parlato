// Parlato: left column of the power modes page, the list of modes.

import { useTranslation } from "react-i18next";
import { Plus } from "lucide-react";
import { Button } from "@/components/ui/button";
import { cn, powerShortcutLabel } from "@/lib/utils";
import type { PowerModeConfig } from "@/lib/tauri";

export function ModeList({
  configs,
  selectedId,
  onSelect,
  onCreate,
}: {
  configs: PowerModeConfig[];
  selectedId: string | null;
  onSelect: (id: string) => void;
  onCreate: () => void;
}) {
  const { t } = useTranslation();
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
          <button
            key={c.id}
            type="button"
            onClick={() => onSelect(c.id)}
            aria-current={active ? "true" : undefined}
            className={cn(
              "flex items-center gap-2.5 rounded-md border-[1.5px] px-3 py-2.5 text-left transition-colors",
              active ? "border-edge bg-accent shadow-[var(--sel-shadow)]" : "border-transparent hover:bg-muted/70",
            )}
          >
            <span
              className={cn(
                "h-2 w-2 flex-none rounded-full",
                c.is_enabled ? "bg-highlight" : "bg-border",
              )}
            />
            <span className="flex min-w-0 flex-1 flex-col">
              <span className="truncate text-sm font-semibold">{c.name}</span>
              <span className="truncate text-xs text-muted-foreground">{summary}</span>
            </span>
            {key && (
              <kbd className="rounded-sm border px-1.5 py-px font-mono text-[10px] text-muted-foreground">{key}</kbd>
            )}
          </button>
        );
      })}
      {configs.length === 0 && (
        <div className="flex flex-col gap-1 rounded-md border-[1.5px] border-dashed border-input p-3 text-xs text-muted-foreground">
          <span className="font-semibold text-foreground">{t("pm.emptyTitle")}</span>
          {t("pm.emptyDesc")}
        </div>
      )}
      <Button size="sm" variant="outline" className="mt-1 self-start" onClick={onCreate}>
        <Plus className="h-3.5 w-3.5" />
        {t("pm.newMode")}
      </Button>
    </nav>
  );
}
