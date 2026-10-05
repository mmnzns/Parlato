// Page AI Models - refonte fidele a VoiceInk ModelManagementView :
// header "Default Model", langue de dictee, filtres pills purement visuels
// (Recommended / Local / Cloud) et liste unifiee de cards de modeles.
//
// L'activation est toujours un geste explicite ("Set as Default" sur une
// card) ; configurer/verifier une cle API ne change jamais la source
// active (issue #6, comportement VoiceInk). Changer de filtre ne change
// jamais la source non plus - seul le modele actif compte, il est rappele
// dans le header et surligne dans la liste.
//
// Parlato: Workbench layout (docs/design/v1, screen "model"): an "in use"
// bar with the language picker, then models organised by company (spec
// docs/superpowers/specs/2026-09-29-speech-model-by-company-design.md): a
// company list on the left (on this PC / online), the chosen company's
// plain picks and other versions on the right. Config in companies.ts.

import { useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { useTranslation } from "react-i18next";
import type * as React from "react";
import { AudioLines, ChevronDown } from "lucide-react";
import { CloudTimeoutPanel } from "@/components/CloudTimeoutPanel";
import { DictationLanguagePanel } from "@/components/DictationLanguagePanel";
import { WhisperThreadsPanel } from "@/components/WhisperThreadsPanel";
import { CloudModelCard } from "@/components/models/CloudModelCard";
import { ImportModelCard } from "@/components/models/ImportModelCard";
import { ParakeetModelCard } from "@/components/models/ParakeetModelCard";
import { WhisperModelCard } from "@/components/models/WhisperModelCard";
import {
  companyOfSource,
  LOCAL_COMPANIES,
  pickModelId,
  RUNS_NOTE,
} from "@/components/models/companies";
import {
  isRowCurrent,
  resolveDefaultDisplayName,
  type CloudModel,
  type CloudProvider,
  type ModelRow,
  type ParakeetDownloadProgress,
} from "@/components/models/types";
import {
  api,
  type DownloadComplete,
  type DownloadError,
  type DownloadProgress,
  type ParakeetModelState,
  type TranscriptionSource,
  type WhisperModelState,
} from "@/lib/tauri";
import { cn } from "@/lib/utils";
import { confirmDelete } from "@/lib/confirmDelete";

/// Parlato: the backend reports a cancelled download as the French error
/// "telechargement annule" (model managers); it is not a real error.
function isCancel(e: unknown): boolean {
  return String(e).includes("annule");
}

export function ModelsPage({
  selectedModelId,
  onSelectModel,
  hardware,
}: {
  selectedModelId: string | null;
  onSelectModel: (id: string | null) => void;
  /** One-line hardware summary shown under the local company header. */
  hardware?: string;
}) {
  const { t } = useTranslation();
  const [whisper, setWhisper] = useState<WhisperModelState[]>([]);
  const [parakeet, setParakeet] = useState<ParakeetModelState[]>([]);
  const [providers, setProviders] = useState<CloudProvider[]>([]);
  const [cloudModels, setCloudModels] = useState<CloudModel[]>([]);
  const [source, setSource] = useState<TranscriptionSource | null>(null);
  // Parlato: company picked in the list (null = follow the model in use),
  // dictation language (for English-only picks) and "Show all versions".
  const [chosenCompany, setChosenCompany] = useState<string | null>(null);
  const [language, setLanguage] = useState("auto");
  const [showAll, setShowAll] = useState(false);
  const [whisperProgress, setWhisperProgress] = useState<
    Record<string, DownloadProgress>
  >({});
  const [parakeetProgress, setParakeetProgress] = useState<
    Record<string, ParakeetDownloadProgress>
  >({});
  const [whisperErrors, setWhisperErrors] = useState<Record<string, string>>(
    {},
  );
  const [parakeetStatus, setParakeetStatus] = useState<Record<string, string>>(
    {},
  );

  // selectedModelId dans une ref pour les callbacks des listeners montes
  // une seule fois (meme intention que l'auto-selection de l'ancien
  // ModelsPanel, sans closure perimee).
  const selectedRef = useRef(selectedModelId);
  useEffect(() => {
    selectedRef.current = selectedModelId;
  }, [selectedModelId]);

  async function refresh() {
    try {
      const [w, pk, provs, cm, src] = await Promise.all([
        api.listWhisperModels(),
        api.listParakeetModels(),
        api.listCloudProviders(),
        api.listCloudModels(),
        api.getTranscriptionSource(),
      ]);
      setWhisper(w);
      setParakeet(pk);
      setProviders(provs);
      setCloudModels(cm);
      setSource(src);
      // Auto-selection : premier whisper telecharge si rien de selectionne
      // (TranscribePanel depend de selectedModelId). Ne change pas le kind.
      if (!selectedRef.current) {
        const first = w.find((m) => m.downloaded);
        if (first) onSelectModel(first.id);
      }
    } catch (e) {
      console.error(e);
    }
  }

  useEffect(() => {
    refresh();
    const unlisteners = [
      listen<TranscriptionSource>("source:changed", (e) => {
        if (e.payload?.kind) setSource(e.payload);
      }),
      listen<DownloadProgress>("model:download:progress", (e) => {
        setWhisperProgress((p) => ({ ...p, [e.payload.id]: e.payload }));
      }),
      listen<DownloadComplete>("model:download:complete", async (e) => {
        setWhisperProgress((p) => {
          const next = { ...p };
          delete next[e.payload.id];
          return next;
        });
        setWhisperErrors((er) => {
          const next = { ...er };
          delete next[e.payload.id];
          return next;
        });
        await refresh();
        if (!selectedRef.current) onSelectModel(e.payload.id);
      }),
      listen<DownloadError>("model:download:error", (e) => {
        setWhisperProgress((p) => {
          const next = { ...p };
          delete next[e.payload.id];
          return next;
        });
        // Parlato: a cancel is not an error; just clear the progress.
        if (isCancel(e.payload.message)) return;
        setWhisperErrors((er) => ({
          ...er,
          [e.payload.id]: e.payload.message,
        }));
      }),
      listen<ParakeetDownloadProgress>(
        "parakeet_model:download:progress",
        (e) => {
          setParakeetProgress((p) => ({ ...p, [e.payload.id]: e.payload }));
        },
      ),
      listen<{ id: string; path: string }>(
        "parakeet_model:download:complete",
        (e) => {
          setParakeetProgress((p) => {
            const { [e.payload.id]: _, ...rest } = p;
            return rest;
          });
          refresh();
        },
      ),
      // Le backend emet cet event sur annulation, echec HTTP ou toute
      // erreur download_impl : on nettoie la barre + on affiche le message.
      listen<{ id: string; message: string }>(
        "parakeet_model:download:error",
        (e) => {
          setParakeetProgress((p) => {
            const { [e.payload.id]: _, ...rest } = p;
            return rest;
          });
          setParakeetStatus((s) => ({
            ...s,
            [e.payload.id]: isCancel(e.payload.message)
              ? t("parakeet.cancelled")
              : t("parakeet.errorPrefix", { message: e.payload.message }),
          }));
        },
      ),
    ];
    return () => {
      Promise.all(unlisteners).then((arr) => arr.forEach((fn) => fn()));
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // --- Whisper ---

  async function downloadWhisper(id: string) {
    // Entree de progression optimiste : bloque le multi-clic, le backend a
    // aussi son propre garde de reentrance.
    if (whisperProgress[id]) return;
    setWhisperProgress((p) => ({
      ...p,
      [id]: { id, downloaded: 0, total: 0 },
    }));
    setWhisperErrors((er) => {
      const next = { ...er };
      delete next[id];
      return next;
    });
    try {
      await api.downloadWhisperModel(id);
    } catch (e) {
      setWhisperProgress((p) => {
        const next = { ...p };
        delete next[id];
        return next;
      });
      if (!isCancel(e)) setWhisperErrors((er) => ({ ...er, [id]: String(e) }));
    }
  }

  async function deleteWhisper(id: string) {
    try {
      await api.deleteWhisperModel(id);
      if (selectedRef.current === id) onSelectModel(null);
      await refresh();
    } catch (e) {
      setWhisperErrors((er) => ({ ...er, [id]: String(e) }));
    }
  }

  async function setDefaultWhisper(id: string) {
    // selected_whisper_model (via App.handleSelectModel) puis bascule du
    // kind : set_transcription_source n'ecrit pas selected_whisper_model,
    // et set_transcription_kind preserve les selections cloud/parakeet.
    onSelectModel(id);
    try {
      await api.setTranscriptionKind("local");
    } catch (e) {
      console.error(e);
    }
  }

  // --- Parakeet ---

  async function downloadParakeet(id: string) {
    setParakeetProgress((p) => ({
      ...p,
      [id]: { id, downloaded: 0, total: 0, current_file: "" },
    }));
    setParakeetStatus((s) => ({ ...s, [id]: t("parakeet.downloading") }));
    try {
      await api.downloadParakeetModel(id);
      setParakeetStatus((s) => ({ ...s, [id]: "" }));
    } catch (e) {
      setParakeetProgress((p) => {
        const { [id]: _, ...rest } = p;
        return rest;
      });
      // Parlato: a cancel also rejects this call; show "cancelled", not the
      // raw backend message.
      setParakeetStatus((s) => ({
        ...s,
        [id]: isCancel(e) ? t("parakeet.cancelled") : t("parakeet.errorPrefix", { message: String(e) }),
      }));
    }
  }

  async function deleteParakeet(id: string) {
    if (!(await confirmDelete(t("parakeet.confirmDelete", { id })))) return;
    try {
      await api.deleteParakeetModel(id);
      await refresh();
    } catch (e) {
      setParakeetStatus((s) => ({
        ...s,
        [id]: t("parakeet.errorPrefix", { message: String(e) }),
      }));
    }
  }

  async function setDefaultParakeet(id: string) {
    try {
      await api.setTranscriptionSource({
        kind: "parakeet",
        whisper_model_id: source?.whisper_model_id,
        cloud_provider: source?.cloud_provider,
        cloud_model: source?.cloud_model,
        parakeet_model_id: id,
      });
    } catch (e) {
      console.error(e);
    }
  }

  // --- Cloud ---

  /// Verifie puis sauvegarde la cle. N'active RIEN : le bouton de la card
  /// devient "Set as Default" et l'activation reste un geste explicite.
  async function verifyAndSaveKey(providerId: string, key: string) {
    await api.verifyApiKey(providerId, key);
    await api.setApiKey(providerId, key);
    await refresh();
  }

  async function setDefaultCloud(m: CloudModel) {
    try {
      await api.setTranscriptionSource({
        kind: "cloud",
        whisper_model_id: source?.whisper_model_id,
        cloud_provider: m.provider_id,
        cloud_model: m.model_id,
        parakeet_model_id: source?.parakeet_model_id,
      });
    } catch (e) {
      console.error(e);
    }
  }

  async function removeKey(providerId: string) {
    try {
      await api.deleteApiKey(providerId);
      // Le provider actif perd sa cle -> retomber sur local (equivalent
      // VoiceInk clearCurrentTranscriptionModel dans clearAPIKey).
      if (source?.kind === "cloud" && source.cloud_provider === providerId) {
        await api.setTranscriptionKind("local");
      }
      await refresh();
    } catch (e) {
      console.error(e);
    }
  }

  // --- Companies (Parlato) ---

  const providerById = useMemo(
    () => new Map(providers.map((p) => [p.id, p])),
    [providers],
  );

  const defaultDisplayName = useMemo(
    () => resolveDefaultDisplayName(source, whisper, parakeet, cloudModels, t),
    [source, whisper, parakeet, cloudModels, t],
  );

  // Comme l'ancien panneau cloud : seuls les modeles batch sont activables
  // ici (les streaming-only ont leur propre chemin pipeline).
  const batchCloud = useMemo(() => cloudModels.filter((m) => m.supports_batch), [cloudModels]);

  // Online companies = providers that have at least one usable model, in
  // catalog order.
  const onlineCompanies = useMemo(
    () => providers.filter((p) => batchCloud.some((m) => m.provider_id === p.id)),
    [providers, batchCloud],
  );

  const activeCompany = companyOfSource(source, !!defaultDisplayName);
  const company = chosenCompany ?? activeCompany ?? "nvidia";
  const localCompany = LOCAL_COMPANIES.find((c) => c.id === company);

  /// Rows of the chosen company: picks first (local), then the other
  /// versions; online companies list all their models as "rest".
  const { pickRows, restRows } = useMemo(() => {
    const whisperRows = whisper.map<ModelRow>((m) => ({ type: "whisper", key: m.id, model: m }));
    const parakeetRows = parakeet.map<ModelRow>((m) => ({ type: "parakeet", key: m.id, model: m }));
    if (!localCompany) {
      const rest = batchCloud
        .filter((m) => m.provider_id === company)
        .map<ModelRow>((m) => ({ type: "cloud", key: `${m.provider_id}:${m.model_id}`, model: m }));
      return { pickRows: [] as { row: ModelRow; label: string }[], restRows: rest };
    }
    const all = localCompany.id === "nvidia" ? parakeetRows : whisperRows;
    const picks = localCompany.picks.flatMap((p) => {
      const row = all.find((r) => r.key === pickModelId(p, language));
      return row ? [{ row, label: t(p.label) }] : [];
    });
    const picked = new Set(picks.map((p) => p.row.key));
    return { pickRows: picks, restRows: all.filter((r) => !picked.has(r.key)) };
  }, [whisper, parakeet, batchCloud, company, localCompany, language, t]);

  // "Show all versions" opens by itself when the model in use is hidden.
  const currentHidden = !!localCompany && restRows.some((r) => isRowCurrent(r, source));
  const showRest = !localCompany || showAll || currentHidden;

  function renderRow(row: ModelRow, pick?: string) {
    const current = isRowCurrent(row, source);
    switch (row.type) {
      case "whisper":
        return (
          <WhisperModelCard
            key={row.key}
            pick={pick}
            model={row.model}
            isCurrent={current}
            progress={whisperProgress[row.model.id] ?? null}
            error={whisperErrors[row.model.id] ?? null}
            onDownload={() => downloadWhisper(row.model.id)}
            onCancelDownload={() => api.cancelDownloadWhisperModel(row.model.id)}
            onDelete={() => deleteWhisper(row.model.id)}
            onSetDefault={() => setDefaultWhisper(row.model.id)}
          />
        );
      case "parakeet":
        return (
          <ParakeetModelCard
            key={row.key}
            pick={pick}
            model={row.model}
            isCurrent={current}
            progress={parakeetProgress[row.model.id] ?? null}
            status={parakeetStatus[row.model.id] || null}
            onDownload={() => downloadParakeet(row.model.id)}
            onCancelDownload={() => api.cancelDownloadParakeetModel(row.model.id)}
            onDelete={() => deleteParakeet(row.model.id)}
            onSetDefault={() => setDefaultParakeet(row.model.id)}
          />
        );
      case "cloud": {
        const provider = providerById.get(row.model.provider_id);
        return (
          <CloudModelCard
            key={row.key}
            pick={pick}
            model={row.model}
            providerName={provider?.display_name ?? row.model.provider_id}
            apiKeyUrl={provider?.api_key_url ?? null}
            isConfigured={provider?.has_api_key ?? false}
            isCurrent={current}
            onVerifyAndSave={(key) => verifyAndSaveKey(row.model.provider_id, key)}
            onSetDefault={() => setDefaultCloud(row.model)}
            onRemoveKey={() => removeKey(row.model.provider_id)}
          />
        );
      }
    }
  }

  const onlineName = providerById.get(company)?.display_name ?? company;

  return (
    <div className="flex flex-col gap-4">
      <section className="flex flex-wrap items-center gap-4 rounded-lg border-[1.5px] border-edge bg-card px-5 py-4">
        <span className="flex h-10 w-10 flex-none items-center justify-center rounded-md bg-highlight text-[#141416]">
          <AudioLines className="h-5 w-5" />
        </span>
        <div className="flex min-w-0 flex-1 flex-col gap-0.5">
          <span className="font-mono text-[11px] text-muted-foreground">{t("speech.inUse")}</span>
          <span className="font-display text-lg leading-6 font-bold">
            {defaultDisplayName ?? t("speech.noneYet")}
          </span>
        </div>
        <DictationLanguagePanel onLanguage={setLanguage} />
      </section>

      <div className="grid grid-cols-[216px_minmax(0,1fr)] items-start gap-4">
        <nav aria-label={t("speech.companies")} className="flex flex-col gap-1.5">
          <GroupLabel>{t("speech.groupLocal")}</GroupLabel>
          {LOCAL_COMPANIES.map((c) => (
            <CompanyButton
              key={c.id}
              name={c.name}
              note={c.family}
              selected={company === c.id}
              inUse={activeCompany === c.id}
              onClick={() => setChosenCompany(c.id)}
            />
          ))}
          <GroupLabel className="mt-3">{t("speech.groupOnline")}</GroupLabel>
          {onlineCompanies.map((p) => (
            <CompanyButton
              key={p.id}
              name={p.display_name}
              note={RUNS_NOTE[p.id] ? t(RUNS_NOTE[p.id]) : p.has_api_key ? t("speech.keyAdded") : undefined}
              selected={company === p.id}
              inUse={activeCompany === p.id}
              onClick={() => setChosenCompany(p.id)}
            />
          ))}
        </nav>

        <div className="@container flex min-w-0 flex-col gap-3">
          <div className="flex flex-col gap-0.5">
            <h2 className="font-display text-lg leading-6 font-bold">
              {localCompany ? localCompany.name : onlineName}
            </h2>
            <p className="text-[13px] text-muted-foreground">
              {localCompany ? t("speech.localIntro") : t("speech.onlineIntro", { name: onlineName })}
            </p>
            {localCompany && hardware && (
              <span className="font-mono text-[11px] text-muted-foreground">{hardware}</span>
            )}
          </div>

          {pickRows.length > 0 && (
            <div className="grid gap-3 @2xl:grid-cols-2">
              {pickRows.map((p) => renderRow(p.row, p.label))}
            </div>
          )}

          {localCompany && restRows.length > 0 && !currentHidden && (
            <button
              type="button"
              onClick={() => setShowAll((v) => !v)}
              aria-expanded={showRest}
              className="flex items-center gap-1.5 self-start text-[13px] font-semibold text-muted-foreground hover:text-foreground"
            >
              <ChevronDown className={cn("h-4 w-4 transition-transform", showRest && "rotate-180")} />
              {showRest ? t("speech.hideVersions") : t("speech.showVersions", { count: restRows.length })}
            </button>
          )}

          {showRest && restRows.length > 0 && (
            <div className="grid gap-3 @2xl:grid-cols-2">{restRows.map((r) => renderRow(r))}</div>
          )}

          {showRest && localCompany?.id === "openai" && (
            <ImportModelCard
              onImported={async (id) => {
                await refresh();
                onSelectModel(id);
              }}
            />
          )}

          {!localCompany && <CloudTimeoutPanel />}
          {localCompany?.id === "openai" && <WhisperThreadsPanel />}
        </div>
      </div>
    </div>
  );
}

function GroupLabel({ children, className }: { children: React.ReactNode; className?: string }) {
  return (
    <span className={cn("px-1 font-mono text-[11px] leading-4 text-muted-foreground", className)}>{children}</span>
  );
}

function CompanyButton({
  name,
  note,
  selected,
  inUse,
  onClick,
}: {
  name: string;
  note?: string;
  selected: boolean;
  inUse: boolean;
  onClick: () => void;
}) {
  const { t } = useTranslation();
  return (
    <button
      type="button"
      onClick={onClick}
      aria-current={selected ? "true" : undefined}
      className={cn(
        "flex flex-col items-start gap-0.5 rounded-md border-[1.5px] bg-card px-3 py-2 text-left transition-colors",
        selected ? "border-edge bg-accent shadow-[var(--sel-shadow)]" : "border-input hover:border-edge",
      )}
    >
      <span className="flex w-full items-center justify-between gap-2 text-sm font-semibold">
        {name}
        {inUse && <span className="h-2 w-2 flex-none rounded-full bg-highlight" aria-label={t("speech.inUseBadge")} />}
      </span>
      {note && <span className="text-xs leading-4 text-muted-foreground">{note}</span>}
    </button>
  );
}
