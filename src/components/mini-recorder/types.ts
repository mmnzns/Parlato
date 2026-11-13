// Shared types and constants of the mini-recorder pill.

// Parlato: "failed" (shows the reason), "empty" (nothing heard) and
// "notice" (a short message for copy / retry shortcuts) before the pill
// closes, instead of vanishing without a word.
export type Stage =
  | "idle"
  | "recording"
  | "transcribing"
  | "enhancing"
  | "failed"
  | "empty"
  | "notice";

export type PipelineEvent = {
  state: "transcribing" | "enhancing" | "pasting" | "done" | "failed";
  message: string | null;
  text: string | null;
  duration_ms: number | null;
  job: "dictation" | "file" | "retry";
  /** Number of the dictation this event belongs to (0 for file / retry). */
  dictation?: number;
};

/** Payload of "recording:started" / "recording:stopped" (extra fields omitted). */
export type RecordingEvent = { dictation_id?: number } | null;

export type PopoverHoverEvent = { hovering: boolean };

/// Duree d'affichage de l'astuce Echap = fenetre du double-Echap (1.5 s).
export const ESCAPE_HINT_MS = 1500;
/// Delai de grace avant fermeture du popover (VoiceInk 0.25 s).
export const POPOVER_GRACE_MS = 250;
export const BARS = 15;
