// Parlato: microphone picker for the Microphone & shortcut page
// (Workbench design, docs/design/v1). The selection is the same persisted
// setting the tray "Audio input" submenu uses.
//
// The level meter only reports while a recording runs, so "Test
// microphone" starts a manual recording to drive it and cancels it when
// the test stops: nothing is transcribed, pasted or kept in history.

import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { AudioLines, Loader2, Mic, RefreshCw, Square } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Block, RadioRow, Section } from "@/components/ui/section";
import { api, type AudioDeviceInfo, type AudioMeter } from "@/lib/tauri";
import { cn } from "@/lib/utils";

const SILENT: AudioMeter = { rms_db: -160, peak_db: -160 };
// Speech usually sits between -40 and -10 dBFS RMS.
const HEARING_DB = -45;

function dbToPercent(db: number): number {
  if (!isFinite(db)) return 0;
  const clamped = Math.max(-60, Math.min(0, db));
  return ((clamped + 60) / 60) * 100;
}

export function MicrophonePanel() {
  const { t } = useTranslation();
  const [devices, setDevices] = useState<AudioDeviceInfo[] | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [testing, setTesting] = useState(false);
  const [busy, setBusy] = useState(false);
  const [meter, setMeter] = useState<AudioMeter>(SILENT);
  const [heard, setHeard] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const poll = useRef<number | null>(null);

  useEffect(() => {
    refresh();
    const un = listen("recording:cancelled", () => stopPolling());
    return () => {
      stopPolling();
      un.then((fn) => fn());
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Stop an in-progress test if the user leaves the page.
  useEffect(() => {
    return () => {
      if (poll.current !== null) api.cancelRecording().catch(() => {});
    };
  }, []);

  async function refresh() {
    try {
      const [list, stored] = await Promise.all([api.listAudioDevices(), api.getSelectedInputDevice()]);
      setDevices(list);
      setSelected(stored && list.some((d) => d.name === stored) ? stored : null);
    } catch (e) {
      setError(String(e));
    }
  }

  async function choose(name: string | null) {
    setSelected(name);
    try {
      await api.setSelectedInputDevice(name);
    } catch (e) {
      setError(String(e));
    }
  }

  function stopPolling() {
    if (poll.current !== null) {
      window.clearInterval(poll.current);
      poll.current = null;
    }
    setTesting(false);
    setMeter(SILENT);
  }

  async function toggleTest() {
    setError(null);
    setBusy(true);
    try {
      if (testing) {
        await api.cancelRecording();
        stopPolling();
      } else {
        setHeard(false);
        await api.startRecording(selected);
        setTesting(true);
        poll.current = window.setInterval(async () => {
          try {
            const m = await api.getAudioMeter();
            setMeter(m);
            if (m.rms_db > HEARING_DB) setHeard(true);
          } catch {
            // meter unavailable for a tick: ignore
          }
        }, 60);
      }
    } catch (e) {
      setError(String(e));
      stopPolling();
    } finally {
      setBusy(false);
    }
  }

  const rms = dbToPercent(meter.rms_db);

  return (
    <Section
      title={t("voice.micTitle")}
      description={t("voice.micDescription")}
      action={
        <Button size="sm" variant="ghost" onClick={refresh} disabled={testing}>
          <RefreshCw className="h-3.5 w-3.5" />
          {t("recorder.refresh")}
        </Button>
      }
    >
      <div role="radiogroup" aria-label={t("voice.micTitle")} className="flex flex-col divide-y">
        {devices && devices.length === 0 ? (
          <Block className="text-sm text-muted-foreground">{t("voice.noDevices")}</Block>
        ) : (
          <>
            <RadioRow
              selected={selected === null}
              onSelect={() => choose(null)}
              disabled={testing}
              title={t("voice.sameAsWindows")}
              description={t("voice.sameAsWindowsNote")}
            />
            {(devices ?? []).map((d) => (
              <RadioRow
                key={d.name}
                selected={selected === d.name}
                onSelect={() => choose(d.name)}
                disabled={testing}
                title={d.name}
                description={d.is_default ? t("voice.windowsDefault") : undefined}
              />
            ))}
          </>
        )}
      </div>

      <Block className="flex flex-col gap-2.5">
        <div className="flex items-center justify-between gap-3">
          <span className="flex items-center gap-2 font-medium">
            <Mic className="h-4 w-4 text-muted-foreground" />
            {t("voice.levelLabel")}
            {testing && (
              <span
                className={cn(
                  "flex items-center gap-1 text-[13px] font-semibold",
                  heard ? "text-positive" : "text-muted-foreground",
                )}
              >
                <AudioLines className="h-3.5 w-3.5" />
                {heard ? t("voice.hearing") : t("voice.quiet")}
              </span>
            )}
          </span>
          <Button size="sm" variant={testing ? "outline" : "default"} onClick={toggleTest} disabled={busy}>
            {busy ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
            ) : testing ? (
              <Square className="h-3.5 w-3.5" />
            ) : (
              <Mic className="h-3.5 w-3.5" />
            )}
            {testing ? t("voice.stopTest") : t("voice.test")}
          </Button>
        </div>
        <div className="h-2.5 w-full overflow-hidden rounded-full border bg-muted">
          <div className="h-full bg-rec transition-[width] duration-75" style={{ width: `${rms}%` }} />
        </div>
        <p className="text-xs text-muted-foreground">{t("voice.levelHint")}</p>
        {error && <p className="text-xs text-destructive">{error}</p>}
      </Block>
    </Section>
  );
}
