// Section OCR screen context de l'EnhancementPanel.
//
// Extraite pour isoler son etat local (enabled, preview text, status) et
// reduire la taille de EnhancementPanel. Pas de props d'etat : le composant
// gere tout en interne en dialoguant avec l'API Tauri.

import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { api } from "@/lib/tauri";

export function EnhancementScreenContext() {
  const { t } = useTranslation();
  const [enabled, setEnabled] = useState(false);
  const [preview, setPreview] = useState<string | null>(null);
  const [status, setStatus] = useState("");

  useEffect(() => {
    refresh();
  }, []);

  async function refresh() {
    try {
      const [en, cached] = await Promise.all([
        api.getScreenContextEnabled(),
        api.getScreenContextCached(),
      ]);
      setEnabled(en);
      setPreview(cached);
    } catch (e) {
      console.error(e);
    }
  }

  async function toggle(v: boolean) {
    setEnabled(v);
    try {
      await api.setScreenContextEnabled(v);
    } catch (e) {
      setStatus(t("screenContext.errorPrefix", { message: String(e) }));
    }
  }

  async function runPreview() {
    setStatus(t("screenContext.capturing"));
    try {
      const text = await api.captureScreenContextPreview();
      setPreview(text);
      setStatus(t("screenContext.okStatus"));
    } catch (e) {
      setStatus(t("screenContext.errorPrefix", { message: String(e) }));
    }
  }

  async function clearCache() {
    await api.clearScreenContext();
    setPreview(null);
    setStatus(t("screenContext.cacheCleared"));
  }

  return (
    <div className="grid gap-2 rounded-md border p-3">
      <label className="flex items-center justify-between">
        <div>
          <p className="text-sm font-medium">{t("screenContext.title")}</p>
          <p className="text-xs text-muted-foreground">
            {t("screenContext.description")}
            <code>&lt;CURRENT_WINDOW_CONTEXT&gt;</code>
            {t("screenContext.descriptionEnd")}
          </p>
        </div>
        <input
          type="checkbox"
          role="switch"
          checked={enabled}
          onChange={(e) => toggle(e.target.checked)}
          className="h-5 w-5"
        />
      </label>
      <div className="flex flex-wrap items-center gap-2">
        <Button size="sm" variant="outline" onClick={runPreview}>
          {t("screenContext.testCapture")}
        </Button>
        {preview && (
          <Button size="sm" variant="ghost" onClick={clearCache}>
            {t("screenContext.clearCache")}
          </Button>
        )}
        {status && (
          <span
            className={cn(
              "text-xs",
              status.startsWith(t("common.error"))
                ? "text-destructive"
                : "text-muted-foreground",
            )}
          >
            {status}
          </span>
        )}
      </div>
      {preview && (
        <pre className="max-h-40 overflow-auto rounded bg-muted/40 p-2 text-[11px] font-mono whitespace-pre-wrap">
          {preview}
        </pre>
      )}
    </div>
  );
}
