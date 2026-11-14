// Sidebar de navigation principale.
//
// Reference VoiceInk Views/ContentView.swift NavigationSplitView +
// SidebarItemView L70-120 : list(selection:) .listStyle(.sidebar),
// item 14pt medium + SF Symbol 18pt.
//
// Parlato: layout and grouping follow the Workbench design
// (docs/design/v1, handoff README section 7): an unlabeled group
// (Home, History), "set up" and "more". The "more" group is always
// visible. View ids are unchanged so the tray menu keeps navigating.

import {
  AudioLines,
  BookA,
  Download,
  FileAudio,
  History,
  House,
  Mic,
  Settings,
  Sparkles,
  X,
  Zap,
} from "lucide-react";
import type { LucideIcon } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { getVersion } from "@tauri-apps/api/app";
import { useHotkeyLabel } from "@/hooks/useHotkeyLabel";
import { useSetupStatus } from "@/hooks/useSetupStatus";
import { MISSING_VIEW } from "@/lib/setupStatus";
import { cancelActiveDownload, downloadView, useActiveDownloads } from "@/lib/downloads";
import { modelName } from "@/lib/modelText";
import { cn } from "@/lib/utils";

export type View =
  | "dashboard"
  | "transcribe"
  | "history"
  | "models"
  | "enhancement"
  | "powermode"
  | "permissions"
  | "audio"
  | "dictionary"
  | "settings";

type Item = {
  id: View;
  labelKey: string;
  icon: LucideIcon;
};

type Group = {
  labelKey: string | null;
  items: Item[];
};

// "permissions" is no longer a sidebar entry: its panel lives on the
// Settings page.
const GROUPS: Group[] = [
  {
    labelKey: null,
    items: [
      { id: "dashboard", labelKey: "sidebar.dashboard", icon: House },
      { id: "history", labelKey: "sidebar.history", icon: History },
    ],
  },
  {
    labelKey: "sidebar.groupSetup",
    items: [
      { id: "audio", labelKey: "sidebar.recorder", icon: Mic },
      { id: "models", labelKey: "sidebar.models", icon: AudioLines },
      { id: "enhancement", labelKey: "sidebar.enhancement", icon: Sparkles },
      { id: "settings", labelKey: "sidebar.settings", icon: Settings },
    ],
  },
  {
    labelKey: "sidebar.groupMore",
    items: [
      { id: "powermode", labelKey: "sidebar.powerMode", icon: Zap },
      { id: "dictionary", labelKey: "sidebar.dictionary", icon: BookA },
      { id: "transcribe", labelKey: "sidebar.transcribe", icon: FileAudio },
    ],
  },
];

/** Breadcrumb shown above each page title, e.g. "set up / ai cleanup". */
export function useCrumb(view: View): string {
  const { t } = useTranslation();
  const target = view === "permissions" ? "settings" : view;
  for (const g of GROUPS) {
    const item = g.items.find((it) => it.id === target);
    if (item) {
      const parts = g.labelKey ? [t(g.labelKey), t(item.labelKey)] : [t(item.labelKey)];
      return parts.join(" / ").toLowerCase();
    }
  }
  return "";
}

