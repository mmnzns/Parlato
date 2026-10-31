// Mini-recorder : overlay 300x120 affichee dans la fenetre "recorder".
//
// Reference VoiceInk Features/Recording/Views/MiniRecorderView.swift +
// Components/RecorderComponents.swift + AudioVisualizerView.swift :
//   VStack [FailureText 54px (Parlato, si echec)]
//          [Divider]
//          HStack [RecordButton 21pt] Spacer [StatusDisplay] Spacer [ModeButton 22pt]
//   control bar 40pt epingle bas, background Color.black opaque,
//   largeur 184 (compact) / 300 (message), corner radius 20 / 14.
//   15 bars audio avec wave + center boost, 60 FPS.
//   Processing: "Transcribing" / "Enhancing" + 5 dots animes.
//   Parlato : pas de texte en direct (LiveTranscriptView de VoiceInk retire
//   en 0.15.0, jamais utile pendant une dictee).
//   Astuce Echap : "Press Esc again to cancel" au premier Echap, une seule
//   fois (RecorderPanelShortcutManager.showEscapeConfirmationHintIfNeeded).
//
// Boutons (VoiceInk 2.x) :
//   - Gauche : RecorderRecordButton, cercle 21 pt gris au repos, rouge en
//     enregistrement, translucide et inactif pendant le traitement. Clic =
//     demarrer / arreter. Le selecteur de prompt de VoiceInk 1.x a ete retire
//     de la bulle (commit d59dfde), le prompt est porte par le mode.
//   - Droite : RecorderModeButton, popover ouvert au survol ou au clic,
//     ferme 250 ms apres avoir quitte le bouton ET le popover
//     (syncPopoverVisibility). Le popover est une fenetre separee
//     (NSPopover chez VoiceInk, fenetre "recorder-popover" ici) : la bulle
//     n'est plus redimensionnee.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { emit, listen } from "@tauri-apps/api/event";
import { LayoutGrid, Sparkles, X } from "lucide-react";
import {
  api,
  type AudioMeter,
  type PowerModeConfig,
  type PowerSession,
  type RecorderNotice,
} from "@/lib/tauri";
import { translateError } from "@/lib/translateError";
import { cn, modeBadge } from "@/lib/utils";

/// Duree d'affichage de l'astuce Echap = fenetre du double-Echap (1.5 s).
const ESCAPE_HINT_MS = 1500;
/// Delai de grace avant fermeture du popover (VoiceInk 0.25 s).
const POPOVER_GRACE_MS = 250;
const BARS = 15;

// Parlato: "failed" (shows the reason), "empty" (nothing heard) and
// "notice" (a short message for copy / retry shortcuts) before the pill
// closes, instead of vanishing without a word.
type Stage =
  | "idle"
  | "recording"
  | "transcribing"
  | "enhancing"
  | "failed"
  | "empty"
  | "notice";

type PipelineEvent = {
  state: "transcribing" | "enhancing" | "pasting" | "done" | "failed";
  message: string | null;
  text: string | null;
  duration_ms: number | null;
  job: "dictation" | "file" | "retry";
};

type PopoverHoverEvent = { hovering: boolean };

