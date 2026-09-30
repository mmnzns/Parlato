// Card modele Whisper local : badges (EN, importe, installe), tailles,
// download/progress/cancel/delete, et le bouton d'action a 3 etats
// VoiceInk : Download -> Set as Default -> Default Model.
//
// Parlato: rendered through the Workbench ModelTile.

import { useTranslation } from "react-i18next";
import { Download, Trash2, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { ModelTile, TileProgress } from "@/components/models/ModelTile";
import type { DownloadProgress, WhisperModelState } from "@/lib/tauri";

function formatBytes(bytes: number | null | undefined): string {
  if (bytes == null) return "-";
  if (bytes >= 1_000_000_000) return `${(bytes / 1_000_000_000).toFixed(1)} GB`;
  if (bytes >= 1_000_000) return `${(bytes / 1_000_000).toFixed(0)} MB`;
  return `${Math.round(bytes / 1024)} KB`;
}

type Props = {
  model: WhisperModelState;
  isCurrent: boolean;
  progress: DownloadProgress | null;
  error: string | null;
  onDownload: () => void;
  onCancelDownload: () => void;
  onDelete: () => void;
  onSetDefault: () => void;
};

export function WhisperModelCard({
  model: m,
  isCurrent,
  progress: p,
  error,
  onDownload,
  onCancelDownload,
  onDelete,
  onSetDefault,
}: Props) {
  const { t } = useTranslation();
  const pct = p ? Math.round((p.downloaded / Math.max(1, p.total)) * 100) : null;
  const meta = [
    m.multilingual || m.imported
      ? m.language_codes.length > 1
        ? t("speech.languages", { count: m.language_codes.filter((c) => c !== "auto").length })
        : null
      : t("speech.englishOnly"),
    formatBytes(m.downloaded && m.on_disk_bytes != null ? m.on_disk_bytes : m.size_bytes),
    m.imported ? t("whisperModels.importedBadge") : null,
    m.downloaded ? t("speech.downloaded") : null,
  ]
    .filter(Boolean)
    .join(" · ");

  const deleteButton = (
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
  );

  return (
    <ModelTile
      current={isCurrent}
      kind="Whisper"
      local
      name={m.display_name}
      tech={m.id}
      speed={m.speed}
      accuracy={m.accuracy}
      description={m.notes || undefined}
      meta={meta}
      trailing={m.downloaded ? deleteButton : undefined}
      action={
        m.downloaded ? (
          <Button size="sm" onClick={onSetDefault}>
            {t("speech.use")}
          </Button>
        ) : p ? (
          <Button size="sm" variant="outline" onClick={onCancelDownload}>
            <X className="h-3.5 w-3.5" />
            {t("speech.cancel")}
          </Button>
        ) : (
          <Button size="sm" variant="outline" onClick={onDownload}>
            <Download className="h-3.5 w-3.5" />
            {t("speech.download")}
          </Button>
        )
      }
    >
      {p && <TileProgress pct={pct} label={`${pct ?? 0}%`} />}
      {error && (
        <p className="text-xs text-destructive">{t("whisperModels.errorPrefix", { message: error })}</p>
      )}
    </ModelTile>
  );
}
