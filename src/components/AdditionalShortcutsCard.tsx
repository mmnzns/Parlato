// Card "Additional shortcuts" : raccourcis globaux utilitaires.
//
// Reference VoiceInk Features/Settings/Views/SettingsView.swift
// Section("Additional Shortcuts") :
//   Paste Last Transcription (Original) / (Enhanced), Retry Last
//   Transcription, Cancel Recording (defaut double-Echap + bouton reset).
// Plus "Open History" (VoiceInk .openHistoryWindow) et un ajout Parla :
// "Copy last transcription" (VoiceInk n'a qu'un accelerateur de menu
// Shift+Cmd+C, actif menu ouvert seulement).
//
// Chaque ligne = libelle + bouton keycap (ouvre HotkeyRecorder) + bouton
// effacer. La config complete est relue avant chaque ecriture pour ne pas
// ecraser les slots primary/secondary geres par HotkeyCard.

import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { RotateCcw, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Block, Row, Section } from "@/components/ui/section";
import { formatCombo, HotkeyRecorder } from "@/components/HotkeyRecorder";
import {
  api,
  type HotkeyConfig,
  type HotkeyTrigger,
  type UtilityShortcuts,
} from "@/lib/tauri";
import { translateError } from "@/lib/translateError";
import { cn } from "@/lib/utils";

type ActionKey = keyof UtilityShortcuts;

// Parlato: cancel first, it is the one most people need.
const ROWS: Array<{ key: ActionKey; labelKey: string; hintKey?: string }> = [
  {
    key: "cancel_recording",
    labelKey: "hotkey.additional.cancelRecording",
    hintKey: "hotkey.additional.cancelHint",
  },
  {
    key: "copy_last_transcription",
    labelKey: "hotkey.additional.copyLast",
    hintKey: "hotkey.additional.copyLastHint",
  },
  { key: "paste_last_transcription", labelKey: "hotkey.additional.pasteLast" },
  {
    key: "paste_last_enhancement",
    labelKey: "hotkey.additional.pasteLastEnhanced",
  },
  {
    key: "retry_last_transcription",
    labelKey: "hotkey.additional.retryLast",
    hintKey: "hotkey.additional.retryLastHint",
  },
  { key: "open_history", labelKey: "hotkey.additional.openHistory" },
];

export function triggerLabel(
  t: (k: string) => string,
  trigger: HotkeyTrigger,
): string | null {
  switch (trigger.kind) {
    case "none":
      return null;
    case "modifier":
      return t(`hotkey.options.${trigger.option}`);
    case "combo":
      return formatCombo(trigger);
  }
}

export function AdditionalShortcutsCard() {
  const { t } = useTranslation();
  const [config, setConfig] = useState<HotkeyConfig | null>(null);
  const [target, setTarget] = useState<ActionKey | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api.getHotkeyConfig().then(setConfig).catch(console.error);
  }, []);

  async function commitAction(key: ActionKey, trigger: HotkeyTrigger) {
    setError(null);
    try {
      // Relit la config courante : HotkeyCard peut avoir modifie les slots
      // d'enregistrement entre temps.
      const current = await api.getHotkeyConfig();
      const next: HotkeyConfig = {
        ...current,
        actions: { ...current.actions, [key]: trigger },
      };
      await api.setHotkeyConfig(next);
      setConfig(next);
    } catch (e) {
      setError(translateError(t, String(e)));
      api.getHotkeyConfig().then(setConfig).catch(console.error);
    }
  }

  async function resetCancel() {
    await commitAction("cancel_recording", { kind: "none" });
    try {
      await api.resetEscapeHint();
    } catch (e) {
      console.error(e);
    }
  }

  return (
    <Section title={t("hotkey.additional.cardTitle")} description={t("hotkey.additional.cardDescription")}>
      {!config ? (
        <Block className="text-sm text-muted-foreground">{t("common.loading")}</Block>
      ) : (
        ROWS.map((row) => {
          const trigger = config.actions[row.key];
          const isCancel = row.key === "cancel_recording";
          const label = triggerLabel(t, trigger);
          return (
            <Row key={row.key} label={t(row.labelKey)} description={row.hintKey ? t(row.hintKey) : undefined}>
              <button
                type="button"
                onClick={() => setTarget(row.key)}
                title={t("hotkey.additional.record")}
                className={cn(
                  "flex h-[34px] min-w-[112px] items-center justify-center rounded-sm border-[1.5px] px-2.5 font-mono text-xs font-semibold transition-colors hover:bg-muted",
                  label ? "border-edge bg-card shadow-btn" : "border-dashed border-input text-muted-foreground",
                )}
              >
                {label ?? (isCancel ? t("hotkey.additional.cancelDefault") : t("hotkey.additional.notSet"))}
              </button>
              {isCancel ? (
                <Button
                  size="icon"
                  variant="ghost"
                  className="text-muted-foreground"
                  onClick={resetCancel}
                  title={t("hotkey.additional.reset")}
                  aria-label={t("hotkey.additional.reset")}
                >
                  <RotateCcw className="h-3.5 w-3.5" />
                </Button>
              ) : (
                <Button
                  size="icon"
                  variant="ghost"
                  className="text-muted-foreground"
                  disabled={trigger.kind === "none"}
                  onClick={() => commitAction(row.key, { kind: "none" })}
                  title={t("hotkey.additional.clear")}
                  aria-label={t("hotkey.additional.clear")}
                >
                  <X className="h-3.5 w-3.5" />
                </Button>
              )}
            </Row>
          );
        })
      )}

      {error && (
        <Block className="py-3">
          <p className="rounded-sm bg-destructive/10 p-2 text-xs text-destructive">{error}</p>
        </Block>
      )}

      <HotkeyRecorder
        open={target !== null}
        onCancel={() => setTarget(null)}
        onCapture={(trigger) => {
          const key = target;
          setTarget(null);
          if (key) commitAction(key, trigger);
        }}
      />
    </Section>
  );
}
