// Updater UI : verifie les nouvelles releases GitHub au demarrage et offre
// l'install. Alimente par tauri-plugin-updater qui consulte l'endpoint defini
// dans tauri.conf.json (latest.json genere par le workflow CI release).
//
// Comportement :
// - Check au mount (une fois par session).
// - Si une release plus recente existe : bandeau discret en haut avec bouton
//   'Installer'. Le user peut Dismiss pour masquer pendant la session.
// - Progress du download stream via le callback contentLength / downloaded.
// - Une fois downloaded : relaunch() pour redemarrer sur la nouvelle version.

import { useEffect, useState, useSyncExternalStore } from "react";
import { useTranslation } from "react-i18next";
import { Download, RefreshCw, X } from "lucide-react";
import { listen } from "@tauri-apps/api/event";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import {
  checkForUpdate,
  formatReleaseDate,
  getUpdaterState,
  installPendingUpdate,
  subscribeUpdater,
} from "@/lib/updater";

export function UpdateChecker() {
  const { t, i18n } = useTranslation();
  // Parlato: state shared with the Settings button (lib/updater.ts).
  const { status: shared, update, percent: pct, error } = useSyncExternalStore(
    subscribeUpdater,
    getUpdaterState,
  );
  const [dismissed, setDismissed] = useState(false);
  const status = shared === "installFailed" ? "available" : shared;

  useEffect(() => {
    checkForUpdate(true);
    const un = listen<void>("tray:check-update", () => {
      setDismissed(false);
      checkForUpdate();
    });
    return () => {
      un.then((fn) => fn());
    };
  }, []);

  const install = installPendingUpdate;

  if (dismissed || !["available", "downloading", "installed"].includes(status)) return null;

  const releaseDate = formatReleaseDate(update?.date, i18n.resolvedLanguage ?? "en");

  return (
    <div
      className={cn(
        "flex items-center gap-3 border-b bg-primary/5 px-4 py-2 text-sm",
      )}
    >
      <Download className="h-4 w-4 text-foreground" />
      <div className="flex-1">
        {status === "available" && update && (
          <span>
            {t("updater.newVersion", { version: update.version })}
            {releaseDate && (
              <span className="ml-2 text-xs text-muted-foreground">({releaseDate})</span>
            )}
          </span>
        )}
        {status === "downloading" && (
          <span>
            {t("updater.downloading", { percent: pct !== null ? ` (${pct}%)` : "" })}
          </span>
        )}
        {status === "installed" && <span>{t("updater.installed")}</span>}
        {error && (
          <p className="text-xs text-destructive">
            {t("common.error")}: {error}
          </p>
        )}
      </div>
      {status === "available" && (
        <Button size="sm" onClick={install}>
          <RefreshCw className="h-3.5 w-3.5" />
          {t("updater.install")}
        </Button>
      )}
      <Button
        size="sm"
        variant="ghost"
        onClick={() => setDismissed(true)}
        title={t("updater.hide")}
      >
        <X className="h-3.5 w-3.5" />
      </Button>
    </div>
  );
}
