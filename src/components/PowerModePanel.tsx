// Parlato: Power modes page, rebuilt to the Workbench design (docs/design/v1,
// screen "profiles"): the list of modes on the left, the selected mode's
// settings on the right, edited in place.
//
// Every change is saved straight away (text fields on blur), so there is no
// Save button. A new mode is created as soon as "New power mode" is
// pressed. The PowerModeConfig shape and the backend commands are
// unchanged from upstream; "Keep my usual" maps to the null values that
// mean "leave the global setting alone".

import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Copy, Globe, Info, Plus, Trash2, X, Zap } from "lucide-react";
import { Button } from "@/components/ui/button";
import { AppPicker } from "@/components/AppPicker";
import { Block, Row, Section, Switch, selectClass } from "@/components/ui/section";
import { cn, powerShortcutLabel } from "@/lib/utils";
import { promptTitle } from "@/lib/promptLabels";
import { modelName } from "@/lib/modelText";
import type { CloudModel } from "@/components/models/types";
import {
  api,
  type CustomPrompt,
  type InstalledApp,
  type LLMProviderInfo,
  type ParakeetModelState,
  type PowerModeConfig,
  type WhisperModelState,
} from "@/lib/tauri";

const inputClass = "h-[34px] min-w-0 rounded-sm border-[1.5px] border-input bg-background px-3 text-sm";

// Alt+1 .. Alt+9, then Alt+0 (the hook's digit order).
const SHORTCUT_SLOTS = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9];

function emptyConfig(name: string): PowerModeConfig {
  return {
    id: "",
    name,
    emoji: "*",
    app_triggers: [],
    url_triggers: [],
    is_enhancement_enabled: false,
    use_screen_capture: null,
    selected_prompt_id: null,
    selected_llm_provider: null,
    selected_llm_model: null,
    transcription_kind: null,
    whisper_model_id: null,
    cloud_provider: null,
    cloud_model: null,
    parakeet_model_id: null,
    language: null,
    auto_send_key: "none",
    shortcut_slot: null,
    is_enabled: true,
    is_default: false,
  };
}

type Catalog = {
  prompts: CustomPrompt[];
  providers: LLMProviderInfo[];
  whisper: WhisperModelState[];
  parakeet: ParakeetModelState[];
  cloud: CloudModel[];
};

