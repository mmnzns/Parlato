// Card modele cloud - replique fidele de VoiceInk CloudModelCardView :
// bouton d'action a 3 etats (Configure -> Set as Default -> Default Model),
// section de configuration de la cle API inline depliable, menu
// "Remove API Key". Verifier une cle la sauvegarde mais n'active jamais le
// modele : l'activation reste le geste explicite "Set as Default" (issue #6,
// meme comportement que VoiceInk).
//
// Divergence assumee : pas de pre-remplissage de la cle sauvegardee (le
// Credential Manager n'est pas relisible cote front, seul has_api_key
// existe). Changer de cle = Remove API Key puis Configure.

import { useState } from "react";
import { useTranslation } from "react-i18next";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ExternalLink, KeyRound, Loader2, ShieldCheck, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { ModelTile } from "@/components/models/ModelTile";
import { modelNotes } from "@/lib/modelText";
import type { CloudModel } from "./types";

type VerifyStatus = "none" | "success" | "failure";

type Props = {
  /** Parlato: plain title from companies.ts ("Best for English"). */
  pick?: string;
  model: CloudModel;
  providerName: string;
  apiKeyUrl: string | null;
  isConfigured: boolean;
  isCurrent: boolean;
  /** Verifie puis sauvegarde la cle ; doit throw si la verification echoue. */
  onVerifyAndSave: (key: string) => Promise<void>;
  onSetDefault: () => void;
  onRemoveKey: () => void;
};

export function CloudModelCard({
  model: m,
  pick,
  providerName,
  apiKeyUrl,
  isConfigured,
  isCurrent,
  onVerifyAndSave,
  onSetDefault,
  onRemoveKey,
}: Props) {
  const { t } = useTranslation();
  const [isExpanded, setIsExpanded] = useState(false);
  const [apiKey, setApiKey] = useState("");
  const [verifying, setVerifying] = useState(false);
  const [verifyStatus, setVerifyStatus] = useState<VerifyStatus>("none");
  const [verifyError, setVerifyError] = useState<string | null>(null);

  async function verify() {
    const key = apiKey.trim();
    if (!key || verifying) return;
    setVerifying(true);
    setVerifyStatus("none");
    setVerifyError(null);
    try {
      await onVerifyAndSave(key);
      // Cle valide et sauvee : la section se referme et le bouton de la
      // card devient "Set as Default" (isConfigured passe a true au
      // refresh du parent) - comme VoiceInk.
      setVerifyStatus("success");
      setApiKey("");
      setIsExpanded(false);
    } catch (e) {
      setVerifyStatus("failure");
      setVerifyError(String(e));
    } finally {
      setVerifying(false);
    }
  }

  const meta = [
    m.multilingual
      ? t("speech.languages", { count: m.language_codes.filter((c) => c !== "auto").length })
      : t("speech.englishOnly"),
    m.supports_streaming ? t("speech.liveWords") : null,
  ]
    .filter(Boolean)
    .join(" · ");

  return (
    <ModelTile
      current={isCurrent}
      pick={pick}
      local={false}
      name={m.display_name}
      tech={`${m.provider_id} · ${m.model_id}`}
      description={modelNotes(t, m.model_id, m.notes) || undefined}
      speed={m.speed}
      accuracy={m.accuracy}
      meta={meta}
      trailing={
        isConfigured ? (
          <Button
            size="icon"
            variant="ghost"
            className="text-muted-foreground"
            onClick={onRemoveKey}
            title={t("speech.removeKey")}
            aria-label={t("speech.removeKey")}
          >
            <Trash2 className="h-4 w-4" />
          </Button>
        ) : undefined
      }
      action={
        isConfigured ? (
          <Button size="sm" onClick={onSetDefault}>
            {t("speech.use")}
          </Button>
        ) : (
          <Button size="sm" variant="outline" onClick={() => setIsExpanded((v) => !v)} aria-expanded={isExpanded}>
            <KeyRound className="h-3.5 w-3.5" />
            {t("speech.addKey")}
          </Button>
        )
      }
    >
      {isExpanded && !isConfigured && (
        <div className="flex flex-col gap-2 border-t border-dashed pt-3">
          <div className="flex items-center gap-2">
            <input
              type="password"
              placeholder={t("aiModels.cloud.apiKeyPlaceholder", { provider: providerName })}
              aria-label={t("aiModels.cloud.apiKeyConfig")}
              value={apiKey}
              onChange={(e) => setApiKey(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") verify();
              }}
              disabled={verifying}
              className="h-[34px] min-w-0 flex-1 rounded-sm border-[1.5px] border-input bg-background px-3 font-mono text-sm placeholder:font-sans"
              autoComplete="off"
            />
            <Button size="sm" onClick={verify} disabled={!apiKey.trim() || verifying}>
              {verifying ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <ShieldCheck className="h-3.5 w-3.5" />}
              {verifying ? t("aiModels.cloud.verifying") : t("aiModels.cloud.verify")}
            </Button>
          </div>
          <span className="text-[11px] text-muted-foreground">{t("speech.keyNote")}</span>
          {apiKeyUrl && (
            <button
              type="button"
              onClick={() => openUrl(apiKeyUrl)}
              className="inline-flex items-center gap-1 self-start text-xs font-semibold text-foreground underline underline-offset-[3px]"
            >
              <ExternalLink className="h-3 w-3" />
              {t("aiModels.getApiKey")}
            </button>
          )}
          {verifyStatus === "failure" && (
            <p role="alert" className="text-xs text-destructive">
              {verifyError ?? t("aiModels.cloud.verifyFailed")}
            </p>
          )}
        </div>
      )}
      {verifyStatus === "success" && isConfigured && !isCurrent && (
        <p className="text-xs text-positive">{t("aiModels.cloud.verifySuccess")}</p>
      )}
    </ModelTile>
  );
}
