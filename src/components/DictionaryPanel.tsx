// Parlato: Dictionary page, rebuilt to the Workbench design (docs/design/v1,
// screen "dictionary").
//
// Both sections live in the same word_replacements table:
//   - a "word Parlato should know" is an entry whose spoken and written
//     sides are identical (e.g. Parlato -> Parlato). The replacer leaves
//     the text as is, and the word still reaches the AI cleanup and the
//     online engines as custom vocabulary (enhancement/service.rs).
//   - a replacement is any other entry ("my address" -> the address).

import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { ArrowRight, Check, Pencil, Plus, Trash2, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Block, Section, Switch } from "@/components/ui/section";
import { cn } from "@/lib/utils";
import { api, type WordReplacement } from "@/lib/tauri";

const inputClass = "h-[34px] min-w-0 rounded-sm border-[1.5px] border-input bg-background px-3 text-sm";

function isWord(e: WordReplacement): boolean {
  return e.original_text.trim() === e.replacement_text.trim();
}

export function DictionaryPanel() {
  const { t } = useTranslation();
  const [entries, setEntries] = useState<WordReplacement[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    refresh();
  }, []);

  async function refresh() {
    try {
      setEntries(await api.listWordReplacements());
    } catch (e) {
      setError(String(e));
    }
  }

  async function run(fn: () => Promise<unknown>) {
    try {
      await fn();
      setError(null);
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  }

  const words = entries.filter(isWord);
  const replacements = entries.filter((e) => !isWord(e));

  return (
    <>
      <WordsSection
        words={words}
        onAdd={(w) => run(() => api.addWordReplacement({ original_text: w, replacement_text: w }))}
        onRemove={(id) => run(() => api.deleteWordReplacement(id))}
      />
      <ReplacementsSection
        items={replacements}
        onAdd={(say, write) => run(() => api.addWordReplacement({ original_text: say, replacement_text: write }))}
        onSave={(id, say, write) =>
          run(() => api.updateWordReplacement({ id, original_text: say, replacement_text: write }))
        }
        onToggle={(e) => run(() => api.updateWordReplacement({ id: e.id, is_enabled: !e.is_enabled }))}
        onRemove={(id) => run(() => api.deleteWordReplacement(id))}
      />
      {error && (
        <p role="alert" className="text-sm text-destructive">
          {t("dictionary.errorPrefix", { message: error })}
        </p>
      )}
    </>
  );
}

function WordsSection({
  words,
  onAdd,
  onRemove,
}: {
  words: WordReplacement[];
  onAdd: (word: string) => void;
  onRemove: (id: string) => void;
}) {
  const { t } = useTranslation();
  const [adding, setAdding] = useState(false);
  const [draft, setDraft] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (adding) inputRef.current?.focus();
  }, [adding]);

  function commit() {
    const w = draft.trim();
    if (w && !words.some((x) => x.original_text.toLowerCase() === w.toLowerCase())) onAdd(w);
    setDraft("");
  }

  return (
    <Section title={t("dict.wordsTitle")} description={t("dict.wordsDesc")}>
      <Block className="flex flex-wrap items-center gap-2">
        {words.map((w) => (
          <span
            key={w.id}
            className="flex h-8 items-center gap-1 rounded-full border-[1.5px] border-edge bg-card pr-1 pl-3 text-sm font-medium"
          >
            {w.original_text}
            <button
              type="button"
              onClick={() => onRemove(w.id)}
              aria-label={t("dict.removeWord", { word: w.original_text })}
              className="flex h-6 w-6 items-center justify-center rounded-full text-muted-foreground hover:bg-muted hover:text-foreground"
            >
              <X className="h-3.5 w-3.5" />
            </button>
          </span>
        ))}
        {adding ? (
          <input
            ref={inputRef}
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") commit();
              if (e.key === "Escape") {
                setDraft("");
                setAdding(false);
              }
            }}
            onBlur={() => {
              commit();
              setAdding(false);
            }}
            placeholder={t("dict.wordPlaceholder")}
            aria-label={t("dict.addWord")}
            className="h-8 w-56 rounded-full border-[1.5px] border-foreground bg-background px-3 text-sm outline-none"
          />
        ) : (
          <button
            type="button"
            onClick={() => setAdding(true)}
            className="flex h-8 items-center gap-1.5 rounded-full border-[1.5px] border-dashed border-input px-3 text-sm font-semibold text-muted-foreground hover:border-edge hover:text-foreground"
          >
            <Plus className="h-3.5 w-3.5" />
            {t("dict.addWord")}
          </button>
        )}
      </Block>
    </Section>
  );
}

