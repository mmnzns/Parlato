// Parlato: "when this mode starts" section: shortcut, paired apps and
// websites, fallback switch.

import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { CircleAlert, ExternalLink, Globe, X } from "lucide-react";
import { AppPicker } from "@/components/AppPicker";
import { Button } from "@/components/ui/button";
import { Block, Row, Section, Switch, selectClass } from "@/components/ui/section";
import { cn, powerShortcutLabel } from "@/lib/utils";
import { isMac } from "@/lib/platform";
import { api, type InstalledApp, type PowerModeConfig } from "@/lib/tauri";
import { ChipInput } from "./ChipInput";
import { SHORTCUT_SLOTS } from "./config";

/** Parlato (Mac): only the site name is compared, so "https://Mail.Google.com/x?y"
 *  is stored as "mail.google.com". */
function siteHost(raw: string): string {
  const rest = raw.trim().replace(/^[a-z][a-z0-9+.-]*:\/\//i, "");
  const authority = rest.split(/[/?#]/)[0] ?? "";
  const hostPort = authority.split("@").pop() ?? "";
  return hostPort.split(":")[0].replace(/\.$/, "").toLowerCase();
}

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
  // Parlato (Mac): the browser macOS refused Automation for, if any. Checked on
  // open and whenever the user comes back from System Settings.
  const [deniedBrowser, setDeniedBrowser] = useState<string | null>(null);
  useEffect(() => {
    if (!isMac) return;
    const check = () =>
      api
        .getBrowserAutomationStatus()
        .then((s) => setDeniedBrowser(s.denied_browser))
        .catch(console.error);
    check();
    window.addEventListener("focus", check);
    return () => window.removeEventListener("focus", check);
  }, []);

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
    const url = isMac ? siteHost(siteDraft ?? "") : (siteDraft ?? "").trim();
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
          <ChipInput
            draft={siteDraft}
            setDraft={setSiteDraft}
            onCommit={addSite}
            label={t("pm.addSite")}
            placeholder={t("pm.sitePlaceholder")}
          />
        </div>
        {/* Parlato (Mac): websites are read from the browser with AppleScript,
            which macOS gates behind the Automation permission. */}
        {isMac && (c.url_triggers.length > 0 || siteDraft !== null || deniedBrowser) && (
          <div className="flex flex-col gap-1.5">
            {(c.url_triggers.length > 0 || siteDraft !== null) && (
              <>
                <span className="text-xs leading-[17px] text-pretty text-muted-foreground">{t("pm.macSiteNote")}</span>
                <span className="text-xs leading-[17px] text-pretty text-muted-foreground">{t("pm.macFirefox")}</span>
              </>
            )}
            {deniedBrowser && (
              <div
                role="alert"
                className="mt-1 flex items-start gap-3 rounded-lg border-[1.5px] border-edge bg-card px-3.5 py-3"
              >
                <CircleAlert className="mt-0.5 h-4 w-4 flex-none text-warn-foreground" />
                <span className="min-w-0 flex-1 text-[13px] leading-[18px] text-pretty">
                  {t("pm.macDenied", { browser: deniedBrowser })}
                </span>
                <Button
                  size="sm"
                  variant="outline"
                  className="flex-none"
                  onClick={() => api.openAutomationSettings().catch(console.error)}
                >
                  {t("pm.macOpenAutomation")}
                  <ExternalLink className="h-3.5 w-3.5" />
                </Button>
              </div>
            )}
          </div>
        )}
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
