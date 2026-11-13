// Parlato: the Snippets section of the Dictionary page. A snippet is a
// trigger phrase and a saved text (several lines allowed): say the phrase,
// get the text. Rules (text_processing/snippets.rs): the phrase on its own
// inserts only the saved text; inside a sentence it replaces just those words;
// AI cleanup never rewrites the inserted text.

import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { ArrowRight, Check, Download, Pencil, Plus, Trash2, Upload, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Block, Section, Switch } from "@/components/ui/section";
import { cn } from "@/lib/utils";
import { api, type Snippet } from "@/lib/tauri";
import { translateError } from "@/lib/translateError";

const inputClass = "h-[34px] min-w-0 rounded-sm border-[1.5px] border-input bg-background px-3 text-sm";
const GRID = "grid grid-cols-[minmax(0,1fr)_16px_minmax(0,1.4fr)_auto] gap-3";

export function SnippetsSection() {
  const { t } = useTranslation();
  const [items, setItems] = useState<Snippet[]>([]);
  const [editing, setEditing] = useState<string | "new" | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    refresh();
  }, []);

  async function refresh() {
    try {
      setItems(await api.listSnippets());
    } catch (e) {
      setError(translateError(t, String(e)));
    }
  }

  // Returns true when the change went through (the editor stays open on an error).
  async function run(fn: () => Promise<unknown>): Promise<boolean> {
    try {
      await fn();
      setError(null);
      await refresh();
      return true;
    } catch (e) {
      setError(translateError(t, String(e)));
      return false;
    }
  }

  async function importCsv() {
    setBusy(true);
    setStatus(null);
    try {
      const res = await api.importSnippetsCsv();
      setError(null);
      if (res) setStatus(t("dict.importDone", { added: res.added, skipped: res.skipped }));
      await refresh();
    } catch (e) {
      setError(translateError(t, String(e)));
    } finally {
      setBusy(false);
    }
  }

  async function exportCsv() {
    setBusy(true);
    setStatus(null);
    try {
      const path = await api.exportSnippetsCsv();
      setError(null);
      if (path) setStatus(t("dict.exportDone", { path }));
    } catch (e) {
      setError(translateError(t, String(e)));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Section
      title={t("dict.snippetsTitle")}
      description={t("dict.snippetsDesc")}
      action={
        <div className="flex gap-2">
          <Button size="sm" variant="outline" onClick={importCsv} disabled={busy} title={t("dict.snippetsImportHint")}>
            <Upload className="h-3.5 w-3.5" />
            {t("dict.importCsv")}
          </Button>
          <Button size="sm" variant="outline" onClick={exportCsv} disabled={busy || items.length === 0}>
            <Download className="h-3.5 w-3.5" />
            {t("dict.exportCsv")}
          </Button>
        </div>
      }
    >
      {(items.length > 0 || editing === "new") && (
        <div className={cn(GRID, "px-5 py-2 font-mono text-[11px] text-muted-foreground")}>
          <span>{t("dict.whenYouSay")}</span>
          <span />
          <span>{t("dict.snippetText")}</span>
          <span className="w-[120px]" />
        </div>
      )}
      {items.length === 0 && editing !== "new" && (
        <Block className="text-sm text-muted-foreground">{t("dict.noSnippets")}</Block>
      )}
      {items.map((s) =>
        editing === s.id ? (
          <SnippetEditor
            key={s.id}
            initialTrigger={s.trigger_text}
            initialContent={s.content}
            onCancel={() => setEditing(null)}
            onSubmit={async (trigger, content) => {
              const ok = await run(() => api.updateSnippet({ id: s.id, trigger_text: trigger, content }));
              if (ok) setEditing(null);
            }}
          />
        ) : (
          <div key={s.id} className={cn(GRID, "items-center px-5 py-3", !s.is_enabled && "opacity-55")}>
            <span className="truncate font-medium" title={s.trigger_text}>
              &ldquo;{s.trigger_text}&rdquo;
            </span>
            <ArrowRight className="h-4 w-4 text-muted-foreground" />
            {/* Line breaks are kept in the preview, two lines at most. */}
            <span className="line-clamp-2 text-sm whitespace-pre-line" title={s.content}>
              {s.content}
            </span>
            <div className="flex w-[120px] items-center justify-end gap-1">
              <Switch
                checked={s.is_enabled}
                onChange={() => run(() => api.updateSnippet({ id: s.id, is_enabled: !s.is_enabled }))}
                label={t("dict.on")}
              />
              <Button
                size="icon"
                variant="ghost"
                className="text-muted-foreground"
                onClick={() => setEditing(s.id)}
                title={t("dict.edit")}
                aria-label={t("dict.edit")}
              >
                <Pencil className="h-3.5 w-3.5" />
              </Button>
              <Button
                size="icon"
                variant="ghost"
                className="text-muted-foreground"
                onClick={() => run(() => api.deleteSnippet(s.id))}
                title={t("dict.delete")}
                aria-label={t("dict.delete")}
              >
                <Trash2 className="h-3.5 w-3.5" />
              </Button>
            </div>
          </div>
        ),
      )}
      {editing === "new" ? (
        <SnippetEditor
          initialTrigger=""
          initialContent=""
          onCancel={() => setEditing(null)}
          onSubmit={async (trigger, content) => {
            const ok = await run(() => api.addSnippet({ trigger_text: trigger, content }));
            if (ok) setEditing(null);
          }}
        />
      ) : (
        <Block className="flex flex-col gap-2 py-3">
          <div>
            <Button size="sm" variant="outline" onClick={() => setEditing("new")}>
              <Plus className="h-3.5 w-3.5" />
              {t("dict.addSnippet")}
            </Button>
          </div>
          <p className="text-xs text-muted-foreground">{t("dict.snippetsHint")}</p>
        </Block>
      )}
      {status && (
        <div role="status" className="px-5 py-3 text-sm text-muted-foreground">
          {status}
        </div>
      )}
      {error && (
        <div role="alert" className="px-5 py-3 text-sm text-destructive">
          {t("dictionary.errorPrefix", { message: error })}
        </div>
      )}
    </Section>
  );
}

