// Editeur de prompts custom.
//
// Reference VoiceInk PromptEditorView + SlidingPanel 400pt.
// shadcn pur : Sheet depuis la droite.
//
// Parlato: rendered as the "Writing style" section of the AI cleanup page,
// a radio list that also sets the active prompt.

import type * as React from "react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Copy, Pencil, Plus, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Block, RadioRow, Section, selectClass } from "@/components/ui/section";
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetHeader,
  SheetTitle,
  SheetFooter,
  SheetClose,
} from "@/components/ui/sheet";
import { api, type CustomPrompt } from "@/lib/tauri";
import { promptDescription, promptTitle } from "@/lib/promptLabels";
import { cn } from "@/lib/utils";
import { confirmDelete } from "@/lib/confirmDelete";

type Props = {
  prompts: CustomPrompt[];
  activeId: string | null;
  onSelect: (id: string) => void;
  onChange: () => void | Promise<void>;
};

function emptyPrompt(defaultTitle: string): CustomPrompt {
  return {
    id: "",
    title: defaultTitle,
    prompt_text: "",
    icon: "pencil",
    description: null,
    is_predefined: false,
    trigger_words: [],
    use_system_instructions: true,
  };
}

export function PromptEditor({ prompts, activeId, onSelect, onChange }: Props) {
  const { t } = useTranslation();
  const [templates, setTemplates] = useState<CustomPrompt[]>([]);
  const [editing, setEditing] = useState<CustomPrompt | null>(null);
  const [isNew, setIsNew] = useState(false);
  const [status, setStatus] = useState("");

  useEffect(() => {
    api
      .listExtraTemplates()
      .then(setTemplates)
      .catch((e) => console.error(e));
  }, []);

  function startNew() {
    setEditing(emptyPrompt(t("promptEditor.newPromptDefaultTitle")));
    setIsNew(true);
    setStatus("");
  }

  function startFromTemplate(t: CustomPrompt) {
    setEditing({ ...t, id: "" });
    setIsNew(true);
    setStatus("");
  }

  function startEdit(p: CustomPrompt) {
    setEditing({ ...p });
    setIsNew(false);
    setStatus("");
  }

  function cancel() {
    setEditing(null);
    setIsNew(false);
    setStatus("");
  }

  async function save() {
    if (!editing) return;
    try {
      if (isNew) {
        await api.addPrompt(editing);
      } else {
        await api.updatePrompt(editing);
      }
      setEditing(null);
      setIsNew(false);
      setStatus(t("promptEditor.saved"));
      await onChange();
    } catch (e) {
      setStatus(t("promptEditor.errorPrefix", { message: String(e) }));
    }
  }

  async function remove(p: CustomPrompt) {
    if (p.is_predefined) return;
    if (!(await confirmDelete(t("promptEditor.confirmDelete", { name: p.title })))) return;
    try {
      await api.deletePrompt(p.id);
      await onChange();
    } catch (e) {
      setStatus(t("promptEditor.errorPrefix", { message: String(e) }));
    }
  }

  return (
    <Section
      title={t("ai.stylesTitle")}
      description={t("ai.stylesDesc")}
      action={
        <div className="flex items-center gap-2">
          {templates.length > 0 && (
            <select
              aria-label={t("promptEditor.fromTemplate")}
              onChange={(e) => {
                const tpl = templates.find((x) => x.title === e.target.value);
                if (tpl) startFromTemplate(tpl);
                e.target.value = "";
              }}
              defaultValue=""
              className={cn(selectClass, "min-w-0")}
            >
              <option value="" disabled>
                {t("promptEditor.fromTemplate")}
              </option>
              {templates.map((tpl) => (
                <option key={tpl.title} value={tpl.title}>
                  {tpl.title}
                </option>
              ))}
            </select>
          )}
          <Button size="sm" variant="outline" onClick={startNew}>
            <Plus className="h-3.5 w-3.5" />
            {t("ai.newStyle")}
          </Button>
        </div>
      }
    >
      <div role="radiogroup" aria-label={t("ai.stylesTitle")} className="flex flex-col divide-y">
        {prompts.map((p) => (
          <div key={p.id} className="relative">
            <RadioRow
              selected={p.id === (activeId ?? prompts[0]?.id)}
              onSelect={() => onSelect(p.id)}
              title={promptTitle(t, p)}
              description={promptDescription(t, p) || undefined}
              trailing={<span aria-hidden className="block w-[92px]" />}
            />
            {/* Actions sit on top of the row so they are not nested in its button. */}
            <div className="absolute top-1/2 right-4 flex -translate-y-1/2 gap-0.5">
              {/* Built-in styles are reset to their original text at every start,
                  so editing them would not stick: copy one to make your own. */}
              {!p.is_predefined && (
                <IconAction label={t("ai.edit")} onClick={() => startEdit(p)}>
                  <Pencil className="h-3.5 w-3.5" />
                </IconAction>
              )}
              <IconAction
                label={t("ai.duplicate")}
                onClick={() =>
                  startFromTemplate({
                    ...p,
                    title: `${p.title} ${t("promptEditor.duplicateSuffix")}`,
                  })
                }
              >
                <Copy className="h-3.5 w-3.5" />
              </IconAction>
              {!p.is_predefined && (
                <IconAction label={t("ai.delete")} onClick={() => remove(p)}>
                  <Trash2 className="h-3.5 w-3.5" />
                </IconAction>
              )}
            </div>
          </div>
        ))}
      </div>
      {status && !editing && (
        <Block className="py-3 text-xs text-muted-foreground">{status}</Block>
      )}

      <Sheet
        open={editing !== null}
        onOpenChange={(open) => {
          if (!open) cancel();
        }}
      >
        <SheetContent side="right" className="w-[400px] sm:max-w-[400px]">
          <SheetHeader>
            <SheetTitle>
              {isNew
                ? t("promptEditor.newPromptTitle")
                : t("promptEditor.editPromptTitle")}
            </SheetTitle>
            <SheetDescription>
              {editing?.is_predefined
                ? t("promptEditor.predefinedDescription")
                : t("promptEditor.customDescription")}
            </SheetDescription>
          </SheetHeader>

          {editing && (
            <div className="mt-4 grid gap-3 overflow-y-auto pb-20 pr-1">
              <div className="grid gap-1">
                <label className="text-xs font-medium">{t("promptEditor.title_field")}</label>
                <input
                  type="text"
                  value={editing.title}
                  onChange={(e) =>
                    setEditing({ ...editing, title: e.target.value })
                  }
                  className="h-9 rounded-md border-[1.5px] border-input bg-background px-3 text-sm"
                />
              </div>
              <div className="grid gap-1">
                <label className="text-xs font-medium">{t("promptEditor.description_field")}</label>
                <input
                  type="text"
                  value={editing.description ?? ""}
                  onChange={(e) =>
                    setEditing({
                      ...editing,
                      description: e.target.value || null,
                    })
                  }
                  className="h-9 rounded-md border-[1.5px] border-input bg-background px-3 text-sm"
                />
              </div>
              <label className="flex items-start gap-2 rounded-md border p-2 text-xs">
                <input
                  type="checkbox"
                  role="switch"
                  className="mt-0.5"
                  checked={editing.use_system_instructions}
                  onChange={(e) =>
                    setEditing({
                      ...editing,
                      use_system_instructions: e.target.checked,
                    })
                  }
                />
                <span>
                  <span className="font-medium">
                    {t("promptEditor.injectWrapper")}
                  </span>
                  <br />
                  <span className="text-muted-foreground">
                    {t("promptEditor.injectWrapperHelp")}
                  </span>
                </span>
              </label>
              <div className="grid gap-1">
                <label className="text-xs font-medium">{t("promptEditor.content")}</label>
                <textarea
                  value={editing.prompt_text}
                  onChange={(e) =>
                    setEditing({ ...editing, prompt_text: e.target.value })
                  }
                  className="min-h-[220px] rounded-md border-[1.5px] border-input bg-background px-3 py-2 font-mono text-xs"
                />
              </div>
              <div className="grid gap-1">
                <label className="text-xs font-medium">
                  {t("promptEditor.triggerWords")}
                </label>
                <input
                  type="text"
                  placeholder={t("promptEditor.triggerWordsPlaceholder")}
                  value={editing.trigger_words.join(", ")}
                  onChange={(e) =>
                    setEditing({
                      ...editing,
                      trigger_words: e.target.value
                        .split(",")
                        .map((s) => s.trim())
                        .filter(Boolean),
                    })
                  }
                  className="h-9 rounded-md border-[1.5px] border-input bg-background px-3 text-sm"
                />
                <p className="text-[10px] text-muted-foreground">
                  {t("promptEditor.triggerWordsHelp")}
                </p>
              </div>
              {status && (
                <p className="text-xs text-muted-foreground">{status}</p>
              )}
            </div>
          )}

          <SheetFooter className="absolute bottom-0 left-0 right-0 border-t bg-background p-4">
            <SheetClose asChild>
              <Button variant="ghost">{t("common.cancel")}</Button>
            </SheetClose>
            <Button onClick={save}>{t("common.save")}</Button>
          </SheetFooter>
        </SheetContent>
      </Sheet>
    </Section>
  );
}

function IconAction({
  label,
  onClick,
  children,
}: {
  label: string;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      title={label}
      aria-label={label}
      className="flex h-[30px] w-[30px] items-center justify-center rounded-sm text-muted-foreground hover:bg-card hover:text-foreground"
    >
      {children}
    </button>
  );
}
