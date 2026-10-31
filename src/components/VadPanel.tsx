// Detection de voix (Silero VAD) : telechargement du petit modele et
// interrupteur. Reference VoiceInk : reglage "Voice Activity Detection".
// Parlato : mis au format Section / Row des autres reglages, tailles via
// lib/format.

import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { Download, HardDrive, Loader2, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Row, Section, Switch } from "@/components/ui/section";
import { formatBytes } from "@/lib/format";
import { removeFromComputer, removeNote } from "@/lib/removeModel";
import { fail, isFailure, statusText, type Status } from "@/lib/status";
import { api } from "@/lib/tauri";
import { translateError } from "@/lib/translateError";

type VadState = {
  downloaded: boolean;
  path: string | null;
  on_disk_bytes: number | null;
  /** Another app is downloading this model right now. */
  downloading_elsewhere?: boolean;
  other_apps?: string[];
};

export function VadPanel() {
  const { t } = useTranslation();
  const [state, setState] = useState<VadState | null>(null);
  const [enabled, setEnabled] = useState(false);
  const [progress, setProgress] = useState<{ downloaded: number; total: number } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [note, setNote] = useState<Status | null>(null);

  useEffect(() => {
    refresh();
    const unlisteners = [
      listen<{ downloaded: number; total: number }>("vad:download:progress", (e) => {
        setProgress(e.payload);
      }),
      listen("vad:download:complete", async () => {
        setProgress(null);
        await refresh();
      }),
    ];
    return () => {
      Promise.all(unlisteners).then((arr) => arr.forEach((fn) => fn()));
    };
  }, []);

  async function refresh() {
    try {
      const [s, e] = await Promise.all([api.vadGetState(), api.vadIsEnabled()]);
      setState(s);
      setEnabled(e);
    } catch (e) {
      setError(String(e));
    }
  }

  async function toggle(next: boolean) {
    setEnabled(next);
    try {
      await api.vadSetEnabled(next);
    } catch (e) {
      setEnabled(!next);
      setError(String(e));
    }
  }

  async function download() {
    setError(null);
    try {
      await api.vadDownload();
    } catch (e) {
      setError(String(e));
      setProgress(null);
    }
  }

  async function remove() {
    try {
      await api.vadDelete();
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  }

  /** "Remove from this computer": Delete for every app, after a confirmation. */
  async function removeEverywhere() {
    if (!state) return;
    setError(null);
    setNote(null);
    const name = t("vad.modelTitle");
    try {
      const outcome = await removeFromComputer(t, {
        id: "vad",
        name,
        bytes: state.on_disk_bytes,
        otherApps: state.other_apps,
      });
      if (outcome === "cancelled") return;
      const n = removeNote(t, outcome, name);
      if (n) setNote(n.failure ? fail(n.text) : n.text);
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  }

  if (!state) return null;
  const pct = progress ? Math.round((progress.downloaded / Math.max(1, progress.total)) * 100) : null;

  return (
    <Section title={t("vad.title")} description={t("vad.description")}>
      <Row
        htmlFor="vad-enabled"
        label={t("vad.enableTitle")}
        description={state.downloaded ? t("vad.enableDesc") : t("vad.downloadFirst")}
        disabled={!state.downloaded}
      >
        <Switch id="vad-enabled" checked={enabled} onChange={toggle} disabled={!state.downloaded} />
      </Row>
      <Row
        label={t("vad.modelTitle")}
        description={
          state.downloaded
            ? t("vad.size", { size: formatBytes(state.on_disk_bytes) })
            : progress
              ? t("vad.downloadingLabel", { pct })
              : t("vad.notDownloaded")
        }
      >
        {state.downloaded ? (
          <div className="flex items-center gap-1">
            <Button size="sm" variant="ghost" onClick={removeEverywhere}>
              <HardDrive className="h-3.5 w-3.5" />
              {t("removeModel.action")}
            </Button>
            <Button size="sm" variant="ghost" onClick={remove}>
              <Trash2 className="h-3.5 w-3.5" />
              {t("vad.delete")}
            </Button>
          </div>
        ) : progress || state.downloading_elsewhere ? (
          <Loader2 className="h-4 w-4 animate-spin text-muted-foreground" aria-hidden />
        ) : (
          <Button size="sm" variant="outline" onClick={download}>
            <Download className="h-3.5 w-3.5" />
            {t("vad.downloadButton")}
          </Button>
        )}
      </Row>
      {note && (
        <p className={isFailure(note) ? "px-5 py-3 text-[13px] text-destructive" : "px-5 py-3 text-[13px] text-muted-foreground"}>
          {statusText(note)}
        </p>
      )}
      {error && (
        <p role="alert" className="px-5 py-3 text-[13px] text-destructive">
          {t("vad.errorPrefix", { message: translateError(t, error) })}
        </p>
      )}
    </Section>
  );
}