function SnippetEditor({
  initialTrigger,
  initialContent,
  onSubmit,
  onCancel,
}: {
  initialTrigger: string;
  initialContent: string;
  onSubmit: (trigger: string, content: string) => void;
  onCancel: () => void;
}) {
  const { t } = useTranslation();
  const [trigger, setTrigger] = useState(initialTrigger);
  const [content, setContent] = useState(initialContent);
  const [missing, setMissing] = useState(false);
  const triggerRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    triggerRef.current?.focus();
  }, []);

  function submit() {
    if (!trigger.trim() || !content.trim()) {
      setMissing(true);
      return;
    }
    onSubmit(trigger.trim(), content);
  }

  return (
    <div className="flex flex-col gap-2 bg-accent/60 px-5 py-3">
      <div className={cn(GRID, "items-start")}>
        <input
          ref={triggerRef}
          value={trigger}
          onChange={(e) => setTrigger(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && submit()}
          placeholder={t("dict.snippetTriggerPlaceholder")}
          aria-label={t("dict.whenYouSay")}
          className={inputClass}
        />
        <ArrowRight className="mt-2 h-4 w-4 text-muted-foreground" />
        {/* A multi-line field: Enter is a new line here, Save is the button. */}
        <textarea
          value={content}
          onChange={(e) => setContent(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Escape") onCancel();
          }}
          rows={4}
          placeholder={t("dict.snippetTextPlaceholder")}
          aria-label={t("dict.snippetText")}
          className="min-w-0 resize-y rounded-sm border-[1.5px] border-input bg-background px-3 py-2 text-sm"
        />
        <div className="flex w-[120px] items-center justify-end gap-1">
          <Button size="icon" onClick={submit} title={t("dict.save")} aria-label={t("dict.save")}>
            <Check className="h-4 w-4" />
          </Button>
          <Button size="icon" variant="ghost" onClick={onCancel} title={t("dict.cancel")} aria-label={t("dict.cancel")}>
            <X className="h-4 w-4" />
          </Button>
        </div>
      </div>
      <span className={cn("text-xs", missing ? "text-destructive" : "text-muted-foreground")}>
        {missing ? t("dict.snippetRequired") : t("dict.snippetEditHint")}
      </span>
    </div>
  );
}
