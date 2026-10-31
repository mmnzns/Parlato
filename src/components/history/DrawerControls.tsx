// Parlato: drawer controls of a History card (play the kept recording,
// transcribe it again).

import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Loader2, Pause, Play, RotateCcw } from "lucide-react";
import { cn } from "@/lib/utils";
import { translateError } from "@/lib/translateError";
import { api } from "@/lib/tauri";
import { formatClock } from "@/components/history/utils";

export const drawerButton =
  "flex h-7 items-center gap-1.5 rounded-sm border-[1.5px] border-edge bg-card px-2.5 text-xs font-semibold hover:bg-accent disabled:opacity-50";

/// Plays the kept recording. The WAV is fetched only on the first press
/// and released when the drawer closes.
export function AudioPlayer({ id }: { id: string }) {
  const { t } = useTranslation();
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const urlRef = useRef<string | null>(null);
  const [playing, setPlaying] = useState(false);
  const [loading, setLoading] = useState(false);
  const [position, setPosition] = useState(0);
  const [length, setLength] = useState(0);
  const [error, setError] = useState<string | null>(null);

  useEffect(
    () => () => {
      audioRef.current?.pause();
      if (urlRef.current) URL.revokeObjectURL(urlRef.current);
    },
    [],
  );

  async function toggle() {
    setError(null);
    let audio = audioRef.current;
    if (!audio) {
      setLoading(true);
      try {
        const bytes = await api.readHistoryAudio(id);
        const url = URL.createObjectURL(new Blob([bytes], { type: "audio/wav" }));
        urlRef.current = url;
        audio = new Audio(url);
        audio.onloadedmetadata = () => setLength(audio!.duration);
        audio.ontimeupdate = () => setPosition(audio!.currentTime);
        audio.onended = () => {
          setPlaying(false);
          setPosition(0);
        };
        audioRef.current = audio;
      } catch (e) {
        setError(translateError(t, String(e)));
        return;
      } finally {
        setLoading(false);
      }
    }
    if (audio.paused) {
      await audio.play();
      setPlaying(true);
    } else {
      audio.pause();
      setPlaying(false);
    }
  }

  const Icon = loading ? Loader2 : playing ? Pause : Play;
  return (
    <>
      <button type="button" onClick={toggle} disabled={loading} className={drawerButton}>
        <Icon className={cn("h-3.5 w-3.5", loading && "animate-spin")} />
        {playing ? t("hist.pause") : t("hist.play")}
        {length > 0 && (
          <span className="font-mono font-normal text-muted-foreground">
            {formatClock(position) ?? "0:00"} / {formatClock(length)}
          </span>
        )}
      </button>
      {error && <span className="text-xs text-destructive">{error}</span>}
    </>
  );
}

/// Runs the recording through the current speech model (and AI cleanup if
/// it is on). The result is a new entry at the top of History.
export function RetranscribeButton({ id }: { id: string }) {
  const { t } = useTranslation();
  const [state, setState] = useState<"idle" | "started" | "error">("idle");
  const [error, setError] = useState<string | null>(null);

  async function run() {
    setError(null);
    try {
      await api.retranscribeHistoryItem(id);
      setState("started");
    } catch (e) {
      setError(translateError(t, String(e)));
      setState("error");
    }
  }

  return (
    <>
      <button
        type="button"
        onClick={run}
        disabled={state === "started"}
        title={t("hist.againHint")}
        className={drawerButton}
      >
        <RotateCcw className="h-3.5 w-3.5" />
        {t("hist.again")}
      </button>
      {state === "started" && (
        <span className="text-xs text-muted-foreground">{t("hist.againStarted")}</span>
      )}
      {error && <span className="text-xs text-destructive">{error}</span>}
    </>
  );
}