export function Sidebar({
  current,
  onSelect,
}: {
  current: View;
  onSelect: (v: View) => void;
}) {
  const { t } = useTranslation();
  // Re-read on every navigation so the status card reflects a shortcut
  // just changed on the Microphone & shortcut page.
  const { label: hotkeyLabel } = useHotkeyLabel(current);
  // Ready only when dictation can really work (model, shortcut, mic, and on
  // a Mac Accessibility); otherwise the first missing step is a link.
  const setup = useSetupStatus(current);
  const ready = !!setup && !setup.missing;
  // Parlato: model downloads running now (they go on when the page that
  // started them is closed).
  const downloads = useActiveDownloads();
  const downloadPct = (() => {
    const known = downloads.filter((d) => d.total > 0);
    if (known.length === 0) return null;
    const total = known.reduce((n, d) => n + d.total, 0);
    return Math.min(100, Math.round((known.reduce((n, d) => n + d.downloaded, 0) / total) * 100));
  })();
  const downloadName = (d: (typeof downloads)[number]) =>
    d.kind === "vad" ? t("vad.modelTitle") : modelName(t, d.id, d.name);
  const downloadLabel =
    downloads.length === 1
      ? downloadPct === null
        ? t("downloads.one", { name: downloads[0] && downloadName(downloads[0]) })
        : t("downloads.onePct", { name: downloadName(downloads[0]), pct: downloadPct })
      : downloadPct === null
        ? t("downloads.many", { count: downloads.length })
        : t("downloads.manyPct", { count: downloads.length, pct: downloadPct });
  // Parlato: installed version, shown under the status card.
  const [version, setVersion] = useState("");
  useEffect(() => {
    getVersion().then(setVersion).catch(console.error);
  }, []);

  return (
    <aside className="flex h-full w-[232px] shrink-0 flex-col border-r-[1.5px] border-sidebar-border bg-sidebar text-sidebar-foreground">
      <div className="flex items-center gap-2.5 px-[18px] pt-[18px] pb-3.5">
        <img src="/favicon.png" alt="Parlato" className="h-[30px] w-[30px] rounded-[7px]" />
        <div className="flex flex-col">
          <span className="font-display text-lg leading-5 font-extrabold tracking-[-0.035em]">
            Parlato
          </span>
          <span className="font-mono text-[11px] leading-[14px] text-sidebar-muted">
            {t("sidebar.tagline")}
          </span>
        </div>
      </div>

      <nav aria-label={t("sidebar.navLabel")} className="flex flex-1 flex-col gap-3.5 overflow-auto px-2.5 py-0.5">
        {GROUPS.map((g, gi) => (
          <div key={g.labelKey ?? gi} className="flex flex-col gap-0.5">
            {g.labelKey && (
              <div className="px-2.5 pb-1.5 font-mono text-[11px] leading-[14px] text-sidebar-muted">
                {t(g.labelKey)}
              </div>
            )}
            {g.items.map((it) => {
              const Icon = it.icon;
              const active = current === it.id || (it.id === "settings" && current === "permissions");
              return (
                <button
                  key={it.id}
                  type="button"
                  onClick={() => onSelect(it.id)}
                  aria-current={active ? "page" : undefined}
                  className={cn(
                    "flex h-9 w-full items-center gap-2.5 rounded-sm px-2.5 text-left text-sm transition-colors",
                    active
                      ? "bg-nav-active font-semibold text-nav-active-foreground"
                      : "font-medium text-sidebar-foreground hover:bg-nav-hover",
                  )}
                >
                  <Icon
                    className={cn(
                      "h-4 w-4 shrink-0",
                      active ? "text-nav-active-icon" : "text-sidebar-muted",
                    )}
                  />
                  <span className="min-w-0 flex-1 truncate">{t(it.labelKey)}</span>
                </button>
              );
            })}
          </div>
        ))}
      </nav>

      {downloads.length > 0 && (
        <div className="mx-3 mt-3 flex items-center gap-1 rounded-lg border-[1.5px] border-sidebar-border bg-card py-1 pr-1 pl-1">
          <button
            type="button"
            onClick={() => onSelect(downloadView(downloads[0].kind))}
            title={downloadLabel}
            className="flex min-w-0 flex-1 cursor-pointer items-center gap-2 rounded-[3px] px-1.5 py-1.5 text-left text-xs leading-4 outline-none hover:bg-nav-hover focus-visible:ring-2 focus-visible:ring-ring"
          >
            <Download className="h-3.5 w-3.5 shrink-0 text-sidebar-muted" />
            <span role="status" className="min-w-0 flex-1 truncate">
              {downloadLabel}
            </span>
          </button>
          {downloads.length > 0 && (
            <button
              type="button"
              onClick={() => downloads.forEach((d) => cancelActiveDownload(d).catch(console.error))}
              aria-label={t("downloads.cancel")}
              title={t("downloads.cancel")}
              className="flex h-7 shrink-0 cursor-pointer items-center gap-1 rounded-[3px] px-1.5 text-xs font-semibold outline-none hover:bg-nav-hover focus-visible:ring-2 focus-visible:ring-ring"
            >
              <X className="h-3.5 w-3.5" />
              {t("downloads.cancel")}
            </button>
          )}
        </div>
      )}

      <div className="mx-3 mt-3 flex flex-col gap-2 rounded-lg border-[1.5px] border-sidebar-border bg-card p-3">
        <div className="flex items-center gap-2">
          <span
            className={cn(
              "h-2 w-2 rounded-full",
              ready ? "bg-rec ring-[3px] ring-rec/25" : "bg-sidebar-muted",
            )}
          />
          <span className="text-[13px] font-semibold">
            {setup ? (ready ? t("sidebar.statusReady") : t("sidebar.statusSetup")) : "\u00a0"}
          </span>
        </div>
        {setup?.missing && (
          <button
            type="button"
            onClick={() => onSelect(MISSING_VIEW[setup.missing!])}
            className="cursor-pointer self-start rounded-[3px] text-left text-xs leading-5 text-sidebar-foreground underline underline-offset-[3px] outline-none focus-visible:ring-2 focus-visible:ring-ring"
          >
            {t(`setup.${setup.missing}`)}
          </button>
        )}
        {ready && hotkeyLabel && (
          <div className="whitespace-nowrap text-xs leading-5 text-sidebar-muted">
            {/* Parlato: the key chip opens the shortcut settings. */}
            <button
              type="button"
              onClick={() => onSelect("audio")}
              title={t("dashboard.changeShortcut")}
              aria-label={`${hotkeyLabel}. ${t("dashboard.changeShortcut")}`}
              className="cursor-pointer rounded-[3px] outline-none hover:[&>kbd]:border-sidebar-foreground hover:[&>kbd]:bg-accent focus-visible:ring-2 focus-visible:ring-ring"
            >
              <kbd className="mx-0.5 inline-block rounded-[3px] border border-b-2 border-sidebar-muted px-1.5 font-mono text-[11px] leading-4 text-sidebar-foreground">
                {hotkeyLabel}
              </kbd>
            </button>{" "}
            {t("sidebar.statusTalk")}
          </div>
        )}
      </div>
      <div className="px-[18px] pt-2 pb-3 font-mono text-[11px] leading-[14px] text-sidebar-muted">
        {version && `Parlato v${version}`}
      </div>
    </aside>
  );
}
