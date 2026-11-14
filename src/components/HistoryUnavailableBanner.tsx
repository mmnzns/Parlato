// Parlato: shown when the history database could not be opened at launch.
// Dictation still works; nothing is saved.

import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { api } from "@/lib/tauri";

export function HistoryUnavailableBanner() {
  const { t } = useTranslation();
  const [unavailable, setUnavailable] = useState(false);

  useEffect(() => {
    let cancelled = false;
    api
      .historyAvailable()
      .then((ok) => {
        if (!cancelled) setUnavailable(!ok);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, []);

  if (!unavailable) return null;
  return (
    <p
      role="alert"
      className="rounded-sm border-[1.5px] border-edge bg-muted px-4 py-3 text-[13px] text-warn-foreground"
    >
      {t("errors.historyUnavailable")}
    </p>
  );
}
