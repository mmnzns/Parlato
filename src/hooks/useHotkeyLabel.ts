// Parlato: human-readable label for the primary dictation shortcut, shared
// by the sidebar status card and the Home screen. Re-reads the config
// whenever `refreshKey` changes (e.g. on navigation) so an edit made on
// the Microphone & shortcut page shows up immediately.

import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { formatCombo } from "@/components/HotkeyRecorder";
import { api, type HotkeyMode, type HotkeyTrigger } from "@/lib/tauri";

export function triggerLabel(
  trigger: HotkeyTrigger | null,
  t: (key: string) => string,
): string | null {
  if (!trigger || trigger.kind === "none") return null;
  if (trigger.kind === "modifier") return t(`hotkey.options.${trigger.option}`);
  return formatCombo(trigger);
}

export function useHotkeyLabel(refreshKey?: unknown): {
  label: string | null;
  mode: HotkeyMode | null;
} {
  const { t } = useTranslation();
  const [trigger, setTrigger] = useState<HotkeyTrigger | null>(null);
  const [mode, setMode] = useState<HotkeyMode | null>(null);

  useEffect(() => {
    api
      .getHotkeyConfig()
      .then((c) => {
        setTrigger(c.primary.trigger);
        setMode(c.primary.mode);
      })
      .catch(console.error);
  }, [refreshKey]);

  return { label: triggerLabel(trigger, t), mode };
}
