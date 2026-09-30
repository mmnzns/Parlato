// Parlato: "Transcribe a file", rebuilt to the Workbench design
// (docs/design/v1, screen "transcribe"): a drop zone, the options that
// apply (speech model, language, AI cleanup), and the list of files.
//
// Any audio or video file is converted to 16 kHz by the backend
// (transcription::audio_file) and runs through the normal pipeline with the
// active speech model, so results also land in History and the clipboard.

import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { Check, Copy, FileAudio, Loader2, TriangleAlert, Upload, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Block, Row, Section } from "@/components/ui/section";
import { DictationLanguagePanel } from "@/components/DictationLanguagePanel";
import { resolveDefaultDisplayName } from "@/components/models/types";
import { useCopyButton } from "@/hooks/useCopyButton";
import { removeFileJob, startFileJob, useFileJobs, type FileJob } from "@/lib/fileJobs";
import { api, type TranscriptionSource } from "@/lib/tauri";
import { translateError } from "@/lib/translateError";
import { cn } from "@/lib/utils";

/** Extensions symphonia can decode (see Cargo.toml). Opus/webm is not supported. */
const EXTENSIONS = ["mp3", "wav", "m4a", "aac", "flac", "ogg", "oga", "aif", "aiff", "caf", "mp4", "m4v", "mov", "mkv"];

function formatClock(secs: number): string {
  const total = Math.round(secs);
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = (total % 60).toString().padStart(2, "0");
  return h > 0 ? `${h}:${m.toString().padStart(2, "0")}:${s}` : `${m}:${s}`;
}

export function TranscribePanel({ onNavigate }: { onNavigate: (view: "models" | "enhancement") => void }) {
  const { t } = useTranslation();
  const { jobs, busy } = useFileJobs();
  const [dragging, setDragging] = useState(false);
  const [modelName, setModelName] = useState<string | null>(null);
  const [source, setSource] = useState<TranscriptionSource | null>(null);
  const [aiOn, setAiOn] = useState(false);

  useEffect(() => {
    refreshOptions();
    const unSource = listen("source:changed", () => refreshOptions());
    // Native file drop: the webview reports real file paths.
    const unDrop = getCurrentWebview().onDragDropEvent((e) => {
      const p = e.payload;
      if (p.type === "enter" || p.type === "over") setDragging(true);
      else if (p.type === "leave") setDragging(false);
      else if (p.type === "drop") {
        setDragging(false);
        const file = p.paths.find((f) => EXTENSIONS.includes(f.split(".").pop()?.toLowerCase() ?? ""));
        if (file ?? p.paths[0]) startFileJob(file ?? p.paths[0]);
      }
    });
    return () => {
      unSource.then((fn) => fn());
      unDrop.then((fn) => fn());
    };
  }, []);

  async function refreshOptions() {
    try {
      const [src, w, pk, cm, en] = await Promise.all([
        api.getTranscriptionSource(),
        api.listWhisperModels(),
        api.listParakeetModels(),
        api.listCloudModels(),
        api.getEnhancementEnabled(),
      ]);
      setSource(src);
      setModelName(resolveDefaultDisplayName(src, w, pk, cm, t));
      setAiOn(en);
    } catch (e) {
      console.error(e);
    }
  }

  async function chooseFile() {
    const picked = await openDialog({
      multiple: false,
      title: t("tf.dialogTitle"),
      filters: [{ name: t("tf.filterName"), extensions: EXTENSIONS }],
    });
    if (typeof picked === "string") startFileJob(picked);
  }

  const [before, after] = t("tf.dropOr", { choose: "\u0000" }).split("\u0000");

  return (
    <>
      <button
        type="button"
        onClick={chooseFile}
        disabled={busy}
        className={cn(
          "flex flex-col items-center gap-1.5 rounded-lg border-[1.5px] border-dashed bg-card px-6 py-9 text-center transition-colors disabled:cursor-not-allowed disabled:opacity-60",
          dragging ? "border-edge bg-accent" : "border-input hover:border-edge",
        )}
      >
        <span className="mb-1 flex h-11 w-11 items-center justify-center rounded-md bg-highlight text-[#141416]">
          <Upload className="h-5 w-5" />
        </span>
        <span className="font-display text-base font-bold">
          {dragging ? t("tf.dropActive") : t("tf.dropTitle")}
        </span>
        <span className="text-[13px] text-muted-foreground">
          {before}
          <u className="font-semibold text-foreground underline-offset-[3px]">{t("tf.choose")}</u>
          {after}
        </span>
        <span className="font-mono text-[11px] text-muted-foreground">{t("tf.formats")}</span>
        {busy && <span className="mt-1 text-xs text-muted-foreground">{t("tf.busyNote")}</span>}
      </button>

      <Section title={t("tf.optionsTitle")}>
        <Row label={t("tf.model")} description={modelName ?? t("tf.noModel")}>
          <Button size="sm" variant="outline" onClick={() => onNavigate("models")}>
            {t("tf.change")}
          </Button>
        </Row>
        {modelName && (
          <Row label={t("speech.language")}>
            <DictationLanguagePanel bare />
          </Row>
        )}
        <Row label={t("tf.aiCleanup")} description={aiOn ? t("tf.aiOn") : t("tf.aiOff")}>
          <Button size="sm" variant="outline" onClick={() => onNavigate("enhancement")}>
            {t("tf.change")}
          </Button>
        </Row>
        {source?.kind === "cloud" && (
          <Block className="py-3 text-xs text-muted-foreground">{t("tf.cloudNote")}</Block>
        )}
      </Section>

      <Section title={t("tf.filesTitle")} description={t("tf.filesDesc")}>
        {jobs.length === 0 ? (
          <Block className="text-sm text-muted-foreground">{t("tf.emptyFiles")}</Block>
        ) : (
          jobs.map((job) => <FileRow key={job.id} job={job} />)
        )}
      </Section>
    </>
  );
}

