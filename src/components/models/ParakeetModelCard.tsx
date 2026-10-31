// Card modele Parakeet : meta int8/F16, multilingue, missing files,
// download avec fichier courant, et le bouton d'action a 3 etats
// VoiceInk : Download -> Set as Default -> Default Model.
//
// Parlato: rendered through the Workbench ModelTile.

import { useTranslation } from "react-i18next";
import { Download, HardDrive, Loader2, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { ModelTile, TileProgress } from "@/components/models/ModelTile";
import { modelName, modelNotes } from "@/lib/modelText";
import { cn } from "@/lib/utils";
import type { ParakeetModelState } from "@/lib/tauri";
import type { ParakeetDownloadProgress } from "./types";

import { formatBytes } from "@/lib/format";
import { isFailure, statusText, type Status } from "@/lib/status";

type Props = {
  /** Parlato: plain title from companies.ts ("Best for English"). */
  pick?: string;
  model: ParakeetModelState;
  isCurrent: boolean;
  progress: ParakeetDownloadProgress | null;
  status: Status | null;
  onDownload: () => void;
  onCancelDownload: () => void;
  onDelete: () => void;
  /** "Remove from this computer": Delete for every app. */
  onRemoveEverywhere: () => void;
  onSetDefault: () => void;
};

export function ParakeetModelCard({
  model: m,
  pick,
  isCurrent,
  progress: prog,
  status: st,
  onDownload,
  onCancelDownload,
  onDelete,
  onRemoveEverywhere,
  onSetDefault,
}: Props) {
  const { t } = useTranslation();
  const pct = prog && prog.total > 0 ? Math.round((prog.downloaded / prog.total) * 100) : null;
  const meta = [
    m.multilingual
      ? t("speech.languages", { count: m.language_codes.filter((c) => c !== "auto").length })
      : t("speech.englishOnly"),
    formatBytes(m.size_bytes),
    m.is_quantized ? "int8" : "F16",
    m.downloaded ? t("speech.downloaded") : null,
  ]
    .filter(Boolean)
    .join(" · ");

  return (
    <ModelTile
      current={isCurrent}
      pick={pick}
      local
      name={modelName(t, m.id, m.display_name)}
      tech={m.id}
      description={modelNotes(t, m.id, m.notes) || undefined}
      speed={m.speed}
      accuracy={m.accuracy}
      meta={meta}
      trailing={
        m.downloaded ? (
          <div className="flex items-center">
            <Button
              size="icon"
              variant="ghost"
              className="text-muted-foreground"
              onClick={onRemoveEverywhere}
              title={t("removeModel.action")}
              aria-label={t("removeModel.action")}
            >
              <HardDrive className="h-4 w-4" />
            </Button>
            <Button
              size="icon"
              variant="ghost"
              className="text-muted-foreground"
              onClick={onDelete}
              title={t("speech.delete")}
              aria-label={t("speech.delete")}
            >
              <Trash2 className="h-4 w-4" />
            </Button>
          </div>
        ) : undefined
      }
      action={
        m.downloaded ? (
          <Button size="sm" onClick={onSetDefault}>
            {t("speech.use")}
          </Button>
        ) : prog ? (
          <Button size="sm" variant="outline" onClick={onCancelDownload}>
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
            {t("speech.cancel")}
          </Button>
        ) : m.downloading_elsewhere ? (
          <Button size="sm" variant="outline" disabled>
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
            {t("speech.downloadingElsewhere")}
          </Button>
        ) : (
          <Button size="sm" variant="outline" onClick={onDownload}>
            <Download className="h-3.5 w-3.5" />
            {t("speech.download")}
          </Button>
        )
      }
    >
      {prog && pct !== null && (
        <TileProgress
          pct={pct}
          label={`${formatBytes(prog.downloaded)} / ${formatBytes(prog.total)} (${pct}%)`}
        />
      )}
      {st && (
        <p
          className={cn(
            "text-xs",
            isFailure(st) ? "text-destructive" : "text-muted-foreground",
          )}
        >
          {statusText(st)}
        </p>
      )}
    </ModelTile>
  );
}
