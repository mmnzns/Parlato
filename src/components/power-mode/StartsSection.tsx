// Parlato: "when this mode starts" section: shortcut, paired apps and
// websites, fallback switch.

import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Globe, X } from "lucide-react";
import { AppPicker } from "@/components/AppPicker";
import { Block, Row, Section, Switch, selectClass } from "@/components/ui/section";
import { cn, powerShortcutLabel } from "@/lib/utils";
import { isMac } from "@/lib/platform";
import type { InstalledApp, PowerModeConfig } from "@/lib/tauri";
import { ChipInput } from "./ChipInput";
import { SHORTCUT_SLOTS } from "./config";

export function StartsSection({
  config: c,
  others,
  onChange,
  set,
}: {
  config: PowerModeConfig;
  others: PowerModeConfig[];
  onChange: (c: PowerModeConfig) => void;
  set: <K extends keyof PowerModeConfig>(key: K, value: PowerModeConfig[K]) => void;
}) {
  const { t } = useTranslation();
  const [appDraft, setAppDraft] = useState<string | null>(null);
  const [picking, setPicking] = useState(false);
  const [siteDraft, setSiteDraft] = useState<string | null>(null);

  function addApp() {
    const exe = (appDraft ?? "").trim().toLowerCase();
    setAppDraft(null);
    if (!exe || c.app_triggers.some((a) => a.exe_name === exe)) return;
    onChange({ ...c, app_triggers: [...c.app_triggers, { id: crypto.randomUUID(), exe_name: exe, app_name: exe }] });
  }

  function addPickedApp(app: InstalledApp) {
    setPicking(false);
    if (c.app_triggers.some((a) => a.exe_name === app.exe_name)) return;
    onChange({
      ...c,
      app_triggers: [...c.app_triggers, { id: crypto.randomUUID(), exe_name: app.exe_name, app_name: app.name }],
    });
  }

  function addSite() {
    const url = (siteDraft ?? "").trim();
    setSiteDraft(null);
    if (!url || c.url_triggers.some((u) => u.url === url)) return;
    onChange({ ...c, url_triggers: [...c.url_triggers, { id: crypto.randomUUID(), url }] });
  }

  const triggers = [
    ...c.app_triggers.map((a) => ({ id: a.id, label: a.app_name || a.exe_name, site: false })),
    ...c.url_triggers.map((u) => ({ id: u.id, label: u.url, site: true })),
  ];

  return (
    <Section title={t("pm.startsTitle")} description={t("pm.startsDesc")}>
      <Row label={t("pm.shortcut")} description={t("pm.shortcutDesc")}>
        <select
          aria-label={t("pm.shortcut")}
          value={c.shortcut_slot ?? ""}
          onChange={(e) => set("shortcut_slot", e.target.value === "" ? null : Number(e.target.value))}
          className={cn(selectClass, "w-[180px]")}
        >
          <option value="">{t("pm.shortcutNone")}</option>
          {SHORTCUT_SLOTS.map((slot) => {
            const key = powerShortcutLabel(slot);
            const owner = others.find((o) => o.shortcut_slot === slot);
            return (
              <option key={slot} value={slot}>
                {owner ? t("pm.shortcutTaken", { key, name: owner.name }) : key}
              </option>
            );
          })}
        </select>
      </Row>
      <Block className="flex flex-col gap-2.5">
        <div className="flex flex-col gap-0.5">
          <span className="font-medium">{t("pm.paired")}</span>
          <span className="text-[13px] leading-[18px] text-muted-foreground">{t("pm.pairedDesc")}</span>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          {triggers.map((tr) => (
            <span
              key={tr.id}
              className={cn(
                "flex h-8 items-center gap-1.5 rounded-full border-[1.5px] border-edge bg-card pr-1 pl-3 text-xs",
                tr.site && "font-mono",
              )}
            >
              {tr.site && <Globe className="h-3 w-3 text-muted-foreground" />}
              {tr.label}
              <button
                type="button"
                aria-label={t("pm.remove", { name: tr.label })}
                onClick={() =>
                  tr.site
                    ? set("url_triggers", c.url_triggers.filter((u) => u.id !== tr.id))
                    : set("app_triggers", c.app_triggers.filter((a) => a.id !== tr.id))
                }
                className="flex h-6 w-6 items-center justify-center rounded-full text-muted-foreground hover:bg-muted hover:text-foreground"
              >
                <X className="h-3.5 w-3.5" />
              </button>
            </span>
          ))}
          <ChipInput
            draft={appDraft}
            setDraft={setAppDraft}
            onOpen={() => setPicking(true)}
            onCommit={addApp}
            label={t("pm.addApp")}
            placeholder={t("pm.appPlaceholder")}
          />
          {/* Parlato: matching a website needs the browser's address, which is
              not read on Mac yet. */}
          {!isMac && (
            <ChipInput
              draft={siteDraft}
              setDraft={setSiteDraft}
              onCommit={addSite}
              label={t("pm.addSite")}
              placeholder={t("pm.sitePlaceholder")}
            />
          )}
        </div>
        {picking && (
          <AppPicker
            paired={c.app_triggers.map((a) => a.exe_name)}
            onPick={addPickedApp}
            onTypeName={() => {
              setPicking(false);
              setAppDraft("");
            }}
            onClose={() => setPicking(false)}
          />
        )}
      </Block>
      <Row label={t("pm.fallback")} description={t("pm.fallbackDesc")} htmlFor="pm-fallback">
        <Switch id="pm-fallback" checked={c.is_default} onChange={(v) => set("is_default", v)} />
      </Row>
    </Section>
  );
}