export function PowerModePanel() {
  const { t } = useTranslation();
  const [configs, setConfigs] = useState<PowerModeConfig[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [autoRestore, setAutoRestore] = useState(true);
  const [catalog, setCatalog] = useState<Catalog>({ prompts: [], providers: [], whisper: [], parakeet: [], cloud: [] });
  const [confirming, setConfirming] = useState(false);

  useEffect(() => {
    refresh(true);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  async function refresh(pickFirst = false) {
    try {
      const [cs, ar, prompts, providers, whisper, parakeet, cloud] = await Promise.all([
        api.listPowerConfigs(),
        api.getPowerAutoRestore(),
        api.listPrompts(),
        api.listLlmProviders(),
        api.listWhisperModels(),
        api.listParakeetModels(),
        api.listCloudModels(),
      ]);
      setConfigs(cs);
      setAutoRestore(ar);
      setCatalog({ prompts, providers, whisper, parakeet, cloud: cloud as CloudModel[] });
      if (pickFirst && cs.length > 0) setSelectedId(cs[0].id);
    } catch (e) {
      console.error(e);
    }
  }

  async function createMode() {
    try {
      const created = await api.addPowerConfig(emptyConfig(t("pm.newName")));
      await refresh();
      setSelectedId(created.id);
    } catch (e) {
      console.error(e);
    }
  }

  // VoiceInk 2.21 duplicateConfiguration: same settings, no app or website
  // triggers (so the two modes never compete), never the default. Added at
  // the end so no other mode's Alt+number shortcut moves.
  async function duplicateSelected() {
    const source = configs.find((c) => c.id === selectedId);
    if (!source) return;
    try {
      const created = await api.addPowerConfig({
        ...source,
        id: "",
        name: duplicateName(source.name, configs.map((c) => c.name)),
        app_triggers: [],
        url_triggers: [],
        shortcut_slot: null,
        is_default: false,
      });
      await refresh();
      setSelectedId(created.id);
    } catch (e) {
      console.error(e);
    }
  }

  async function save(next: PowerModeConfig) {
    const prev = configs.find((c) => c.id === next.id);
    // Optimistic: the editor stays responsive, the list reflects the change.
    setConfigs((cs) => cs.map((c) => (c.id === next.id ? next : c)));
    try {
      await api.updatePowerConfig(next);
      // is_default and shortcut_slot are exclusive: the backend may have
      // cleared them on another mode.
      if (next.is_default || next.shortcut_slot !== prev?.shortcut_slot) await refresh();
    } catch (e) {
      console.error(e);
      await refresh();
    }
  }

  async function removeSelected() {
    if (!selectedId) return;
    try {
      await api.deletePowerConfig(selectedId);
      setConfirming(false);
      const rest = configs.filter((c) => c.id !== selectedId);
      setSelectedId(rest[0]?.id ?? null);
      await refresh();
    } catch (e) {
      console.error(e);
    }
  }

  async function toggleAutoRestore(v: boolean) {
    setAutoRestore(v);
    await api.setPowerAutoRestore(v);
  }

  const shortcutFor = (c: PowerModeConfig) => (c.is_enabled ? powerShortcutLabel(c.shortcut_slot) : null);
  const selected = configs.find((c) => c.id === selectedId) ?? null;

  return (
    <>
      <div className="grid items-start gap-5 md:grid-cols-[220px_minmax(0,1fr)]">
        <nav className="flex flex-col gap-1.5" aria-label={t("pm.yourModes")}>
          <span className="px-1 font-mono text-[11px] text-muted-foreground">{t("pm.yourModes")}</span>
          {configs.map((c) => {
            const active = c.id === selectedId;
            const paired = c.app_triggers.length + c.url_triggers.length;
            const summary = !c.is_enabled
              ? t("pm.summaryOff")
              : c.is_default
                ? t("pm.summaryEverywhere")
                : t("pm.summaryApps", { count: paired });
            const key = shortcutFor(c);
            return (
              <button
                key={c.id}
                type="button"
                onClick={() => {
                  setSelectedId(c.id);
                  setConfirming(false);
                }}
                aria-current={active ? "true" : undefined}
                className={cn(
                  "flex items-center gap-2.5 rounded-md border-[1.5px] px-3 py-2.5 text-left transition-colors",
                  active ? "border-edge bg-accent shadow-[var(--sel-shadow)]" : "border-transparent hover:bg-muted/70",
                )}
              >
                <span
                  className={cn(
                    "h-2 w-2 flex-none rounded-full",
                    c.is_enabled ? "bg-highlight" : "bg-border",
                  )}
                />
                <span className="flex min-w-0 flex-1 flex-col">
                  <span className="truncate text-sm font-semibold">{c.name}</span>
                  <span className="truncate text-xs text-muted-foreground">{summary}</span>
                </span>
                {key && (
                  <kbd className="rounded-sm border px-1.5 py-px font-mono text-[10px] text-muted-foreground">{key}</kbd>
                )}
              </button>
            );
          })}
          {configs.length === 0 && (
            <div className="flex flex-col gap-1 rounded-md border-[1.5px] border-dashed border-input p-3 text-xs text-muted-foreground">
              <span className="font-semibold text-foreground">{t("pm.emptyTitle")}</span>
              {t("pm.emptyDesc")}
            </div>
          )}
          <Button size="sm" variant="outline" className="mt-1 self-start" onClick={createMode}>
            <Plus className="h-3.5 w-3.5" />
            {t("pm.newMode")}
          </Button>
        </nav>

        <div className="flex min-w-0 flex-col gap-[22px]">
          {selected ? (
            <ModeEditor
              key={selected.id}
              config={selected}
              others={configs.filter((c) => c.id !== selected.id)}
              catalog={catalog}
              onChange={save}
              confirming={confirming}
              onAskDelete={() => setConfirming(true)}
              onCancelDelete={() => setConfirming(false)}
              onDelete={removeSelected}
              onDuplicate={duplicateSelected}
            />
          ) : (
            <div className="rounded-lg border-[1.5px] border-dashed border-input bg-card p-8 text-center text-sm text-muted-foreground">
              {t("pm.pickOne")}
            </div>
          )}
        </div>
      </div>

      {/* A single row, not a numbered section: it applies to all modes. */}
      <div className="rounded-lg border-[1.5px] border-edge bg-card">
        <Row label={t("pm.autoRestore")} description={t("pm.autoRestoreDesc")} htmlFor="pm-auto-restore">
          <Switch id="pm-auto-restore" checked={autoRestore} onChange={toggleAutoRestore} />
        </Row>
      </div>

      <p className="flex items-center gap-1.5 text-xs text-muted-foreground">
        <Info className="h-3.5 w-3.5" />
        {t("pm.shortcutWins")}
      </p>
    </>
  );
}

function ModeEditor({
  config: c,
  others,
  catalog,
  onChange,
  confirming,
  onAskDelete,
  onCancelDelete,
  onDelete,
  onDuplicate,
}: {
  config: PowerModeConfig;
  others: PowerModeConfig[];
  catalog: Catalog;
  onChange: (c: PowerModeConfig) => void;
  confirming: boolean;
  onAskDelete: () => void;
  onCancelDelete: () => void;
  onDelete: () => void;
  onDuplicate: () => void;
}) {
  const { t } = useTranslation();
  const [name, setName] = useState(c.name);
  const [language, setLanguage] = useState(c.language ?? "");
  const [appDraft, setAppDraft] = useState<string | null>(null);
  const [picking, setPicking] = useState(false);
  const [siteDraft, setSiteDraft] = useState<string | null>(null);

  function set<K extends keyof PowerModeConfig>(key: K, value: PowerModeConfig[K]) {
    onChange({ ...c, [key]: value });
  }

  function addApp() {
    const exe = (appDraft ?? "").trim().toLowerCase();
    setAppDraft(null);
    if (!exe || c.app_triggers.some((a) => a.exe_name === exe)) return;
    onChange({ ...c, app_triggers: [...c.app_triggers, { id: crypto.randomUUID(), exe_name: exe, app_name: exe }] });
  }

  function addPickedApp(app: InstalledApp) {
    setPicking(false);
    if (c.app_triggers.some((a) => a.exe_name === app.exe_name)) return;
    onChange({
      ...c,
      app_triggers: [...c.app_triggers, { id: crypto.randomUUID(), exe_name: app.exe_name, app_name: app.name }],
    });
  }

  function addSite() {
    const url = (siteDraft ?? "").trim();
    setSiteDraft(null);
    if (!url || c.url_triggers.some((u) => u.url === url)) return;
    onChange({ ...c, url_triggers: [...c.url_triggers, { id: crypto.randomUUID(), url }] });
  }

  // Speech model: one select combining kind + model id ("" = keep usual).
  const speechValue =
    c.transcription_kind === "local" && c.whisper_model_id
      ? `local:${c.whisper_model_id}`
      : c.transcription_kind === "parakeet" && c.parakeet_model_id
        ? `parakeet:${c.parakeet_model_id}`
        : c.transcription_kind === "cloud" && c.cloud_provider && c.cloud_model
          ? `cloud:${c.cloud_provider}:${c.cloud_model}`
          : "";

  function setSpeech(v: string) {
    const base = {
      ...c,
      transcription_kind: null,
      whisper_model_id: null,
      parakeet_model_id: null,
      cloud_provider: null,
      cloud_model: null,
    } as PowerModeConfig;
    if (v.startsWith("local:")) onChange({ ...base, transcription_kind: "local", whisper_model_id: v.slice(6) });
    else if (v.startsWith("parakeet:")) onChange({ ...base, transcription_kind: "parakeet", parakeet_model_id: v.slice(9) });
    else if (v.startsWith("cloud:")) {
      const [, provider, ...model] = v.split(":");
      onChange({ ...base, transcription_kind: "cloud", cloud_provider: provider, cloud_model: model.join(":") });
    } else onChange(base);
  }

  const provider = catalog.providers.find((p) => p.id === c.selected_llm_provider);
  const whisper = catalog.whisper.filter((m) => m.downloaded);
  const parakeet = catalog.parakeet.filter((m) => m.downloaded);
  const cloud = catalog.cloud.filter((m) => m.supports_batch);
  const triggers = [
    ...c.app_triggers.map((a) => ({ id: a.id, label: a.app_name || a.exe_name, site: false })),
    ...c.url_triggers.map((u) => ({ id: u.id, label: u.url, site: true })),
  ];

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

      <Section title={t("pm.startsTitle")} description={t("pm.startsDesc")}>
        <Row label={t("pm.shortcut")} description={t("pm.shortcutDesc")}>
          <select
            aria-label={t("pm.shortcut")}
            value={c.shortcut_slot ?? ""}
            onChange={(e) => set("shortcut_slot", e.target.value === "" ? null : Number(e.target.value))}
            className={cn(selectClass, "w-[180px]")}
          >
            <option value="">{t("pm.shortcutNone")}</option>
            {SHORTCUT_SLOTS.map((slot) => {
              const key = powerShortcutLabel(slot);
              const owner = others.find((o) => o.shortcut_slot === slot);
              return (
                <option key={slot} value={slot}>
                  {owner ? t("pm.shortcutTaken", { key, name: owner.name }) : key}
                </option>
              );
            })}
          </select>
        </Row>
        <Block className="flex flex-col gap-2.5">
          <div className="flex flex-col gap-0.5">
            <span className="font-medium">{t("pm.paired")}</span>
            <span className="text-[13px] leading-[18px] text-muted-foreground">{t("pm.pairedDesc")}</span>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            {triggers.map((tr) => (
              <span
                key={tr.id}
                className={cn(
                  "flex h-8 items-center gap-1.5 rounded-full border-[1.5px] border-edge bg-card pr-1 pl-3 text-xs",
                  tr.site && "font-mono",
                )}
              >
                {tr.site && <Globe className="h-3 w-3 text-muted-foreground" />}
                {tr.label}
                <button
                  type="button"
                  aria-label={t("pm.remove", { name: tr.label })}
                  onClick={() =>
                    tr.site
                      ? set("url_triggers", c.url_triggers.filter((u) => u.id !== tr.id))
                      : set("app_triggers", c.app_triggers.filter((a) => a.id !== tr.id))
                  }
                  className="flex h-6 w-6 items-center justify-center rounded-full text-muted-foreground hover:bg-muted hover:text-foreground"
                >
                  <X className="h-3.5 w-3.5" />
                </button>
              </span>
            ))}
            <ChipInput
              draft={appDraft}
              setDraft={setAppDraft}
              onOpen={() => setPicking(true)}
              onCommit={addApp}
              label={t("pm.addApp")}
              placeholder={t("pm.appPlaceholder")}
            />
            <ChipInput
              draft={siteDraft}
              setDraft={setSiteDraft}
              onCommit={addSite}
              label={t("pm.addSite")}
              placeholder={t("pm.sitePlaceholder")}
            />
          </div>
          {picking && (
            <AppPicker
              paired={c.app_triggers.map((a) => a.exe_name)}
              onPick={addPickedApp}
              onTypeName={() => {
                setPicking(false);
                setAppDraft("");
              }}
              onClose={() => setPicking(false)}
            />
          )}
        </Block>
        <Row label={t("pm.fallback")} description={t("pm.fallbackDesc")} htmlFor="pm-fallback">
          <Switch id="pm-fallback" checked={c.is_default} onChange={(v) => set("is_default", v)} />
        </Row>
      </Section>

      <Section title={t("pm.doesTitle")} description={t("pm.doesDesc")}>
        <Row label={t("pm.speechModel")} description={t("pm.speechModelDesc")}>
          <select
            aria-label={t("pm.speechModel")}
            value={speechValue}
            onChange={(e) => setSpeech(e.target.value)}
            className={cn(selectClass, "max-w-[240px]")}
          >
            <option value="">{t("pm.keep")}</option>
            {whisper.length > 0 && (
              <optgroup label={t("pm.localWhisper")}>
                {whisper.map((m) => (
                  <option key={m.id} value={`local:${m.id}`}>
                    {modelName(t, m.id, m.display_name)}
                  </option>
                ))}
              </optgroup>
            )}
            {parakeet.length > 0 && (
              <optgroup label={t("pm.localParakeet")}>
                {parakeet.map((m) => (
                  <option key={m.id} value={`parakeet:${m.id}`}>
                    {modelName(t, m.id, m.display_name)}
                  </option>
                ))}
              </optgroup>
            )}
            <optgroup label={t("pm.online")}>
              {cloud.map((m) => (
                <option key={`${m.provider_id}:${m.model_id}`} value={`cloud:${m.provider_id}:${m.model_id}`}>
                  {m.display_name}
                </option>
              ))}
            </optgroup>
          </select>
        </Row>
        <Row label={t("pm.language")} description={t("pm.languageDesc")}>
          <input
            value={language}
            onChange={(e) => setLanguage(e.target.value)}
            onBlur={() => {
              const v = language.trim() || null;
              if (v !== c.language) set("language", v);
            }}
            placeholder="auto"
            aria-label={t("pm.language")}
            className={cn(inputClass, "w-24 font-mono")}
          />
        </Row>
        <Row label={t("pm.aiCleanup")} description={t("pm.aiCleanupDesc")} htmlFor="pm-ai">
          <Switch id="pm-ai" checked={c.is_enhancement_enabled} onChange={(v) => set("is_enhancement_enabled", v)} />
        </Row>
        {c.is_enhancement_enabled && (
          <>
            <Row label={t("pm.aiService")} description={t("pm.aiServiceDesc")}>
              <select
                aria-label={t("pm.aiService")}
                value={c.selected_llm_provider ?? ""}
                onChange={(e) => {
                  const id = e.target.value || null;
                  const p = catalog.providers.find((x) => x.id === id);
                  onChange({ ...c, selected_llm_provider: id, selected_llm_model: id ? (p?.default_model ?? null) : null });
                }}
                className={cn(selectClass, "max-w-[240px]")}
              >
                <option value="">{t("pm.keep")}</option>
                {catalog.providers.map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.label}
                  </option>
                ))}
              </select>
            </Row>
            {provider && (
              <Row label={t("pm.aiModel")}>
                {provider.models.length > 0 ? (
                  <select
                    aria-label={t("pm.aiModel")}
                    value={c.selected_llm_model ?? ""}
                    onChange={(e) => set("selected_llm_model", e.target.value || null)}
                    className={cn(selectClass, "max-w-[240px]")}
                  >
                    {c.selected_llm_model && !provider.models.includes(c.selected_llm_model) && (
                      <option value={c.selected_llm_model}>{c.selected_llm_model}</option>
                    )}
                    {provider.models.map((m) => (
                      <option key={m} value={m}>
                        {m}
                      </option>
                    ))}
                  </select>
                ) : (
                  <FreeTextModel value={c.selected_llm_model ?? ""} onCommit={(v) => set("selected_llm_model", v || null)} />
                )}
              </Row>
            )}
            <Row label={t("pm.style")} description={t("pm.styleDesc")}>
              <select
                aria-label={t("pm.style")}
                value={c.selected_prompt_id ?? ""}
                onChange={(e) => set("selected_prompt_id", e.target.value || null)}
                className={cn(selectClass, "max-w-[240px]")}
              >
                <option value="">{t("pm.keep")}</option>
                {catalog.prompts.map((p) => (
                  <option key={p.id} value={p.id}>
                    {promptTitle(t, p)}
                  </option>
                ))}
              </select>
            </Row>
            <Row label={t("pm.screen")}>
              <select
                aria-label={t("pm.screen")}
                value={c.use_screen_capture === null ? "" : c.use_screen_capture ? "on" : "off"}
                onChange={(e) =>
                  set("use_screen_capture", e.target.value === "" ? null : e.target.value === "on")
                }
                className={selectClass}
              >
                <option value="">{t("pm.keep")}</option>
                <option value="on">{t("pm.screenOn")}</option>
                <option value="off">{t("pm.screenOff")}</option>
              </select>
            </Row>
          </>
        )}
      </Section>
    </>
  );
}

