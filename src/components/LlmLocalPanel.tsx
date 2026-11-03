import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { Check, ChevronRight, Download, HardDrive, Loader2, Trash2, Upload } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Block, Row } from "@/components/ui/section";
import { modelNotes } from "@/lib/modelText";
import { removeFromComputer, removeNote } from "@/lib/removeModel";
import { cn } from "@/lib/utils";
import {
  api,
  type GgufModelState,
  type LlamaCppSettings,
} from "@/lib/tauri";
import { confirmDelete } from "@/lib/confirmDelete";
import { formatBytes } from "@/lib/format";
import { fail, isFailure, statusText, type Status } from "@/lib/status";
import { isCancelled, translateError } from "@/lib/translateError";

type DownloadProgress = { id: string; downloaded: number; total: number };


export function LlmLocalPanel() {
  const { t } = useTranslation();
  const [models, setModels] = useState<GgufModelState[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [backend, setBackend] = useState("cpu");
  const [settings, setSettings] = useState<LlamaCppSettings>({
    context_size: 4096,
    max_tokens: 1024,
  });
  const [progress, setProgress] = useState<Record<string, DownloadProgress>>({});
  const [status, setStatus] = useState<Record<string, Status>>({});

  useEffect(() => {
    refresh();
    const unProg = listen<DownloadProgress>(
      "llm_model:download:progress",
      (e) =>
        setProgress((prev) => ({ ...prev, [e.payload.id]: e.payload })),
    );
    const unDone = listen<{ id: string; path: string }>(
      "llm_model:download:complete",
      (e) => {
        setProgress((prev) => {
          const { [e.payload.id]: _, ...rest } = prev;
          return rest;
        });
        refresh();
      },
    );
    const unErr = listen<{ id: string; message: string }>(
      "llm_model:download:error",
      (e) => {
        setProgress((prev) => {
          const { [e.payload.id]: _, ...rest } = prev;
          return rest;
        });
        setStatus((s) => ({
          ...s,
          [e.payload.id]: isCancelled(e.payload.message)
            ? t("llmLocal.cancelled")
            : fail(t("llmLocal.errorPrefix", { message: translateError(t, e.payload.message) })),
        }));
      },
    );
    return () => {
      unProg.then((fn) => fn());
      unDone.then((fn) => fn());
      unErr.then((fn) => fn());
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  async function refresh() {
    try {
      const [list, sel, usedBackend, st] = await Promise.all([
        api.listGgufModels(),
        api.getSelectedGguf(),
        api.llamacppBackend(),
        api.getLlamacppSettings(),
      ]);
      setModels(list);
      setSelectedId(sel);
      setBackend(usedBackend);
      setSettings(st);
    } catch (e) {
      console.error(e);
    }
  }

  async function download(id: string) {
    if (progress[id]) return;
    // Optimistic progress entry so the button flips to "Annuler" before
    // the first real progress event arrives. Protects against
    // double-click.
    setProgress((p) => ({ ...p, [id]: { id, downloaded: 0, total: 0 } }));
    setStatus((s) => ({ ...s, [id]: t("llmLocal.downloading") }));
    try {
      await api.downloadGgufModel(id);
      setStatus((s) => ({ ...s, [id]: "" }));
    } catch (e) {
      setProgress((p) => {
        const { [id]: _, ...rest } = p;
        return rest;
      });
      setStatus((s) => ({ ...s, [id]: fail(t("llmLocal.errorPrefix", { message: translateError(t, String(e)) })) }));
    }
  }

  async function cancelDownload(id: string) {
    await api.cancelDownloadGgufModel(id);
    // Backend emits llm_model:download:error which triggers UI cleanup.
  }

  async function remove(id: string) {
    if (!(await confirmDelete(t("llmLocal.confirmDelete", { id })))) return;
    try {
      await api.deleteGgufModel(id);
      await refresh();
    } catch (e) {
      setStatus((s) => ({ ...s, [id]: fail(t("llmLocal.errorPrefix", { message: translateError(t, String(e)) })) }));
    }
  }

  /** "Remove from this computer": Delete for every app, after a confirmation. */
  async function removeEverywhere(m: GgufModelState) {
    setStatus((s) => ({ ...s, [m.id]: "" }));
    try {
      const outcome = await removeFromComputer(t, {
        id: m.id,
        name: m.display_name,
        bytes: m.on_disk_bytes ?? m.size_bytes,
        otherApps: m.other_apps,
      });
      if (outcome === "cancelled") return;
      const note = removeNote(t, outcome, m.display_name);
      if (note) setStatus((s) => ({ ...s, [m.id]: note.failure ? fail(note.text) : note.text }));
      await refresh();
    } catch (e) {
      setStatus((s) => ({ ...s, [m.id]: fail(t("llmLocal.errorPrefix", { message: translateError(t, String(e)) })) }));
    }
  }

  async function importModel() {
    try {
      await api.importGgufModel();
      await refresh();
    } catch (e) {
      // Closing the file picker is not an error.
      if (isCancelled(e)) return;
      setStatus((s) => ({ ...s, import: fail(t("llmLocal.errorPrefix", { message: translateError(t, String(e)) })) }));
    }
  }

  async function select(id: string | null) {
    try {
      await api.setSelectedGguf(id);
      setSelectedId(id);
    } catch (e) {
      console.error(e);
    }
  }

  async function updateSetting(patch: Partial<LlamaCppSettings>) {
    const next = { ...settings, ...patch };
    setSettings(next);
    try {
      await api.setLlamacppSettings(next);
    } catch (e) {
      console.error(e);
    }
  }

  const [tuning, setTuning] = useState(false);

  // Parlato: rendered inside the "On this PC" section of the AI cleanup page
  // (rows only, no card of its own) when the built-in engine is selected.
  return (
    <>
      <Block className="flex flex-col gap-1 py-3">
        <span className="font-medium">{t("ai.builtinTitle")}</span>
        <span className="text-[13px] leading-[18px] text-pretty text-muted-foreground">
          {t("ai.builtinDesc")}{" "}
          {backend === "cpu"
            ? t("ai.runsOnCpu")
            : t("ai.runsOnGpu", { backend: backend === "metal" ? "Metal" : backend === "vulkan" ? "Vulkan" : backend })}{" "}
          {t("ai.gpuSetting")}
        </span>
      </Block>

      {models.map((m) => {
        const prog = progress[m.id];
        const st = status[m.id];
        const isSelected = m.id === selectedId;
        const pct = prog && prog.total > 0 ? Math.round((prog.downloaded / prog.total) * 100) : null;
        const meta = [
          formatBytes(m.size_bytes),
          m.context_length > 0 ? t("llmLocal.ctx", { count: m.context_length.toLocaleString() }) : null,
          m.imported ? t("llmLocal.imported") : null,
        ]
          .filter(Boolean)
          .join(" · ");
        return (
          <div key={m.id} className={cn(isSelected && "bg-accent")}>
            <Row
              label={
                <span className="flex flex-wrap items-center gap-2">
                  {m.display_name}
                  {isSelected && (
                    <span className="flex items-center gap-1 text-xs font-semibold text-positive">
                      <Check className="h-3.5 w-3.5" />
                      {t("ai.inUse")}
                    </span>
                  )}
                </span>
              }
              description={
                <>
                  {m.notes && <span className="block">{modelNotes(t, m.id, m.notes)}</span>}
                  <span className="font-mono text-xs">{meta}</span>
                </>
              }
            >
              {m.downloaded && !isSelected && (
                <Button size="sm" onClick={() => select(m.id)}>
                  {t("ai.use")}
                </Button>
              )}
              {!m.downloaded && !prog && m.downloading_elsewhere && (
                <Button size="sm" variant="outline" disabled>
                  <Loader2 className="h-3.5 w-3.5 animate-spin" />
                  {t("speech.downloadingElsewhere")}
                </Button>
              )}
              {!m.downloaded && !prog && !m.downloading_elsewhere && (
                <Button size="sm" variant="outline" onClick={() => download(m.id)}>
                  <Download className="h-3.5 w-3.5" />
                  {t("llmLocal.download")}
                </Button>
              )}
              {prog && (
                <Button size="sm" variant="ghost" onClick={() => cancelDownload(m.id)}>
                  <Loader2 className="h-3.5 w-3.5 animate-spin" />
                  {pct !== null ? `${pct}%` : ""} {t("llmLocal.cancel")}
                </Button>
              )}
              {m.downloaded && !m.imported && (
                <Button
                  size="icon"
                  variant="ghost"
                  className="text-muted-foreground"
                  onClick={() => removeEverywhere(m)}
                  title={t("removeModel.action")}
                  aria-label={t("removeModel.action")}
                >
                  <HardDrive className="h-4 w-4" />
                </Button>
              )}
              {m.downloaded && m.removable && (
                <Button
                  size="icon"
                  variant="ghost"
                  className="text-muted-foreground"
                  onClick={() => remove(m.id)}
                  title={t("ai.delete")}
                  aria-label={t("ai.delete")}
                >
                  <Trash2 className="h-4 w-4" />
                </Button>
              )}
            </Row>
            {prog && pct !== null && (
              <div className="px-5 pb-3">
                <div className="h-1.5 w-full overflow-hidden rounded-full bg-muted">
                  <div className="h-full bg-primary transition-[width]" style={{ width: `${pct}%` }} />
                </div>
                <p className="mt-1 font-mono text-[11px] text-muted-foreground">
                  {formatBytes(prog.downloaded)} / {formatBytes(prog.total)}
                </p>
              </div>
            )}
            {st && (
              <p
                className={cn(
                  "px-5 pb-3 text-xs",
                  isFailure(st) ? "text-destructive" : "text-muted-foreground",
                )}
              >
                {statusText(st)}
              </p>
            )}
          </div>
        );
      })}

      <Block className="flex flex-col gap-3 py-3">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <button
            type="button"
            onClick={() => setTuning((v) => !v)}
            aria-expanded={tuning}
            className="flex items-center gap-1.5 text-[13px] font-semibold text-muted-foreground hover:text-foreground"
          >
            <ChevronRight className={cn("h-4 w-4 transition-transform", tuning && "rotate-90")} />
            {t("ai.tuning")}
          </button>
          <Button size="sm" variant="ghost" onClick={importModel}>
            <Upload className="h-3.5 w-3.5" />
            {t("ai.import")}
          </Button>
        </div>
        {status.import && <p className="text-xs text-destructive">{statusText(status.import)}</p>}
        {tuning && (
          <div className="grid grid-cols-1 gap-3 pl-5 text-xs sm:grid-cols-2">
            <label className="grid gap-1">
              <span className="font-medium">{t("llmLocal.contextSize")}</span>
              <input
                type="number"
                min={512}
                max={131072}
                step={512}
                value={settings.context_size}
                onChange={(e) => updateSetting({ context_size: Number(e.target.value) || 4096 })}
                className="h-8 rounded-sm border-[1.5px] border-input bg-background px-2"
              />
            </label>
            <label className="grid gap-1">
              <span className="font-medium">{t("llmLocal.maxTokens")}</span>
              <input
                type="number"
                min={32}
                max={8192}
                step={32}
                value={settings.max_tokens}
                onChange={(e) => updateSetting({ max_tokens: Number(e.target.value) || 1024 })}
                className="h-8 rounded-sm border-[1.5px] border-input bg-background px-2"
              />
            </label>
          </div>
        )}
      </Block>
    </>
  );
}
