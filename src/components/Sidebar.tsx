// Sidebar de navigation principale.
//
// Reference VoiceInk Views/ContentView.swift NavigationSplitView +
// SidebarItemView L70-120 : list(selection:) .listStyle(.sidebar),
// item 14pt medium + SF Symbol 18pt.

import {
  Book,
  FileAudio,
  Gauge,
  History,
  Mic,
  Settings,
  Shield,
  Sparkles,
  Wrench,
  Zap,
} from "lucide-react";
import type { LucideIcon } from "lucide-react";
import { useTranslation } from "react-i18next";
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

// VoiceInk ContentView.swift L7-37 order. "VoiceInk Pro" is replaced by
// "Settings" (no licence for Parla) and some panels are merged.
const ITEMS: Item[] = [
  { id: "dashboard", labelKey: "sidebar.dashboard", icon: Gauge },
  { id: "transcribe", labelKey: "sidebar.transcribe", icon: FileAudio },
  { id: "history", labelKey: "sidebar.history", icon: History },
  { id: "models", labelKey: "sidebar.models", icon: Wrench },
  { id: "enhancement", labelKey: "sidebar.enhancement", icon: Sparkles },
  { id: "powermode", labelKey: "sidebar.powerMode", icon: Zap },
  { id: "permissions", labelKey: "sidebar.permissions", icon: Shield },
  { id: "audio", labelKey: "sidebar.recorder", icon: Mic },
  { id: "dictionary", labelKey: "sidebar.dictionary", icon: Book },
  { id: "settings", labelKey: "sidebar.settings", icon: Settings },
];

export function Sidebar({
  current,
  onSelect,
}: {
  current: View;
  onSelect: (v: View) => void;
}) {
  const { t } = useTranslation();
  return (
    <aside className="flex h-full w-[210px] shrink-0 flex-col border-r bg-muted/30">
      <div className="flex items-center gap-2 border-b px-4 py-3">
        <img
          src="/favicon.png"
          alt="Parlato"
          className="h-7 w-7 rounded-md shadow-sm"
        />
        <div className="flex items-baseline gap-2">
          <span className="text-sm font-semibold">Parlato</span>
          <span className="text-[10px] text-muted-foreground">Windows</span>
        </div>
      </div>
      <nav className="flex-1 overflow-auto p-2">
        <ul className="grid gap-0.5">
          {ITEMS.map((it) => {
            const Icon = it.icon;
            const active = current === it.id;
            return (
              <li key={it.id}>
                <button
                  type="button"
                  onClick={() => onSelect(it.id)}
                  className={cn(
                    "flex w-full items-center gap-3 rounded-md px-3 py-2 text-left text-sm font-medium transition-colors",
                    active
                      ? "bg-accent text-accent-foreground"
                      : "text-muted-foreground hover:bg-accent/60 hover:text-foreground",
                  )}
                >
                  <Icon className="h-4 w-4 shrink-0" />
                  <span className="truncate">{t(it.labelKey)}</span>
                </button>
              </li>
            );
          })}
        </ul>
      </nav>
    </aside>
  );
}
