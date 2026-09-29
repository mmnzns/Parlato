import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { Activity, Check, Download, Loader2, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { api } from "@/lib/tauri";

type VadState = {
  downloaded: boolean;
  path: string | null;
  on_disk_bytes: number | null;
};

function formatBytes(n: number | null | undefined): string {
  if (n == null) return "-";
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)} Mo`;
  if (n >= 1_000) return `${Math.round(n / 1_000)} Ko`;
  return `${n} o`;
}

export function VadPanel() {
  const { t } = useTranslation();
  const [state, setState] = useState<VadState | null>(null);
  const [enabled, setEnabled] = useState(false);
  const [progress, setProgress] = useState<{ downloaded: number; total: number } | null>(null);
  const [error, setError] = useState<string | null>(null);

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

  async function toggle() {
    try {
      const next = !enabled;
      await api.vadSetEnabled(next);
      setEnabled(next);
    } catch (e) {
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

  if (!state) return null;
  const pct = progress
    ? Math.round((progress.downloaded / Math.max(1, progress.total)) * 100)
    : null;

  return (
    <Card>
      <CardHeader>
        <div className="flex items-center gap-2">
          <Activity className="h-4 w-4 text-muted-foreground" />
          <CardTitle className="text-base">{t("vad.title")}</CardTitle>
        </div>
        <CardDescription>{t("vad.description")}</CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        <label className="flex cursor-pointer items-start gap-3">
          <input
            type="checkbox"
            role="switch"
            checked={enabled}
            onChange={toggle}
            className="mt-1 h-4 w-4 cursor-pointer"
            disabled={!state.downloaded}
          />
          <div>
            <p className="text-sm font-medium leading-tight">{t("vad.enableTitle")}</p>
            <p className="text-xs text-muted-foreground">
              {state.downloaded
                ? t("vad.installed")
                : t("vad.downloadFirst")}
            </p>
          </div>
        </label>

        {state.downloaded ? (
          <div className="flex items-start justify-between gap-3 rounded-md border p-3">
            <div>
              <p className="flex items-center gap-2 text-sm font-medium">
                <Check className="h-3.5 w-3.5 text-green-600 dark:text-green-400" />
                {t("vad.modelInstalled")}
              </p>
              <p className="text-xs text-muted-foreground">
                {t("vad.size", { size: formatBytes(state.on_disk_bytes) })}
              </p>
              {state.path && (
                <code className="mt-1 block break-all text-[11px] text-muted-foreground">
                  {state.path}
                </code>
              )}
            </div>
            <Button size="sm" variant="ghost" onClick={remove}>
              <Trash2 className="h-3.5 w-3.5" />
              {t("vad.delete")}
            </Button>
          </div>
        ) : progress ? (
          <div className="rounded-md border p-3">
            <div className="mb-2 flex items-center gap-2 text-sm">
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
              {t("vad.downloadingLabel", { pct })}
            </div>
            <div className="h-1.5 w-full overflow-hidden rounded-full bg-muted">
              <div
                className="h-full bg-primary transition-all"
                style={{ width: `${pct ?? 0}%` }}
              />
            </div>
          </div>
        ) : (
          <Button onClick={download} variant="outline">
            <Download /> {t("vad.downloadButton")}
          </Button>
        )}

        {error && (
          <div className="rounded-md bg-destructive/10 p-3 text-sm text-destructive">
            {t("vad.errorPrefix", { message: error })}
          </div>
        )}
      </CardContent>
    </Card>
  );
}