export function MiniRecorderView() {
  const { t } = useTranslation();
  const [stage, setStage] = useState<Stage>("recording");
  const [meterDb, setMeterDb] = useState<number>(-160);
  const [escapeHint, setEscapeHint] = useState(false);
  const [powerSession, setPowerSession] = useState<PowerSession | null>(null);
  const [powerConfigs, setPowerConfigs] = useState<PowerModeConfig[]>([]);
  const [style, setStyle] = useState<"mini" | "notch">("mini");
  // Parlato: reason shown in the "failed" stage, label of the "notice" stage.
  const [failure, setFailure] = useState<string>("");
  const [notice, setNotice] = useState<RecorderNotice | null>(null);

  // Popover Mode : survol du bouton (cette fenetre) + survol du popover
  // (remonte par l'autre fenetre via "recorder:popover-hover").
  const [hoverButton, setHoverButton] = useState(false);
  const [hoverPopover, setHoverPopover] = useState(false);
  const popoverOpenRef = useRef(false);
  const closeTimerRef = useRef<number | null>(null);
  const modeButtonRef = useRef<HTMLButtonElement>(null);
  const pillRef = useRef<HTMLDivElement>(null);

  const openPopover = useCallback(async () => {
    const btn = modeButtonRef.current;
    const pill = pillRef.current;
    if (!btn || !pill) return;
    const p = pill.getBoundingClientRect();
    popoverOpenRef.current = true;
    try {
      await emit("recorder:popover-refresh", {});
      // Parlato: the picker is centred on the pill, not on the button.
      await api.openRecorderPopover(p.left + p.width / 2, p.top, p.bottom);
    } catch (e) {
      console.error(e);
    }
  }, []);

  const closePopover = useCallback(() => {
    popoverOpenRef.current = false;
    api.closeRecorderPopover().catch(console.error);
  }, []);

  const wantPopover = hoverButton || hoverPopover;
  useEffect(() => {
    if (closeTimerRef.current !== null) {
      window.clearTimeout(closeTimerRef.current);
      closeTimerRef.current = null;
    }
    if (wantPopover) {
      if (!popoverOpenRef.current) openPopover();
    } else if (popoverOpenRef.current) {
      closeTimerRef.current = window.setTimeout(closePopover, POPOVER_GRACE_MS);
    }
    return () => {
      if (closeTimerRef.current !== null) {
        window.clearTimeout(closeTimerRef.current);
        closeTimerRef.current = null;
      }
    };
  }, [wantPopover, openPopover, closePopover]);

  useEffect(() => {
    // Charge une fois au mount - la fenetre mini-recorder est recreee a
    // chaque start donc pas besoin de re-fetch frequent.
    Promise.all([api.listPowerConfigs(), api.getRecorderStyle()])
      .then(([pcs, rs]) => {
        setPowerConfigs(pcs);
        setStyle(rs === "notch" ? "notch" : "mini");
      })
      .catch(console.error);

    const showNotice = (n: RecorderNotice | null) => {
      if (!n) return;
      setNotice(n);
      setStage("notice");
    };
    api.takeRecorderNotice().then(showNotice).catch(console.error);

    let hintTimer: number | null = null;
    const unlistens = [
      listen("recording:stopped", () => {
        setStage("transcribing");
        setEscapeHint(false);
      }),
      listen("recording:cancelled", () => setStage("idle")),
      listen<PipelineEvent>("pipeline:state", (e) => {
        const p = e.payload;
        // Parlato: only this dictation's events, not a file being
        // transcribed in the background.
        if (p.job !== "dictation") return;
        if (p.state === "transcribing") setStage("transcribing");
        else if (p.state === "enhancing") setStage("enhancing");
        else if (p.state === "failed") {
          setFailure(p.message ?? "");
          setStage("failed");
        } else if (p.state === "done" && !p.text) setStage("empty");
        // pasting / done -> VoiceInk dismiss directement sans afficher de
        // badge. On laisse le stage precedent jusqu'au close.
      }),
      listen("recorder:notice", () => {
        api.takeRecorderNotice().then(showNotice).catch(console.error);
      }),
      listen("recorder:escape-hint", () => {
        setEscapeHint(true);
        if (hintTimer !== null) window.clearTimeout(hintTimer);
        hintTimer = window.setTimeout(() => setEscapeHint(false), ESCAPE_HINT_MS);
      }),
      listen<PowerSession | null>("power_mode:active", (e) => {
        setPowerSession(e.payload);
      }),
      listen<PopoverHoverEvent>("recorder:popover-hover", (e) => {
        setHoverPopover(!!e.payload?.hovering);
      }),
    ];
    return () => {
      if (hintTimer !== null) window.clearTimeout(hintTimer);
      Promise.all(unlistens).then((arr) => arr.forEach((fn) => fn()));
    };
  }, []);

  // Polling du meter pendant l'enregistrement uniquement.
  useEffect(() => {
    if (stage !== "recording") return;
    let raf = 0;
    const tick = async () => {
      try {
        const m: AudioMeter = await api.getAudioMeter();
        setMeterDb(m.peak_db);
      } catch {
        // ignore
      }
      raf = window.requestAnimationFrame(tick);
    };
    raf = window.requestAnimationFrame(tick);
    return () => window.cancelAnimationFrame(raf);
  }, [stage]);

  const failureText = stage === "failed" ? translateError(t, failure) : "";
  const expanded = stage === "failed" || stage === "notice";
  const isNotch = style === "notch";
  const hasEnabledConfigs = powerConfigs.some((c) => c.is_enabled);

  // Notch : content aligne en haut, pill qui descend du bord superieur
  // avec top-flat + bottom-rounded (VoiceInk NotchShape). Mini : content
  // aligne en bas, pill pleinement arrondi.
  const containerAlign = isNotch ? "items-start" : "items-end";
  const spacingStyle = isNotch
    ? ({ marginTop: 0 } as const)
    : ({ marginBottom: 24 } as const);
  const shape = isNotch
    ? expanded
      ? "rounded-b-[22px] rounded-t-none"
      : "rounded-b-[16px] rounded-t-none"
    : expanded
      ? "rounded-[14px]"
      : "rounded-[20px]";

  return (
    <div
      className={cn(
        "flex h-screen w-screen justify-center bg-transparent",
        containerAlign,
      )}
    >
      <div
        ref={pillRef}
        className={cn(
          // Parlato: Workbench charcoal pill, flat: no shadow in any state.
          "flex flex-col bg-[#141416] text-white transition-all duration-300 ease-in-out",
          // Parlato: thin lime outline (orange on a failure) so the pill stays
          // easy to find on a dark desktop. Drawn inside, the size is unchanged.
          "ring-[1.5px] ring-inset",
          stage === "failed" ? "ring-[#FF8A5C]" : "ring-[#B5E61D]",
          // Parlato: wider while the Cancel button is shown, so "Cleaning up"
          // stays on one line in every language.
          expanded
            ? "w-[300px]"
            : stage === "transcribing" || stage === "enhancing"
              ? "w-[224px]"
              : "w-[184px]",
          shape,
        )}
        style={spacingStyle}
      >
        {failureText && !isNotch && (
          <>
            <FailureText text={failureText} />
            <div className="h-px bg-white/15" />
          </>
        )}
        <div className="flex h-10 items-center">
          <RecorderRecordButton
            stage={stage}
            onClick={() => api.toggleRecordingFromUi().catch(console.error)}
          />

          <div className="flex flex-1 items-center justify-center overflow-hidden px-1">
            {stage === "recording" && escapeHint && (
              <span className="truncate text-[11px] font-medium text-white/90">
                {t("miniRecorder.escapeHint")}
              </span>
            )}
            {stage === "recording" && !escapeHint && (
              <AudioVisualizer meterDb={meterDb} />
            )}
            {stage === "transcribing" && (
              <ProcessingStatusDisplay label={t("miniRecorder.writing")} intervalMs={180} />
            )}
            {stage === "enhancing" && (
              <ProcessingStatusDisplay label={t("miniRecorder.cleaning")} intervalMs={220} />
            )}
            {stage === "idle" && <StaticVisualizer />}
            {stage === "failed" && (
              <span role="alert" className="truncate text-[11px] font-semibold text-[#FF8A5C]">
                {t("miniRecorder.notPasted")}
              </span>
            )}
            {stage === "empty" && (
              <span className="truncate text-[11px] font-medium text-white/90">
                {t("miniRecorder.nothingHeard")}
              </span>
            )}
            {stage === "notice" && notice && (
              <span
                role="status"
                // Full text on hover, in case the label is cut with an ellipsis.
                title={
                  notice.kind === "failed"
                    ? translateError(t, notice.detail)
                    : t(`miniRecorder.notice.${notice.kind}`)
                }
                className={cn(
                  "min-w-0 truncate text-[11px] font-medium",
                  notice.kind === "failed" ? "text-[#FF8A5C]" : "text-white/90",
                )}
              >
                {notice.kind === "failed"
                  ? translateError(t, notice.detail)
                  : t(`miniRecorder.notice.${notice.kind}`)}
              </span>
            )}
          </div>

          {(stage === "transcribing" || stage === "enhancing") && (
            // Parlato: stop this dictation, nothing gets pasted.
            <button
              type="button"
              onClick={() => api.cancelProcessing().catch(console.error)}
              title={t("miniRecorder.cancelProcessing")}
              aria-label={t("miniRecorder.cancelProcessing")}
              className="mr-1 flex h-[22px] w-[22px] shrink-0 items-center justify-center rounded-full text-white/70 transition-colors hover:bg-white/10 hover:text-white"
            >
              <X className="h-3.5 w-3.5" />
            </button>
          )}
          <button
            ref={modeButtonRef}
            type="button"
            onMouseEnter={() => setHoverButton(true)}
            onMouseLeave={() => setHoverButton(false)}
            onClick={() => {
              if (popoverOpenRef.current) closePopover();
              else openPopover();
            }}
            title={powerSession?.config_name ?? t("miniRecorder.powerMode")}
            className={cn(
              // Parlato: no highlight on hover, the pill looks the same with the picker open.
              "mr-3 flex h-[22px] w-[22px] shrink-0 items-center justify-center rounded-full",
              !hasEnabledConfigs && "opacity-60",
            )}
          >
            {powerSession ? (
              <span className="text-[14px] font-semibold leading-none">
                {modeBadge(powerSession.emoji, powerSession.config_name)}
              </span>
            ) : (
              // VoiceInk : icone "square.grid.2x2" tant qu'aucun mode n'est
              // effectif.
              <LayoutGrid className="h-3.5 w-3.5 text-white/85" />
            )}
          </button>
        </div>
        {failureText && isNotch && (
          <>
            <div className="h-px bg-white/15" />
            <FailureText text={failureText} />
          </>
        )}
      </div>
    </div>
  );
}

