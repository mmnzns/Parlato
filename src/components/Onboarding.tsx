// Onboarding : flow au premier demarrage.
//
// Reference VoiceInk Views/Onboarding/OnboardingView.swift +
// OnboardingPermissionsView.swift. Sur Parla, l'adaptation Windows etait
// welcome + microphone + langue OCR + autostart + hotkey.
//
// Parlato: rebuilt to the Workbench design (docs/design/v1, "onboarding"):
// four steps with a step list on the left. Each step reuses the panel the
// app already has, so setup and Settings can never disagree:
//   1. Microphone  -> MicrophonePanel (device + level test)
//   2. Speech model -> ModelsPage (download or pick a model)
//   3. Shortcut    -> HotkeyCard (key + how it works)
//   4. Try it      -> practice box, plus start-at-login
// On macOS a first "Permissions" step asks for Microphone and Accessibility
// (shortcut + paste) before anything else, so the system prompts come with
// an explanation.
// The OCR language step moved out: it lives in Settings > Permissions.

import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { ArrowRight, Check } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Row, Switch } from "@/components/ui/section";
import { HotkeyCard } from "@/components/HotkeyCard";
import { MicrophonePanel } from "@/components/MicrophonePanel";
import { ModelsPage } from "@/components/ModelsPage";
import { useHotkeyLabel } from "@/hooks/useHotkeyLabel";
import { AccessibilityAction, MicrophoneAction, PermissionRow } from "@/components/PermissionsPanel";
import { api, type PermissionStatus } from "@/lib/tauri";
import { cn } from "@/lib/utils";
import { isMac } from "@/lib/platform";
import { Mic, ShieldCheck } from "lucide-react";

type Step = "access" | "mic" | "model" | "key" | "try";
const STEPS: readonly Step[] = isMac
  ? ["access", "mic", "model", "key", "try"]
  : ["mic", "model", "key", "try"];

export function Onboarding({
  onDone,
  selectedModelId,
  onSelectModel,
}: {
  onDone: () => void;
  selectedModelId: string | null;
  onSelectModel: (id: string | null) => void;
}) {
  const { t } = useTranslation();
  const [index, setIndex] = useState(0);
  const step: Step = STEPS[index];
  const { label: keyLabel } = useHotkeyLabel(step);
  const key = keyLabel ?? t("hotkey.options.rightAlt");

  const copy: Record<Step, { label: string; title: string; desc: string }> = {
    access: { label: t("ob.accessLabel"), title: t("ob.accessTitle"), desc: t("ob.accessDesc") },
    mic: { label: t("ob.micLabel"), title: t("ob.micTitle"), desc: t("ob.micDesc") },
    model: { label: t("ob.modelLabel"), title: t("ob.modelTitle"), desc: t("ob.modelDesc") },
    key: { label: t("ob.keyLabel"), title: t("ob.keyTitle"), desc: t("ob.keyDesc") },
    try: { label: t("ob.tryLabel"), title: t("ob.tryTitle"), desc: t("ob.tryDesc", { key }) },
  };

  async function finish() {
    try {
      await api.setOnboardingCompleted(true);
    } catch (e) {
      console.error(e);
    }
    onDone();
  }

  const last = index === STEPS.length - 1;

  return (
    <div
      className="fixed inset-0 z-[100] grid grid-cols-[260px_minmax(0,1fr)] bg-background text-foreground"
      style={{ backgroundImage: "var(--dot-grid)", backgroundSize: "18px 18px" }}
    >
      <aside className="flex flex-col gap-6 border-r-[1.5px] border-sidebar-border bg-sidebar px-6 py-8 text-sidebar-foreground">
        <div className="flex flex-col gap-1.5">
          <img src="/favicon.png" alt="Parlato" className="mb-2 h-10 w-10 rounded-[9px]" />
          <span className="font-display text-lg font-extrabold">{t("ob.setupTitle")}</span>
          <span className="text-[13px] text-muted-foreground">{t("ob.setupDesc")}</span>
        </div>
        <ol className="flex flex-col gap-1">
          {STEPS.map((s, i) => {
            const done = i < index;
            const current = i === index;
            return (
              <li key={s}>
                <button
                  type="button"
                  onClick={() => setIndex(i)}
                  aria-current={current ? "step" : undefined}
                  className={cn(
                    "flex w-full items-center gap-3 rounded-md px-2.5 py-2 text-left text-sm font-semibold transition-colors",
                    current ? "bg-nav-active text-nav-active-foreground" : "hover:bg-muted",
                  )}
                >
                  <span
                    className={cn(
                      "flex h-6 w-6 flex-none items-center justify-center rounded-full border-[1.5px] font-mono text-[11px]",
                      current
                        ? "border-highlight bg-highlight text-[#141416]"
                        : done
                          ? "border-edge bg-card"
                          : "border-input text-muted-foreground",
                    )}
                  >
                    {done ? <Check className="h-3.5 w-3.5" /> : i + 1}
                  </span>
                  {copy[s].label}
                </button>
              </li>
            );
          })}
        </ol>
        <span className="mt-auto font-mono text-[11px] text-muted-foreground">{t("ob.later")}</span>
      </aside>

      <div className="flex min-h-0 flex-col">
        <div data-slot="page" className="min-h-0 flex-1 overflow-auto">
          <div className="mx-auto flex max-w-[760px] flex-col gap-[22px] px-10 pt-10 pb-8">
            <header className="flex flex-col gap-1.5">
              <span className="font-mono text-[11px] text-muted-foreground">
                {t("ob.stepOf", { n: index + 1 })}
              </span>
              <h1 className="font-display text-[28px] leading-8 font-extrabold tracking-[-0.035em]">
                {copy[step].title}
              </h1>
              <p className="max-w-[560px] text-sm text-pretty text-muted-foreground">{copy[step].desc}</p>
            </header>

            {step === "access" && <AccessStep />}
            {step === "mic" && (
              <>
                <MicrophonePanel />
                <button
                  type="button"
                  onClick={() => api.openPrivacyMicrophone()}
                  className="self-start text-xs font-semibold underline underline-offset-[3px]"
                >
                  {t("ob.micSettings")}
                </button>
              </>
            )}
            {step === "model" && <ModelsPage selectedModelId={selectedModelId} onSelectModel={onSelectModel} />}
            {step === "key" && <HotkeyCard />}
            {step === "try" && <TryStep keyLabel={key} />}
          </div>
        </div>

        <footer className="flex items-center gap-3 border-t bg-card px-10 py-4">
          <div className="flex flex-1 gap-1.5" aria-hidden>
            {STEPS.map((s, i) => (
              <span
                key={s}
                className={cn("h-1.5 w-8 rounded-full", i <= index ? "bg-foreground" : "bg-border")}
              />
            ))}
          </div>
          <Button variant="ghost" size="sm" onClick={finish}>
            {t("ob.skip")}
          </Button>
          <Button variant="outline" size="sm" onClick={() => setIndex((i) => i - 1)} disabled={index === 0}>
            {t("ob.back")}
          </Button>
          <Button size="sm" onClick={() => (last ? finish() : setIndex((i) => i + 1))}>
            {last ? t("ob.start") : t("ob.continue")}
            <ArrowRight className="h-3.5 w-3.5" />
          </Button>
        </footer>
      </div>
    </div>
  );
}

