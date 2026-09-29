import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Sliders } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { api, type TextProcessingSettings } from "@/lib/tauri";

export function PostProcessingPanel() {
  const { t } = useTranslation();
  const [settings, setSettings] = useState<TextProcessingSettings | null>(null);
  const [customWords, setCustomWords] = useState("");

  useEffect(() => {
    refresh();
  }, []);

  async function refresh() {
    try {
      const s = await api.getTextProcessingSettings();
      setSettings(s);
      setCustomWords(s.filler_words.join(", "));
    } catch (e) {
      console.error(e);
    }
  }

  async function toggle(key: keyof TextProcessingSettings) {
    if (!settings) return;
    const value = !settings[key];
    try {
      if (key === "text_formatting_enabled") await api.setTextFormattingEnabled(value);
      if (key === "remove_filler_words") await api.setRemoveFillerWords(value);
      if (key === "append_trailing_space") await api.setAppendTrailingSpace(value);
      if (key === "restore_clipboard_after_paste")
        await api.setRestoreClipboardAfterPaste(value);
      setSettings({ ...settings, [key]: value });
    } catch (e) {
      console.error(e);
    }
  }

  async function saveFillers() {
    const words = customWords
      .split(",")
      .map((w) => w.trim())
      .filter((w) => w.length > 0);
    try {
      await api.setFillerWords(words);
      await refresh();
    } catch (e) {
      console.error(e);
    }
  }

  if (!settings) return null;

  return (
    <Card>
      <CardHeader>
        <div className="flex items-center gap-2">
          <Sliders className="h-4 w-4 text-muted-foreground" />
          <CardTitle className="text-base">{t("postProcessing.title")}</CardTitle>
        </div>
        <CardDescription>{t("postProcessing.description")}</CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        <Toggle
          label={t("postProcessing.formatParagraphs")}
          description={t("postProcessing.formatParagraphsDescription")}
          checked={settings.text_formatting_enabled}
          onChange={() => toggle("text_formatting_enabled")}
        />
        <Toggle
          label={t("postProcessing.removeFillers")}
          description={t("postProcessing.removeFillersDescription")}
          checked={settings.remove_filler_words}
          onChange={() => toggle("remove_filler_words")}
        />
        <Toggle
          label={t("postProcessing.appendSpace")}
          description={t("postProcessing.appendSpaceDescription")}
          checked={settings.append_trailing_space}
          onChange={() => toggle("append_trailing_space")}
        />
        <Toggle
          label={t("postProcessing.restoreClipboard")}
          description={t("postProcessing.restoreClipboardDescription")}
          checked={settings.restore_clipboard_after_paste}
          onChange={() => toggle("restore_clipboard_after_paste")}
        />

        <div className="space-y-2 rounded-md border p-3">
          <label className="block text-sm font-medium">
            {t("postProcessing.fillersLabel")}
          </label>
          <div className="flex gap-2">
            <input
              value={customWords}
              onChange={(e) => setCustomWords(e.target.value)}
              className="flex h-9 flex-1 rounded-md border-[1.5px] border-input bg-background px-3 text-sm"
            />
            <Button size="sm" variant="outline" onClick={saveFillers}>
              {t("postProcessing.save")}
            </Button>
          </div>
          <p className="text-xs text-muted-foreground">
            {t("postProcessing.fillersDefault")}
          </p>
        </div>
      </CardContent>
    </Card>
  );
}

type ToggleProps = {
  label: string;
  description: string;
  checked: boolean;
  onChange: () => void;
};

function Toggle({ label, description, checked, onChange }: ToggleProps) {
  return (
    <label className="flex cursor-pointer items-start gap-3">
      <input
        type="checkbox"
        role="switch"
        checked={checked}
        onChange={onChange}
        className="mt-1 h-4 w-4 cursor-pointer"
      />
      <div>
        <p className="text-sm font-medium leading-tight">{label}</p>
        <p className="text-xs text-muted-foreground">{description}</p>
      </div>
    </label>
  );
}