// -- RecordButton (gauche) : VoiceInk RecorderRecordButton -----------------

// Parlato: Workbench palette (docs/design/v1, recorder pill): grey when
// ready, lime while listening, translucent while processing.
const RECORD_COLORS = {
  ready: { surface: "#3A3A3F", border: "#4A4A50", mark: "#C7C7CC" },
  recording: {
    surface: "#B5E61D",
    border: "#B5E61D",
    mark: "#141416",
  },
  processing: {
    surface: "rgba(255, 255, 255, 0.13)",
    border: "rgba(255, 255, 255, 0.18)",
    mark: "rgba(255, 255, 255, 0.86)",
  },
} as const;

function RecorderRecordButton({
  stage,
  onClick,
}: {
  stage: Stage;
  onClick: () => void;
}) {
  const { t } = useTranslation();
  const visual =
    stage === "recording"
      ? "recording"
      : stage === "transcribing" || stage === "enhancing"
        ? "processing"
        : "ready";
  const colors = RECORD_COLORS[visual];
  const disabled = visual === "processing";
  const title =
    visual === "recording"
      ? t("miniRecorder.stopRecording")
      : visual === "ready"
        ? t("miniRecorder.startRecording")
        : t("miniRecorder.processing");
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      title={title}
      className="ml-2.5 flex h-[22px] w-[22px] shrink-0 items-center justify-center rounded-full transition-colors duration-150 disabled:cursor-default"
      style={{
        background: colors.surface,
        boxShadow: `inset 0 0 0 0.6px ${colors.border}`,
      }}
    >
      {stage === "enhancing" ? (
        <Sparkles className="h-3 w-3 text-[#B5E61D]" />
      ) : (
        <span
          className="block h-[7px] w-[7px] rounded-[2.2px]"
          style={{ background: colors.mark }}
        />
      )}
    </button>
  );
}

