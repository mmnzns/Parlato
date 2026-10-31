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

import { useTranslation } from "react-i18next";
import { LayoutGrid, X } from "lucide-react";
import { api } from "@/lib/tauri";
import { translateError } from "@/lib/translateError";
import { cn, modeBadge } from "@/lib/utils";
import { AudioVisualizer, StaticVisualizer } from "./mini-recorder/AudioVisualizer";
import { FailureText } from "./mini-recorder/FailureText";
import { ProcessingStatusDisplay } from "./mini-recorder/ProcessingStatusDisplay";
import { RecorderRecordButton } from "./mini-recorder/RecorderRecordButton";
import { useModePopover } from "./mini-recorder/useModePopover";
import { useRecorderState } from "./mini-recorder/useRecorderState";

// Transparent space (px) between the visible Mini pill and the bottom of its
// 120 pt window. Keep equal to PILL_BOTTOM_INSET in src-tauri/src/mini_recorder.rs,
// which uses it to place the window so the visible pill sits a fixed gap above
// the taskbar/Dock.
const MINI_PILL_BOTTOM_INSET = 24;

export function MiniRecorderView() {
  const { t } = useTranslation();
  const {
    setHoverButton,
    setHoverPopover,
    popoverOpenRef,
    modeButtonRef,
    pillRef,
    openPopover,
    closePopover,
  } = useModePopover();
  const {
    stage,
    meterDb,
    escapeHint,
    powerSession,
    powerConfigs,
    style,
    failure,
    notice,
  } = useRecorderState(setHoverPopover);

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
    : ({ marginBottom: MINI_PILL_BOTTOM_INSET } as const);
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