/** "+ Add ..." chip that turns into a small text input. */
function ChipInput({
  draft,
  setDraft,
  onOpen,
  onCommit,
  label,
  placeholder,
}: {
  draft: string | null;
  setDraft: (v: string | null) => void;
  /** Parlato: what the button does instead of opening the text box. */
  onOpen?: () => void;
  onCommit: () => void;
  label: string;
  placeholder: string;
}) {
  if (draft === null) {
    return (
      <button
        type="button"
        onClick={() => (onOpen ? onOpen() : setDraft(""))}
        className="flex h-8 items-center gap-1.5 rounded-full border-[1.5px] border-dashed border-input px-3 text-xs font-semibold text-muted-foreground hover:border-edge hover:text-foreground"
      >
        <Plus className="h-3.5 w-3.5" />
        {label}
      </button>
    );
  }
  return (
    <input
      autoFocus
      value={draft}
      onChange={(e) => setDraft(e.target.value)}
      onKeyDown={(e) => {
        if (e.key === "Enter") onCommit();
        if (e.key === "Escape") setDraft(null);
      }}
      onBlur={onCommit}
      placeholder={placeholder}
      aria-label={label}
      className="h-8 w-48 rounded-full border-[1.5px] border-foreground bg-background px-3 font-mono text-xs outline-none"
    />
  );
}

function FreeTextModel({ value, onCommit }: { value: string; onCommit: (v: string) => void }) {
  const [v, setV] = useState(value);
  return (
    <input
      value={v}
      onChange={(e) => setV(e.target.value)}
      onBlur={() => v.trim() !== value && onCommit(v.trim())}
      className={cn(inputClass, "w-[240px] font-mono")}
    />
  );
}

// VoiceInk 2.21 nextDuplicateName: "Email" -> "Email 1", "Email 1" -> "Email 2".
function duplicateName(name: string, taken: string[]): string {
  const names = new Set(taken);
  let base = name;
  const m = name.match(/^(.*) (\d+)$/);
  if (m && Number(m[2]) > 0 && names.has(m[1])) base = m[1];
  let n = 1;
  while (names.has(`${base} ${n}`)) n += 1;
  return `${base} ${n}`;
}