// -- Audio visualizer (15 bars, wave + center boost) -----------------------

function AudioVisualizer({ meterDb }: { meterDb: number }) {
  // Normalisation -60dB..0dB -> 0..1 (aligne VoiceInk Recorder.swift)
  const power = normalizePower(meterDb);
  // Curve perceptuelle ^0.7 (VoiceInk AudioVisualizerView amplitude clamp).
  const amplitude = Math.pow(power, 0.7);

  const startTime = useRef<number>(performance.now());
  const [tick, setTick] = useState(0);

  useEffect(() => {
    let raf = 0;
    const loop = () => {
      setTick(performance.now() - startTime.current);
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);
    return () => cancelAnimationFrame(raf);
  }, []);

  const bars = useMemo(() => {
    const t = tick / 1000;
    return Array.from({ length: BARS }, (_, i) => {
      const phase = i * 0.4;
      const wave = Math.sin(t * 8 + phase) * 0.5 + 0.5;
      const distanceFromCenter = Math.abs(i - (BARS - 1) / 2) / ((BARS - 1) / 2);
      const centerBoost = 1.0 - distanceFromCenter * 0.4;
      const height = 4 + amplitude * wave * centerBoost * 24;
      return Math.max(4, Math.min(28, height));
    });
  }, [tick, amplitude]);

  return (
    <div className="flex h-7 items-center gap-[2px]">
      {bars.map((h, i) => (
        <span
          key={i}
          className="w-[3px] rounded-[1.5px] bg-[#B5E61D]"
          style={{ height: `${h}px` }}
        />
      ))}
    </div>
  );
}

