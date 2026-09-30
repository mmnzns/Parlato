// Section OCR screen context de l'EnhancementPanel.
//
// Extraite pour isoler son etat local (enabled, preview text, status) et
// reduire la taille de EnhancementPanel. Pas de props d'etat : le composant
// gere tout en interne en dialoguant avec l'API Tauri.

import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { ScanText } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/section";
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

  // Parlato: one Workbench bar (icon, text, Test, switch) with the
  // captured preview underneath when there is one.
  return (
    <div className="rounded-lg border-[1.5px] border-edge bg-card">
      <div className="flex items-center gap-4 px-5 py-4">
        <ScanText className="h-5 w-5 flex-none text-muted-foreground" />
        <div className="flex min-w-0 flex-1 flex-col gap-0.5">
          <span className="font-medium">{t("ai.screenTitle")}</span>
          <span className="text-[13px] leading-[18px] text-pretty text-muted-foreground">{t("ai.screenDesc")}</span>
        </div>
        <Button size="sm" variant="outline" onClick={runPreview}>
          {t("ai.test")}
        </Button>
        <Switch checked={enabled} onChange={toggle} label={t("ai.screenTitle")} />
      </div>
      {(status || preview) && (
        <div className="flex flex-col gap-2 border-t px-5 py-3">
          {status && (
            <span
              className={cn(
                "text-xs",
                status.startsWith(t("common.error")) ? "text-destructive" : "text-muted-foreground",
              )}
            >
              {status}
            </span>
          )}
          {preview && (
            <>
              <pre className="max-h-40 overflow-auto rounded-sm bg-muted p-2 font-mono text-[11px] whitespace-pre-wrap">
                {preview}
              </pre>
              <Button size="sm" variant="ghost" className="self-start" onClick={clearCache}>
                {t("ai.clear")}
              </Button>
            </>
          )}
        </div>
      )}
    </div>
  );
}
