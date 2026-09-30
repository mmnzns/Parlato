// Card "Recording shortcut" pour Settings : permet de configurer le
// raccourci primary + secondary qui declenche l'enregistrement.
//
// Reference VoiceInk Views/Settings/SettingsView.swift L40-79 :
//   Section("Shortcuts")
//     LabeledContent("Shortcut 1") + hotkeyModePicker + hotkeyPicker + custom
//     [optional] LabeledContent("Shortcut 2") + meme
//     [if no secondary] Button("Add Second Shortcut")
//
// Trois niveaux de configuration :
//   1. Modifier picker  (Right Alt, Left Ctrl, etc.)
//   2. Mode picker      (Toggle / Push-to-talk / Hybrid)
//   3. Custom combo     (Ctrl+Alt+R, F13, etc.) via HotkeyRecorder dialog
//
// Reset to defaults wipe la config user et remet Right Alt + Hybrid.

import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Plus, RotateCcw, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Block, RadioRow, Row, Section, selectClass } from "@/components/ui/section";
import { formatCombo, HotkeyRecorder } from "@/components/HotkeyRecorder";
import {
  api,
  type HotkeyConfig,
  type HotkeyMode,
  type HotkeyOptionId,
  type HotkeySlotConfig,
  type HotkeyTrigger,
} from "@/lib/tauri";
import { translateError } from "@/lib/translateError";
import { cn } from "@/lib/utils";

const MODIFIER_OPTIONS: HotkeyOptionId[] = [
  "rightAlt",
  "leftAlt",
  "rightCtrl",
  "leftCtrl",
  "rightWin",
  "rightShift",
  "leftShift",
];

const MODES: HotkeyMode[] = ["toggle", "pushToTalk", "hybrid"];

// Parlato: the Workbench design lists the modes as a radio list, most
// common first, with plain-language titles.
const MODE_ROWS: { mode: HotkeyMode; title: string; desc: string }[] = [
  { mode: "pushToTalk", title: "voice.holdTitle", desc: "voice.holdDesc" },
  { mode: "toggle", title: "voice.toggleTitle", desc: "voice.toggleDesc" },
  { mode: "hybrid", title: "voice.hybridTitle", desc: "voice.hybridDesc" },
];

type SelectValue =
  | { kind: "none" }
  | { kind: "modifier"; option: HotkeyOptionId }
  | { kind: "custom" };

function selectValueOf(trigger: HotkeyTrigger): SelectValue {
  switch (trigger.kind) {
    case "none":
      return { kind: "none" };
    case "modifier":
      return { kind: "modifier", option: trigger.option };
    case "combo":
      return { kind: "custom" };
  }
}

function selectValueToString(v: SelectValue): string {
  if (v.kind === "none") return "none";
  if (v.kind === "custom") return "custom";
  return `mod:${v.option}`;
}

function parseSelectValue(s: string): SelectValue {
  if (s === "none") return { kind: "none" };
  if (s === "custom") return { kind: "custom" };
  if (s.startsWith("mod:")) {
    return { kind: "modifier", option: s.slice(4) as HotkeyOptionId };
  }
  return { kind: "none" };
}

