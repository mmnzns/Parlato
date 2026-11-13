// Parlato: live "can I dictate?" status (see lib/setupStatus.ts). Re-reads on
// navigation (`refreshKey`), when the window gets focus again (after System
// Settings or a model downloaded elsewhere), and on the events that say the
// model or the shortcut changed.

import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { loadSetupStatus, SETUP_CHANGED_EVENT, type SetupStatus } from "@/lib/setupStatus";

export function useSetupStatus(refreshKey?: unknown): SetupStatus | null {
  const { t, i18n } = useTranslation();
  const [status, setStatus] = useState<SetupStatus | null>(null);
  const [tick, setTick] = useState(0);

  useEffect(() => {
    const bump = () => setTick((n) => n + 1);
    window.addEventListener("focus", bump);
    window.addEventListener(SETUP_CHANGED_EVENT, bump);
    const unlisteners = [
      listen("source:changed", bump),
      listen("model:download:complete", bump),
      listen("parakeet_model:download:complete", bump),
    ];
    return () => {
      window.removeEventListener("focus", bump);
      window.removeEventListener(SETUP_CHANGED_EVENT, bump);
      unlisteners.forEach((u) => u.then((fn) => fn()));
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    loadSetupStatus(t)
      .then((s) => {
        if (!cancelled) setStatus(s);
      })
      .catch(console.error);
    return () => {
      cancelled = true;
    };
    // The language changes the model name, so it is a dependency too.
  }, [refreshKey, tick, t, i18n.resolvedLanguage]);

  return status;
}