/// Parlato: macOS permissions, explained before macOS asks.
function AccessStep() {
  const { t } = useTranslation();
  const [status, setStatus] = useState<PermissionStatus | null>(null);
  const refresh = () => {
    api.checkPermissions().then(setStatus).catch(console.error);
  };
  useEffect(() => {
    refresh();
    window.addEventListener("focus", refresh);
    return () => window.removeEventListener("focus", refresh);
  }, []);
  return (
    <div className="divide-y rounded-lg border-[1.5px] border-edge bg-card">
      <PermissionRow
        icon={Mic}
        title={t("permissions.microphoneTitle")}
        description={t("permissions.microphoneDescription")}
        state={status?.microphone}
        action={<MicrophoneAction state={status?.microphone} onChange={refresh} />}
      />
      <PermissionRow
        icon={ShieldCheck}
        title={t("permissions.mac.accessibilityTitle")}
        description={t("permissions.mac.accessibilityDescription")}
        state={status?.accessibility}
        action={<AccessibilityAction state={status?.accessibility} onChange={refresh} />}
      />
    </div>
  );
}

function TryStep({ keyLabel }: { keyLabel: string }) {
  const { t } = useTranslation();
  const [autostart, setAutostart] = useState<boolean | null>(null);

  useEffect(() => {
    api
      .checkPermissions()
      .then((p) => setAutostart(p.autostart.ok))
      .catch(console.error);
  }, []);

  async function toggleAutostart(v: boolean) {
    setAutostart(v);
    try {
      await api.setAutostartEnabled(v);
    } catch (e) {
      console.error(e);
      setAutostart(!v);
    }
  }

  return (
    <>
      <div className="flex flex-col gap-2">
        <textarea
          placeholder={t("ob.tryPlaceholder", { key: keyLabel })}
          className="min-h-[160px] rounded-lg border-[1.5px] border-edge bg-card p-4 text-[15px] leading-6 shadow-[var(--sel-shadow)] outline-none focus:border-foreground"
        />
        <span className="text-xs text-muted-foreground">{t("ob.tryNote")}</span>
      </div>
      {autostart !== null && (
        <div className="rounded-lg border-[1.5px] border-edge bg-card">
          <Row label={t("ob.autostart")} description={t("ob.autostartDesc")} htmlFor="ob-autostart">
            <Switch id="ob-autostart" checked={autostart} onChange={toggleAutostart} />
          </Row>
        </div>
      )}
    </>
  );
}
