import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { Check, ChevronRight, Download, Loader2, Trash2, Upload } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Block, Row } from "@/components/ui/section";
import { cn } from "@/lib/utils";
import {
  api,
  type GgufModelState,
  type LlamaCppSettings,
} from "@/lib/tauri";

type DownloadProgress = { id: string; downloaded: number; total: number };

function formatBytes(b: number): string {
  if (b < 1024) return `${b} B`;
  if (b < 1024 * 1024) return `${(b / 1024).toFixed(1)} KB`;
  if (b < 1024 * 1024 * 1024) return `${(b / 1024 / 1024).toFixed(1)} MB`;
  return `${(b / 1024 / 1024 / 1024).toFixed(2)} GB`;
}

export function LlmLocalPanel() {
  const { t } = useTranslation();
  const [models, setModels] = useState<GgufModelState[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [cudaEnabled, setCudaEnabled] = useState(false);
  const [settings, setSettings] = useState<LlamaCppSettings>({
    n_gpu_layers: 0,
    context_size: 4096,
    max_tokens: 1024,
  });
  const [progress, setProgress] = useState<Record<string, DownloadProgress>>({});
  const [status, setStatus] = useState<Record<string, string>>({});

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
          [e.payload.id]: e.payload.message.includes("annule")
            ? t("llmLocal.cancelled")
            : t("llmLocal.errorPrefix", { message: e.payload.message }),
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
      const [list, sel, cuda, st] = await Promise.all([
        api.listGgufModels(),
        api.getSelectedGguf(),
        api.llamacppCudaEnabled(),
        api.getLlamacppSettings(),
      ]);
      setModels(list);
      setSelectedId(sel);
      setCudaEnabled(cuda);
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
      setStatus((s) => ({ ...s, [id]: t("llmLocal.errorPrefix", { message: String(e) }) }));
    }
  }

  async function cancelDownload(id: string) {
    await api.cancelDownloadGgufModel(id);
    // Backend emits llm_model:download:error which triggers UI cleanup.
  }

  async function remove(id: string) {
    if (!confirm(t("llmLocal.confirmDelete", { id }))) return;
    try {
      await api.deleteGgufModel(id);
      await refresh();
    } catch (e) {
      setStatus((s) => ({ ...s, [id]: t("llmLocal.errorPrefix", { message: String(e) }) }));
    }
  }

  async function importModel() {
    try {
      await api.importGgufModel();
      await refresh();
    } catch (e) {
      setStatus((s) => ({ ...s, import: t("llmLocal.errorPrefix", { message: String(e) }) }));
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
          {cudaEnabled ? t("llmLocal.cudaBuild") : t("ai.runsOnCpu")}
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
                  {m.notes && <span className="block">{m.notes}</span>}
                  <span className="font-mono text-xs">{meta}</span>
                </>
              }
            >
              {m.downloaded && !isSelected && (
                <Button size="sm" onClick={() => select(m.id)}>
                  {t("ai.use")}
                </Button>
              )}
              {!m.downloaded && !prog && (
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
              {m.downloaded && (
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
                  st.startsWith(t("common.error")) ? "text-destructive" : "text-muted-foreground",
                )}
              >
                {st}
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
        {status.import && <p className="text-xs text-destructive">{status.import}</p>}
        {tuning && (
          <div className="grid grid-cols-1 gap-3 pl-5 text-xs sm:grid-cols-3">
            <label className="grid gap-1">
              <span className="font-medium">{t("llmLocal.nGpuLayers")}</span>
              <input
                type="number"
                min={0}
                max={200}
                value={settings.n_gpu_layers}
                onChange={(e) => updateSetting({ n_gpu_layers: Number(e.target.value) || 0 })}
                className="h-8 rounded-sm border-[1.5px] border-input bg-background px-2"
              />
              <span className="text-muted-foreground">{t("llmLocal.nGpuLayersHelp")}</span>
            </label>
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
