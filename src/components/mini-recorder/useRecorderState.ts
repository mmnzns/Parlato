import { useEffect, useRef, useState } from "react";
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
  const [stage, setStageState] = useState<Stage>("recording");
  // Parlato: the listeners below live in a one-shot effect, so they read the
  // current stage through this ref.
  const stageRef = useRef<Stage>("recording");
  const setStage = (s: Stage) => {
    stageRef.current = s;
    setStageState(s);
  };
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
      // Parlato: the pill window is reused between dictations, so a new
      // recording must wipe whatever the previous one left (a failure text,
      // a notice, the transcribing stage) instead of showing it stale.
      listen("recording:started", () => {
        setFailure("");
        setNotice(null);
        setEscapeHint(false);
        setStage("recording");
      }),
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
        const apply = () => {
          if (p.state === "transcribing") setStage("transcribing");
          else if (p.state === "enhancing") setStage("enhancing");
          else if (p.state === "failed") {
            setFailure(p.message ?? "");
            setStage("failed");
          } else if (p.state === "done" && !p.text) setStage("empty");
          // pasting / done -> VoiceInk dismiss directement sans afficher de
          // badge. On laisse le stage precedent jusqu'au close.
        };
        // Parlato: while a NEW recording is on, an event can only come from
        // the previous dictation still being processed, and must not replace
        // the recording stage. The stage alone is not proof (a very short
        // dictation can end before this window listened to "recording:
        // stopped"), so the backend is asked.
        if (stageRef.current === "recording") {
          api
            .isRecording()
            .then((recording) => {
              if (!recording) apply();
            })
            .catch(() => apply());
          return;
        }
        apply();
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
