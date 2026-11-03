// Parlato: title card of a mode (name, duplicate, delete, enabled) and its
// delete confirmation.

import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Copy, Trash2, Zap } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/section";
import type { PowerModeConfig } from "@/lib/tauri";

export function ModeHeader({
  config: c,
  set,
  confirming,
  onAskDelete,
  onCancelDelete,
  onDelete,
  onDuplicate,
}: {
  config: PowerModeConfig;
  set: <K extends keyof PowerModeConfig>(key: K, value: PowerModeConfig[K]) => void;
  confirming: boolean;
  onAskDelete: () => void;
  onCancelDelete: () => void;
  onDelete: () => void;
  onDuplicate: () => void;
}) {
  const { t } = useTranslation();
  const [name, setName] = useState(c.name);

  return (
    <>
      <section className="flex items-center gap-4 rounded-lg border-[1.5px] border-edge bg-card px-5 py-4">
        <span className="flex h-10 w-10 flex-none items-center justify-center rounded-md bg-highlight text-[#141416]">
          <Zap className="h-5 w-5" />
        </span>
        <input
          value={name}
          onChange={(e) => setName(e.target.value)}
          onBlur={() => name.trim() && name !== c.name && set("name", name.trim())}
          onKeyDown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()}
          aria-label={t("pm.nameLabel")}
          className="min-w-0 flex-1 rounded-sm border-[1.5px] border-transparent bg-transparent px-1.5 py-1 font-display text-lg font-bold outline-none hover:border-input focus:border-foreground"
        />
        <Button
          size="icon"
          variant="ghost"
          className="text-muted-foreground"
          onClick={onDuplicate}
          title={t("pm.duplicate")}
          aria-label={t("pm.duplicate")}
        >
          <Copy className="h-4 w-4" />
        </Button>
        <Button
          size="icon"
          variant="ghost"
          className="text-muted-foreground"
          onClick={onAskDelete}
          title={t("pm.delete")}
          aria-label={t("pm.delete")}
        >
          <Trash2 className="h-4 w-4" />
        </Button>
        <Switch checked={c.is_enabled} onChange={(v) => set("is_enabled", v)} label={t("pm.enabled")} />
      </section>

      {confirming && (
        <div
          role="alertdialog"
          aria-label={t("pm.delete")}
          className="flex flex-wrap items-center gap-3 rounded-lg border-[1.5px] border-destructive bg-card px-5 py-3"
        >
          <Trash2 className="h-4 w-4 text-destructive" />
          <span className="min-w-0 flex-1 text-[13px]">
            <b>{t("pm.confirmTitle", { name: c.name })}</b> {t("pm.confirmBody")}
          </span>
          <Button size="sm" variant="ghost" onClick={onCancelDelete}>
            {t("pm.cancel")}
          </Button>
          <Button size="sm" variant="destructive" onClick={onDelete}>
            {t("pm.confirmDelete")}
          </Button>
        </div>
      )}
    </>
  );
}