function FileRow({ job }: { job: FileJob }) {
  const { t } = useTranslation();
  const working = job.status === "converting" || job.status === "transcribing" || job.status === "cleaning";
  const statusLabel =
    job.status === "converting"
      ? t("tf.converting")
      : job.status === "transcribing"
        ? t("tf.transcribing")
        : t("tf.cleaning");
  const empty = job.status === "done" && !job.text?.trim();

  return (
    <div className="flex items-start gap-4 px-5 py-4">
      <span className="mt-0.5 flex h-9 w-9 flex-none items-center justify-center rounded-md bg-muted">
        <FileAudio className="h-4 w-4" />
      </span>
      <div className="flex min-w-0 flex-1 flex-col gap-1">
        <div className="flex flex-wrap items-baseline gap-x-3 gap-y-0.5">
          <span className="truncate font-medium" title={job.name}>
            {job.name}
          </span>
          {job.durationSec != null && (
            <span className="font-mono text-xs text-muted-foreground">{formatClock(job.durationSec)}</span>
          )}
        </div>
        {working && (
          <span className="flex items-center gap-1.5 text-[13px] text-muted-foreground">
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
            {statusLabel}
          </span>
        )}
        {job.status === "failed" && (
          <span role="alert" className="flex items-start gap-1.5 text-[13px] text-destructive">
            <TriangleAlert className="mt-0.5 h-3.5 w-3.5 flex-none" />
            {translateError(t, job.error ?? "")}
          </span>
        )}
        {job.status === "done" &&
          (empty ? (
            <span className="text-[13px] text-muted-foreground">{t("tf.noSpeech")}</span>
          ) : (
            <p className="line-clamp-3 text-[13px] leading-5 text-pretty whitespace-pre-wrap">{job.text}</p>
          ))}
      </div>
      <div className="flex flex-none items-center gap-1.5">
        {job.status === "done" && !empty && <CopyText text={job.text ?? ""} />}
        {!working && (
          <Button
            size="icon"
            variant="ghost"
            className="text-muted-foreground"
            onClick={() => removeFileJob(job.id)}
            title={t("tf.remove")}
            aria-label={t("tf.remove")}
          >
            <X className="h-4 w-4" />
          </Button>
        )}
      </div>
    </div>
  );
}

function CopyText({ text }: { text: string }) {
  const { t } = useTranslation();
  const { copied, copy } = useCopyButton();
  return (
    <Button size="sm" variant="outline" onClick={() => copy(text)} className={cn(copied && "text-positive")}>
      {copied ? <Check className="h-3.5 w-3.5" /> : <Copy className="h-3.5 w-3.5" />}
      {copied ? t("tf.copied") : t("tf.copy")}
    </Button>
  );
}
