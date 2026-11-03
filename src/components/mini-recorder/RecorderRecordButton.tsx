import { useTranslation } from "react-i18next";
import { Sparkles } from "lucide-react";
import type { Stage } from "./types";

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

export function RecorderRecordButton({
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
