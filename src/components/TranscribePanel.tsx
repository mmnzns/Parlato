import { useState } from "react";
import { useTranslation } from "react-i18next";
import { FileText, Loader2, Play } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { api } from "@/lib/tauri";

type Props = {
  lastWavPath: string | null;
  selectedModelId: string | null;
};

export function TranscribePanel({ lastWavPath, selectedModelId }: Props) {
  const { t } = useTranslation();
  const [busy, setBusy] = useState(false);
  const [text, setText] = useState<string>("");
  const [durationMs, setDurationMs] = useState<number | null>(null);
  const [language, setLanguage] = useState<string>("auto");
  const [error, setError] = useState<string | null>(null);

  async function run() {
    if (!lastWavPath || !selectedModelId) return;
    setBusy(true);
    setError(null);
    setText("");
    setDurationMs(null);
    try {
      const res = await api.transcribeWav({
        wav_path: lastWavPath,
        model_id: selectedModelId,
        language: language === "auto" ? null : language,
      });
      setText(res.text);
      setDurationMs(res.duration_ms);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  const disabled = !lastWavPath || !selectedModelId || busy;

  return (
    <Card>
      <CardHeader>
        <div className="flex items-center gap-2">
          <FileText className="h-4 w-4 text-muted-foreground" />
          <CardTitle className="text-base">{t("transcribe.title")}</CardTitle>
        </div>
        <CardDescription>{t("transcribe.description")}</CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="flex flex-wrap items-end gap-3">
          <div>
            <label className="mb-1 block text-xs font-medium">{t("transcribe.language")}</label>
            <select
              value={language}
              onChange={(e) => setLanguage(e.target.value)}
              disabled={busy}
              className="flex h-9 rounded-md border-[1.5px] border-input bg-background px-3 py-1 text-sm disabled:opacity-50"
            >
              <option value="auto">{t("transcribe.auto")}</option>
              <option value="fr">{t("transcribe.french")}</option>
              <option value="en">{t("transcribe.english")}</option>
              <option value="es">{t("transcribe.spanish")}</option>
              <option value="de">{t("transcribe.german")}</option>
              <option value="it">{t("transcribe.italian")}</option>
              <option value="pt">{t("transcribe.portuguese")}</option>
              <option value="nl">{t("transcribe.dutch")}</option>
              <option value="ja">{t("transcribe.japanese")}</option>
              <option value="zh">{t("transcribe.chinese")}</option>
            </select>
          </div>
          <Button onClick={run} disabled={disabled}>
            {busy ? <Loader2 className="animate-spin" /> : <Play />}
            {t("transcribe.transcribe")}
          </Button>
          <div className="text-xs text-muted-foreground">
            {!lastWavPath && t("transcribe.recordFirst")}
            {lastWavPath && !selectedModelId && t("transcribe.selectModel")}
            {durationMs != null &&
              t("transcribe.completedIn", {
                seconds: (durationMs / 1000).toFixed(2),
              })}
          </div>
        </div>

        {text && (
          <div className="rounded-md border bg-muted/50 p-3 text-sm leading-relaxed">
            {text}
          </div>
        )}

        {error && (
          <div className="rounded-md bg-destructive/10 p-3 text-sm text-destructive">
            {t("transcribe.errorPrefix", { message: error })}
          </div>
        )}
      </CardContent>
    </Card>
  );
}
