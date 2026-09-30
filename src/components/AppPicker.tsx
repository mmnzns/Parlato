// Parlato: pick an app to pair with a power mode from the apps on this PC
// (Start menu shortcuts plus windows open right now), instead of typing its
// program name. Apps open now are listed first.

import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Check, Loader2, Search } from "lucide-react";
import { api, type InstalledApp } from "@/lib/tauri";
import { cn } from "@/lib/utils";

type Props = {
  /** exe names already paired with this mode. */
  paired: string[];
  onPick: (app: InstalledApp) => void;
  /** Switch to typing the program name by hand. */
  onTypeName: () => void;
  onClose: () => void;
};

export function AppPicker({ paired, onPick, onTypeName, onClose }: Props) {
  const { t } = useTranslation();
  const [apps, setApps] = useState<InstalledApp[] | null>(null);
  const [query, setQuery] = useState("");

  useEffect(() => {
    api
      .listInstalledApps()
      .then(setApps)
      .catch((e) => {
        console.error(e);
        setApps([]);
      });
  }, []);

  const shown = useMemo(() => {
    const q = query.trim().toLowerCase();
    const list = (apps ?? []).filter(
      (a) => !q || a.name.toLowerCase().includes(q) || a.exe_name.includes(q),
    );
    // Open apps first, each group already sorted by name by the backend.
    return [...list.filter((a) => a.running), ...list.filter((a) => !a.running)];
  }, [apps, query]);

  return (
    <div
      className="flex w-full flex-col gap-2 rounded-md border-[1.5px] border-edge bg-card p-2.5"
      onKeyDown={(e) => e.key === "Escape" && onClose()}
    >
      <label className="flex h-9 items-center gap-2 rounded-sm border-[1.5px] border-input bg-background px-3 text-muted-foreground focus-within:border-foreground">
        <Search className="h-4 w-4 flex-none" />
        <input
          autoFocus
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder={t("pm.appSearch")}
          aria-label={t("pm.appSearch")}
          className="h-full w-full min-w-0 bg-transparent text-sm text-foreground outline-none placeholder:text-muted-foreground"
        />
      </label>

      <div className="flex max-h-64 flex-col overflow-y-auto" role="listbox" aria-label={t("pm.addApp")}>
        {apps === null ? (
          <span className="flex items-center gap-2 px-2 py-3 text-xs text-muted-foreground">
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
            {t("pm.appLoading")}
          </span>
        ) : shown.length === 0 ? (
          <span className="px-2 py-3 text-xs text-muted-foreground">{t("pm.appNone")}</span>
        ) : (
          shown.map((a) => {
            const done = paired.includes(a.exe_name);
            return (
              <button
                key={a.exe_name}
                type="button"
                role="option"
                aria-selected={done}
                disabled={done}
                onClick={() => onPick(a)}
                className={cn(
                  "flex items-center gap-2 rounded-sm px-2 py-1.5 text-left text-sm",
                  done ? "text-muted-foreground" : "hover:bg-muted",
                )}
              >
                <span className="min-w-0 flex-1 truncate">{a.name}</span>
                {a.running && (
                  <span className="flex-none rounded-full bg-highlight px-1.5 py-px text-[10px] font-semibold text-[#141416]">
                    {t("pm.appOpen")}
                  </span>
                )}
                <span className="flex-none font-mono text-[11px] text-muted-foreground">{a.exe_name}</span>
                {done && <Check className="h-3.5 w-3.5 flex-none" />}
              </button>
            );
          })
        )}
      </div>

      <div className="flex items-center justify-between gap-2 border-t border-dashed pt-2 text-xs">
        <button
          type="button"
          onClick={onTypeName}
          className="font-semibold text-foreground underline underline-offset-[3px]"
        >
          {t("pm.appTypeName")}
        </button>
        <button type="button" onClick={onClose} className="text-muted-foreground hover:text-foreground">
          {t("pm.cancel")}
        </button>
      </div>
    </div>
  );
}