function StaticVisualizer() {
  return (
    <div className="flex h-7 items-center gap-[2px]">
      {Array.from({ length: BARS }).map((_, i) => (
        <span
          key={i}
          className="w-[3px] rounded-[1.5px] bg-white/45"
          style={{ height: "4px" }}
        />
      ))}
    </div>
  );
}

function normalizePower(db: number): number {
  if (!isFinite(db) || db <= -60) return 0;
  const clamped = Math.max(-60, Math.min(0, db));
  return (clamped + 60) / 60;
}

// -- Processing display ("Transcribing" / "Enhancing" + 5 dots) ------------

function ProcessingStatusDisplay({
  label,
  intervalMs,
}: {
  label: string;
  intervalMs: number;
}) {
  const [step, setStep] = useState(0);
  useEffect(() => {
    const id = window.setInterval(() => setStep((s) => (s + 1) % 6), intervalMs);
    return () => window.clearInterval(id);
  }, [intervalMs]);
  return (
    <div className="flex items-center gap-2 whitespace-nowrap text-[11px] font-medium text-white">
      <span>{label}</span>
      <span className="flex items-center gap-[3px]">
        {Array.from({ length: 5 }).map((_, i) => (
          <span
            key={i}
            className={cn(
              "h-[3px] w-[3px] rounded-full transition-opacity",
              i < step ? "bg-white" : "bg-white/30",
            )}
          />
        ))}
      </span>
    </div>
  );
}

// -- FailureText (Parlato) ----------------------------------------------------
//
// Why the dictation was not pasted, translated, in a 54 px band above the
// control bar. The recording stays in History for Transcribe again.

function FailureText({ text }: { text: string }) {
  return (
    <div
      role="alert"
      className="line-clamp-3 h-[54px] overflow-hidden px-4 py-1.5 text-left text-[12px] leading-snug text-white/85"
    >
      {text}
    </div>
  );
}
