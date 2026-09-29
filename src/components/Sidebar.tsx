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
  FileAudio,
  History,
  House,
  Mic,
  Settings,
  Sparkles,
  Zap,
} from "lucide-react";
import type { LucideIcon } from "lucide-react";
import { useTranslation } from "react-i18next";
import { useHotkeyLabel } from "@/hooks/useHotkeyLabel";
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

      <nav className="flex flex-1 flex-col gap-3.5 overflow-auto px-2.5 py-0.5">
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

      <div className="m-3 flex flex-col gap-2 rounded-lg border-[1.5px] border-sidebar-border bg-card p-3">
        <div className="flex items-center gap-2">
          <span
            className={cn(
              "h-2 w-2 rounded-full",
              hotkeyLabel ? "bg-rec ring-[3px] ring-rec/25" : "bg-sidebar-muted",
            )}
          />
          <span className="text-[13px] font-semibold">
            {hotkeyLabel ? t("sidebar.statusReady") : t("sidebar.statusNoShortcut")}
          </span>
        </div>
        {hotkeyLabel && (
          <div className="text-xs leading-5 text-sidebar-muted">
            {t("sidebar.statusHoldBefore")}{" "}
            <kbd className="mx-0.5 inline-block rounded-[3px] border border-b-2 border-sidebar-muted px-1.5 font-mono text-[11px] leading-4 text-sidebar-foreground">
              {hotkeyLabel}
            </kbd>{" "}
            {t("sidebar.statusHoldAfter")}
          </div>
        )}
      </div>
    </aside>
  );
}