export function HotkeyCard() {
  const { t } = useTranslation();
  const [config, setConfig] = useState<HotkeyConfig | null>(null);
  const [showSecondary, setShowSecondary] = useState(false);
  const [recorderTarget, setRecorderTarget] = useState<"primary" | "secondary" | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api
      .getHotkeyConfig()
      .then((c) => {
        setConfig(c);
        setShowSecondary(c.secondary.trigger.kind !== "none");
      })
      .catch(console.error);
  }, []);

  async function commit(next: HotkeyConfig) {
    setError(null);
    setConfig(next);
    try {
      // Cette card ne gere que primary / secondary : on relit les
      // raccourcis additionnels pour ne pas ecraser ceux enregistres par
      // AdditionalShortcutsCard entre temps.
      const current = await api.getHotkeyConfig();
      const merged = { ...next, actions: current.actions };
      await api.setHotkeyConfig(merged);
      setConfig(merged);
    } catch (e) {
      setError(translateError(t, String(e)));
      api.getHotkeyConfig().then(setConfig).catch(console.error);
    }
  }

  async function reset() {
    try {
      const c = await api.resetHotkeyConfig();
      setConfig(c);
      setShowSecondary(false);
    } catch (e) {
      console.error(e);
    }
  }

  function updateSlot(slot: "primary" | "secondary", patch: Partial<HotkeySlotConfig>) {
    if (!config) return;
    const next = {
      ...config,
      [slot]: { ...config[slot], ...patch },
    };
    commit(next);
  }

  function onTriggerChange(slot: "primary" | "secondary", raw: string) {
    if (!config) return;
    const value = parseSelectValue(raw);
    if (value.kind === "custom") {
      // Open recorder, will commit when capture finishes.
      setRecorderTarget(slot);
      return;
    }
    if (value.kind === "none") {
      updateSlot(slot, { trigger: { kind: "none" } });
    } else {
      updateSlot(slot, {
        trigger: { kind: "modifier", option: value.option },
      });
    }
  }

  function onComboCaptured(trigger: Extract<HotkeyTrigger, { kind: "combo" }>) {
    if (!config || !recorderTarget) {
      setRecorderTarget(null);
      return;
    }
    updateSlot(recorderTarget, { trigger });
    setRecorderTarget(null);
  }

  function addSecondary() {
    if (!config) return;
    setShowSecondary(true);
    commit({
      ...config,
      secondary: {
        trigger: { kind: "modifier", option: "rightWin" },
        mode: config.primary.mode,
      },
    });
  }

  function removeSecondary() {
    if (!config) return;
    setShowSecondary(false);
    commit({
      ...config,
      secondary: { trigger: { kind: "none" }, mode: config.secondary.mode },
    });
  }

  return (
    <Section
      title={t("voice.shortcutTitle")}
      description={t("voice.shortcutDescription")}
      action={
        <Button size="sm" variant="ghost" onClick={reset} disabled={!config}>
          <RotateCcw className="h-3.5 w-3.5" />
          {t("hotkey.resetDefaults")}
        </Button>
      }
    >
      {!config ? (
        <Block className="text-sm text-muted-foreground">{t("common.loading")}</Block>
      ) : (
        <>
          <Row label={t("voice.keyLabel")} description={t("voice.keyHint")}>
            <TriggerPicker
              slot={config.primary}
              onSelect={(v) => onTriggerChange("primary", v)}
              onEditCombo={() => setRecorderTarget("primary")}
            />
          </Row>

          <div role="radiogroup" aria-label={t("voice.shortcutTitle")} className="flex flex-col divide-y">
            {MODE_ROWS.map((r) => (
              <RadioRow
                key={r.mode}
                selected={config.primary.mode === r.mode}
                onSelect={() => updateSlot("primary", { mode: r.mode })}
                disabled={config.primary.trigger.kind === "none"}
                title={t(r.title)}
                description={t(r.desc)}
              />
            ))}
          </div>

          <Row label={t("voice.secondLabel")} description={t("voice.secondHint")}>
            {showSecondary ? (
              <>
                <TriggerPicker
                  slot={config.secondary}
                  onSelect={(v) => onTriggerChange("secondary", v)}
                  onEditCombo={() => setRecorderTarget("secondary")}
                />
                <select
                  aria-label={t("voice.worksAs")}
                  value={config.secondary.mode}
                  onChange={(e) => updateSlot("secondary", { mode: e.target.value as HotkeyMode })}
                  disabled={config.secondary.trigger.kind === "none"}
                  className={cn(selectClass, "min-w-0")}
                >
                  {MODES.map((m) => (
                    <option key={m} value={m}>
                      {t(`hotkey.modes.${m}`)}
                    </option>
                  ))}
                </select>
                <Button
                  size="icon"
                  variant="ghost"
                  onClick={removeSecondary}
                  title={t("voice.remove")}
                  aria-label={t("voice.remove")}
                >
                  <X className="h-4 w-4" />
                </Button>
              </>
            ) : (
              <Button size="sm" variant="outline" onClick={addSecondary}>
                <Plus className="h-3.5 w-3.5" />
                {t("voice.add")}
              </Button>
            )}
          </Row>

          {(config.primary.trigger.kind === "modifier" && config.primary.trigger.option === "rightAlt") ||
          error ? (
            <Block className="flex flex-col gap-2 py-3">
              {config.primary.trigger.kind === "modifier" && config.primary.trigger.option === "rightAlt" && (
                <p className="text-xs leading-relaxed text-muted-foreground">{t("hotkey.altGrTip")}</p>
              )}
              {error && <p className="rounded-sm bg-destructive/10 p-2 text-xs text-destructive">{error}</p>}
            </Block>
          ) : null}
        </>
      )}

      <HotkeyRecorder
        open={recorderTarget !== null}
        onCancel={() => setRecorderTarget(null)}
        onCapture={onComboCaptured}
      />
    </Section>
  );
}

/** Key select, plus the recorded combo as an editable chip when "Custom" is used. */
function TriggerPicker({
  slot,
  onSelect,
  onEditCombo,
}: {
  slot: HotkeySlotConfig;
  onSelect: (v: string) => void;
  onEditCombo: () => void;
}) {
  const { t } = useTranslation();
  const valueStr = selectValueToString(selectValueOf(slot.trigger));

  return (
    <>
      {slot.trigger.kind === "combo" && (
        <button
          type="button"
          onClick={onEditCombo}
          title={t("hotkey.editCombo")}
          className="flex h-[34px] items-center rounded-sm border-[1.5px] border-edge bg-card px-2.5 font-mono text-xs font-semibold shadow-btn transition-colors hover:bg-muted"
        >
          {formatCombo(slot.trigger)}
        </button>
      )}
      <select
        aria-label={t("voice.keyLabel")}
        value={valueStr}
        onChange={(e) => onSelect(e.target.value)}
        className={selectClass}
      >
        <option value="none">{t("hotkey.options.none")}</option>
        {MODIFIER_OPTIONS.map((opt) => (
          <option key={opt} value={`mod:${opt}`}>
            {t(`hotkey.options.${opt}`)}
          </option>
        ))}
        <option value="custom">{t("hotkey.options.custom")}</option>
      </select>
    </>
  );
}
