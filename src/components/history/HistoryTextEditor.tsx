// Parlato: in-place editor for a dictation's text, and the "Add to
// Dictionary?" prompt shown after a correction. Saving keeps the original
// (the backend stores the correction next to it) and returns the words that
// changed; nothing reaches the Dictionary without a click on Add.

import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Check, Plus, X } from "lucide-react";
import { api } from "@/lib/tauri";
import { translateError } from "@/lib/translateError";

export function TextEditor({
  initial,
  onSave,
  onCancel,
}: {
  initial: string;
  onSave: (text: string) => Promise<void>;
  onCancel: () => void;
}) {
  const { t } = useTranslation();
  const [text, setText] = useState(initial);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const ref = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    el.focus();
    el.setSelectionRange(el.value.length, el.value.length);
  }, []);

  const empty = text.trim() === "";
  const unchanged = text.trim() === initial.trim();

  async function save() {
    if (empty || busy) return;
    if (unchanged) {
      onCancel();
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await onSave(text);
    } catch (e) {
      setError(translateError(t, String(e)));
      setBusy(false);
    }
  }

  return (
    <div className="flex flex-col gap-2">
      <textarea
        ref={ref}
        value={text}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Escape") {
            e.stopPropagation();
            onCancel();
          } else if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
            e.preventDefault();
            void save();
          }
        }}
        aria-label={t("hist.editLabel")}
        rows={Math.min(10, Math.max(3, Math.ceil(text.length / 80)))}
        className="w-full max-w-[68ch] resize-y rounded-sm border-[1.5px] border-foreground bg-background p-3 text-[15px] leading-6 outline-none"
      />
      <div className="flex flex-wrap items-center gap-2">
        <button
          type="button"
          onClick={save}
          disabled={empty || busy}
          className="flex h-7 items-center gap-1.5 rounded-sm border-[1.5px] border-edge bg-primary px-2.5 text-xs font-semibold text-primary-foreground disabled:opacity-50"
        >
          <Check className="h-3.5 w-3.5" />
          {t("hist.editSave")}
        </button>
        <button
          type="button"
          onClick={onCancel}
          className="flex h-7 items-center rounded-sm px-2.5 text-xs font-semibold text-muted-foreground hover:bg-accent hover:text-foreground"
        >
          {t("hist.editCancel")}
        </button>
        <span className="text-xs text-muted-foreground">{t("hist.editHint")}</span>
      </div>
      {error && (
        <p role="alert" className="text-xs text-destructive">
          {error}
        </p>
      )}
    </div>
  );
}

/** "Add to Dictionary?" with one chip per changed word: Add or dismiss each. */
export function DictionarySuggestions({
  words,
  onChange,
}: {
  words: string[];
  onChange: (remaining: string[]) => void;
}) {
  const { t } = useTranslation();
  const [added, setAdded] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function add(word: string) {
    setError(null);
    try {
      await api.addWordReplacement({ original_text: word, replacement_text: word });
      setAdded(word);
      onChange(words.filter((w) => w !== word));
    } catch (e) {
      setError(translateError(t, String(e)));
    }
  }

  if (words.length === 0) {
    return added ? (
      <p role="status" className="text-xs text-positive">
        {t("hist.dictAdded", { word: added })}
      </p>
    ) : null;
  }

  return (
    <div
      role="group"
      aria-label={t("hist.dictAsk")}
      className="flex flex-wrap items-center gap-x-3 gap-y-2 rounded-sm border-[1.5px] border-dashed border-input bg-muted px-3 py-2"
    >
      <span className="text-xs font-semibold">{t("hist.dictAsk")}</span>
      {words.map((w) => (
        <span
          key={w}
          className="flex h-7 items-center gap-0.5 rounded-full border-[1.5px] border-edge bg-card pr-0.5 pl-3 text-sm font-medium"
        >
          {w}
          <button
            type="button"
            onClick={() => add(w)}
            aria-label={t("hist.dictAdd", { word: w })}
            className="ml-1 flex h-6 items-center gap-1 rounded-full bg-primary px-2 text-xs font-semibold text-primary-foreground hover:opacity-90"
          >
            <Plus className="h-3 w-3" />
            {t("hist.dictAddShort")}
          </button>
          <button
            type="button"
            onClick={() => onChange(words.filter((x) => x !== w))}
            aria-label={t("hist.dictDismiss", { word: w })}
            title={t("hist.dictDismissShort")}
            className="flex h-6 w-6 items-center justify-center rounded-full text-muted-foreground hover:bg-accent hover:text-foreground"
          >
            <X className="h-3.5 w-3.5" />
          </button>
        </span>
      ))}
      {error && (
        <span role="alert" className="text-xs text-destructive">
          {error}
        </span>
      )}
    </div>
  );
}
