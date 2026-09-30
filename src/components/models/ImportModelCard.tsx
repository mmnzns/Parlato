// Card d'import de modele whisper local - replique VoiceInk
// "Import Local Model" (card en fin de liste Local) + InfoTip.
// Dialog fichier .bin -> commande import_whisper_model.

import { useState } from "react";
import { useTranslation } from "react-i18next";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { FolderOpen, Upload } from "lucide-react";
import { Button } from "@/components/ui/button";
import { InfoTip } from "@/components/ui/info-tip";
import { api } from "@/lib/tauri";

export function ImportModelCard({
  onImported,
}: {
  onImported: (id: string) => void;
}) {
  const { t } = useTranslation();
  const [error, setError] = useState<string | null>(null);

  async function importModel() {
    try {
      const selected = await openDialog({
        multiple: false,
        filters: [
          { name: t("whisperModels.dialogFilter"), extensions: ["bin"] },
        ],
        title: t("whisperModels.dialogTitle"),
      });
      if (!selected || typeof selected !== "string") return;
      const newId = await api.importWhisperModel(selected);
      setError(null);
      onImported(newId);
    } catch (e) {
      setError(String(e));
    }
  }

  return (
    <div className="flex flex-col gap-2 rounded-lg border-[1.5px] border-dashed border-input bg-card px-5 py-4">
      <div className="flex items-center gap-4">
        <FolderOpen className="h-5 w-5 flex-none text-muted-foreground" />
        <div className="flex min-w-0 flex-1 flex-col gap-0.5">
          <span className="flex items-center gap-1.5 font-medium">
            {t("speech.importTitle")}
            <InfoTip learnMoreUrl="https://tryvoiceink.com/docs/custom-local-whisper-models">
              {t("aiModels.import.infoTip")}
            </InfoTip>
          </span>
          <span className="text-[13px] text-muted-foreground">{t("speech.importDesc")}</span>
        </div>
        <Button size="sm" variant="outline" onClick={importModel}>
          <Upload className="h-3.5 w-3.5" />
          {t("speech.browse")}
        </Button>
      </div>
      {error && <p className="text-xs text-destructive">{t("aiModels.import.error", { message: error })}</p>}
    </div>
  );
}
