import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  api,
  type AudioMeter,
  type PowerModeConfig,
  type PowerSession,
  type RecorderNotice,
} from "@/lib/tauri";
import {
  ESCAPE_HINT_MS,
  type PipelineEvent,
  type PopoverHoverEvent,
  type Stage,
} from "./types";

/// State of the pill driven by backend events (stage, meter, notice, mode).
/// `setHoverPopover` is fed by the "recorder:popover-hover" listener, which
/// lives in the same effect as the other listeners.
export function useRecorderState(setHoverPopover: (hovering: boolean) => void) {
  const [stage, setStage] = useState<Stage>("recording");
  const [meterDb, setMeterDb] = useState<number>(-160);
  const [escapeHint, setEscapeHint] = useState(false);
  const [powerSession, setPowerSession] = useState<PowerSession | null>(null);
  const [powerConfigs, setPowerConfigs] = useState<PowerModeConfig[]>([]);
  const [style, setStyle] = useState<"mini" | "notch">("mini");
  // Parlato: reason shown in the "failed" stage, label of the "notice" stage.
  const [failure, setFailure] = useState<string>("");
  const [notice, setNotice] = useState<RecorderNotice | null>(null);

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
    // setHoverPopover is a stable useState setter.
    // eslint-disable-next-line react-hooks/exhaustive-deps
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

  return {
    stage,
    meterDb,
    escapeHint,
    powerSession,
    powerConfigs,
    style,
    failure,
    notice,
  };
}