function ReplacementsSection({
  items,
  onAdd,
  onSave,
  onToggle,
  onRemove,
}: {
  items: WordReplacement[];
  onAdd: (say: string, write: string) => void;
  onSave: (id: string, say: string, write: string) => void;
  onToggle: (e: WordReplacement) => void;
  onRemove: (id: string) => void;
}) {
  const { t } = useTranslation();
  const [editing, setEditing] = useState<string | "new" | null>(null);

  return (
    <Section title={t("dict.replTitle")} description={t("dict.replDesc")}>
      {(items.length > 0 || editing === "new") && (
        <div className="grid grid-cols-[minmax(0,1fr)_16px_minmax(0,1.4fr)_auto] gap-3 px-5 py-2 font-mono text-[11px] text-muted-foreground">
          <span>{t("dict.whenYouSay")}</span>
          <span />
          <span>{t("dict.parlatoWrites")}</span>
          <span className="w-[120px]" />
        </div>
      )}
      {items.length === 0 && editing !== "new" && (
        <Block className="text-sm text-muted-foreground">{t("dict.noReplacements")}</Block>
      )}
      {items.map((e) =>
        editing === e.id ? (
          <ReplacementEditor
            key={e.id}
            initialSay={e.original_text}
            initialWrite={e.replacement_text}
            onCancel={() => setEditing(null)}
            onSubmit={(say, write) => {
              onSave(e.id, say, write);
              setEditing(null);
            }}
          />
        ) : (
          <div
            key={e.id}
            className={cn(
              "grid grid-cols-[minmax(0,1fr)_16px_minmax(0,1.4fr)_auto] items-center gap-3 px-5 py-3",
              !e.is_enabled && "opacity-55",
            )}
          >
            <span className="truncate font-medium" title={e.original_text}>
              &ldquo;{e.original_text}&rdquo;
            </span>
            <ArrowRight className="h-4 w-4 text-muted-foreground" />
            <span className="truncate text-sm" title={e.replacement_text}>
              {e.replacement_text}
            </span>
            <div className="flex w-[120px] items-center justify-end gap-1">
              <Switch checked={e.is_enabled} onChange={() => onToggle(e)} label={t("dict.on")} />
              <Button
                size="icon"
                variant="ghost"
                className="text-muted-foreground"
                onClick={() => setEditing(e.id)}
                title={t("dict.edit")}
                aria-label={t("dict.edit")}
              >
                <Pencil className="h-3.5 w-3.5" />
              </Button>
              <Button
                size="icon"
                variant="ghost"
                className="text-muted-foreground"
                onClick={() => onRemove(e.id)}
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
        <ReplacementEditor
          initialSay=""
          initialWrite=""
          onCancel={() => setEditing(null)}
          onSubmit={(say, write) => {
            onAdd(say, write);
            setEditing(null);
          }}
        />
      ) : (
        <Block className="py-3">
          <Button size="sm" variant="outline" onClick={() => setEditing("new")}>
            <Plus className="h-3.5 w-3.5" />
            {t("dict.addReplacement")}
          </Button>
        </Block>
      )}
    </Section>
  );
}

function ReplacementEditor({
  initialSay,
  initialWrite,
  onSubmit,
  onCancel,
}: {
  initialSay: string;
  initialWrite: string;
  onSubmit: (say: string, write: string) => void;
  onCancel: () => void;
}) {
  const { t } = useTranslation();
  const [say, setSay] = useState(initialSay);
  const [write, setWrite] = useState(initialWrite);
  const [missing, setMissing] = useState(false);

  function submit() {
    if (!say.trim() || !write.trim()) {
      setMissing(true);
      return;
    }
    onSubmit(say.trim(), write.trim());
  }

  return (
    <div className="flex flex-col gap-2 bg-accent/60 px-5 py-3">
      <div className="grid grid-cols-[minmax(0,1fr)_16px_minmax(0,1.4fr)_auto] items-center gap-3">
        <input
          autoFocus
          value={say}
          onChange={(e) => setSay(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && submit()}
          placeholder={t("dict.sayPlaceholder")}
          aria-label={t("dict.whenYouSay")}
          className={inputClass}
        />
        <ArrowRight className="h-4 w-4 text-muted-foreground" />
        <input
          value={write}
          onChange={(e) => setWrite(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && submit()}
          placeholder={t("dict.writePlaceholder")}
          aria-label={t("dict.parlatoWrites")}
          className={inputClass}
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
        {missing ? t("dict.bothRequired") : t("dict.variantsHint")}
      </span>
    </div>
  );
}
