// Parlato : zone "Essayer" de la page Nettoyage IA. Montre ce que le modele
// et le style choisis font d'un texte, sans dicter. Comme VoiceInk, Wispr
// Flow et Superwhisper.

import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Loader2, Sparkles } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Section } from "@/components/ui/section";
import { api } from "@/lib/tauri";
import { translateError } from "@/lib/translateError";

export function EnhancementTryBox() {
  const { t } = useTranslation();
  const [input, setInput] = useState("");
  const [output, setOutput] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [running, setRunning] = useState(false);

  async function run() {
    const text = input.trim();
    if (!text) return;
    setRunning(true);
    setError(null);
    setOutput(null);
    try {
      setOutput(await api.tryEnhancement(text));
    } catch (e) {
      setError(translateError(t, String(e)));
    } finally {
      setRunning(false);
    }
  }

  return (
    <Section title={t("ai.tryTitle")} description={t("ai.tryDesc")}>
      <div className="flex flex-col gap-3 px-5 py-4">
        <label htmlFor="ai-try-input" className="sr-only">
          {t("ai.tryTitle")}
        </label>
        <textarea
          id="ai-try-input"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          placeholder={t("ai.tryPlaceholder")}
          rows={3}
          className="w-full resize-y rounded-sm border-[1.5px] border-input bg-background px-3 py-2 text-sm"
        />
        <div>
          <Button size="sm" onClick={run} disabled={running || !input.trim()}>
            {running ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <Sparkles className="h-3.5 w-3.5" />}
            {t("ai.tryButton")}
          </Button>
        </div>
        {output !== null && (
          <p
            aria-live="polite"
            className="rounded-sm bg-muted px-3 py-2 text-sm leading-6 whitespace-pre-wrap"
          >
            {output}
          </p>
        )}
        {error && (
          <p role="alert" className="text-[13px] text-destructive">
            {error}
          </p>
        )}
      </div>
    </Section>
  );
}
