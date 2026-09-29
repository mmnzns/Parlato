import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { BookOpen, Plus, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { cn } from "@/lib/utils";
import { api, type WordReplacement } from "@/lib/tauri";

export function DictionaryPanel() {
  const { t } = useTranslation();
  const [entries, setEntries] = useState<WordReplacement[]>([]);
  const [original, setOriginal] = useState("");
  const [replacement, setReplacement] = useState("");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    refresh();
  }, []);

  async function refresh() {
    try {
      const list = await api.listWordReplacements();
      setEntries(list);
    } catch (e) {
      setError(String(e));
    }
  }

  async function add() {
    if (!original.trim() || !replacement.trim()) {
      setError(t("dictionary.bothRequired"));
      return;
    }
    try {
      await api.addWordReplacement({
        original_text: original.trim(),
        replacement_text: replacement.trim(),
      });
      setOriginal("");
      setReplacement("");
      setError(null);
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  }

  async function toggle(entry: WordReplacement) {
    try {
      await api.updateWordReplacement({
        id: entry.id,
        is_enabled: !entry.is_enabled,
      });
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  }

  async function remove(id: string) {
    try {
      await api.deleteWordReplacement(id);
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  }

  async function editInline(
    entry: WordReplacement,
    field: "original_text" | "replacement_text",
    value: string,
  ) {
    if (value === entry[field]) return;
    try {
      await api.updateWordReplacement({ id: entry.id, [field]: value });
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  }

  return (
    <Card>
      <CardHeader>
        <div className="flex items-center gap-2">
          <BookOpen className="h-4 w-4 text-muted-foreground" />
          <CardTitle className="text-base">{t("dictionary.title")}</CardTitle>
        </div>
        <CardDescription>{t("dictionary.description")}</CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        <div className="grid gap-2 sm:grid-cols-[1fr_1fr_auto]">
          <input
            value={original}
            onChange={(e) => setOriginal(e.target.value)}
            placeholder={t("dictionary.originalPlaceholder")}
            className="flex h-9 rounded-md border-[1.5px] border-input bg-background px-3 text-sm"
          />
          <input
            value={replacement}
            onChange={(e) => setReplacement(e.target.value)}
            placeholder={t("dictionary.replacementPlaceholder")}
            className="flex h-9 rounded-md border-[1.5px] border-input bg-background px-3 text-sm"
          />
          <Button onClick={add}>
            <Plus /> {t("dictionary.add")}
          </Button>
        </div>

        {entries.length === 0 ? (
          <p className="rounded-md border border-dashed p-4 text-center text-sm text-muted-foreground">
            {t("dictionary.empty")}
          </p>
        ) : (
          <div className="divide-y rounded-md border">
            {entries.map((e) => (
              <div
                key={e.id}
                className={cn(
                  "grid grid-cols-[auto_1fr_1fr_auto] items-center gap-2 p-2",
                  !e.is_enabled && "opacity-60",
                )}
              >
                <input
                  type="checkbox"
                  role="switch"
                  checked={e.is_enabled}
                  onChange={() => toggle(e)}
                  className="h-4 w-4 cursor-pointer"
                  title={e.is_enabled ? t("dictionary.disable") : t("dictionary.enable")}
                />
                <input
                  defaultValue={e.original_text}
                  onBlur={(ev) => editInline(e, "original_text", ev.target.value)}
                  className="flex h-8 rounded-md border-[1.5px] border-input bg-background px-2 text-xs font-mono"
                />
                <input
                  defaultValue={e.replacement_text}
                  onBlur={(ev) => editInline(e, "replacement_text", ev.target.value)}
                  className="flex h-8 rounded-md border-[1.5px] border-input bg-background px-2 text-xs"
                />
                <Button size="sm" variant="ghost" onClick={() => remove(e.id)}>
                  <Trash2 className="h-3.5 w-3.5" />
                </Button>
              </div>
            ))}
          </div>
        )}

        {error && (
          <div className="rounded-md bg-destructive/10 p-3 text-sm text-destructive">
            {t("dictionary.errorPrefix", { message: error })}
          </div>
        )}
      </CardContent>
    </Card>
  );
}
