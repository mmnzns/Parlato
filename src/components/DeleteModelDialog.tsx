// Parlato: the one Delete question for every downloaded model (speech cards,
// local AI rows, voice detection).
//
// - Another app (Talaan, Ensayo) uses the model: two choices, "Delete from
//   Parlato" (lets go for this app only) and "Delete everywhere and free {size}".
// - Nobody else uses it: one confirm that frees the space.
// - An import that lives outside the shared folder: one confirm, Parlato only.
//
// `useDeleteModel().ask()` resolves to the choice; the caller runs the matching
// command (`delete_*` for "parlato", `remove_model_everywhere` for "everywhere").

import { useCallback, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import { appList } from "@/lib/removeModel";
import { formatBytes } from "@/lib/format";

export type DeleteChoice = "parlato" | "everywhere" | "cancel";

export type DeleteTarget = {
  name: string;
  /** What the model takes on disk (frees this much). */
  bytes: number | null | undefined;
  /** Talaan / Ensayo when they also use it. */
  otherApps?: string[];
  /** Not in the shared folder: Delete is for Parlato only. */
  imported?: boolean;
};

export function useDeleteModel() {
  const [target, setTarget] = useState<DeleteTarget | null>(null);
  const resolver = useRef<((c: DeleteChoice) => void) | null>(null);

  const ask = useCallback((model: DeleteTarget) => {
    // A second question while one is open answers the first with Cancel.
    resolver.current?.("cancel");
    setTarget(model);
    return new Promise<DeleteChoice>((resolve) => {
      resolver.current = resolve;
    });
  }, []);

  const answer = useCallback((c: DeleteChoice) => {
    resolver.current?.(c);
    resolver.current = null;
    setTarget(null);
  }, []);

  const dialog = <DeleteModelDialog target={target} onAnswer={answer} />;
  return { ask, dialog };
}

function DeleteModelDialog({ target, onAnswer }: { target: DeleteTarget | null; onAnswer: (c: DeleteChoice) => void }) {
  const { t } = useTranslation();
  // Keep the last target while the dialog fades out.
  const last = useRef<DeleteTarget | null>(null);
  if (target) last.current = target;
  const m = target ?? last.current;
  const apps = m?.otherApps ?? [];
  const shared = !!m && !m.imported && apps.length > 0;
  const size = formatBytes(m?.bytes);

  return (
    <Dialog open={!!target} onOpenChange={(open) => !open && onAnswer("cancel")}>
      {m && (
        <DialogContent>
          <div className="flex flex-col gap-1.5">
            <DialogTitle>{t("deleteModel.title", { name: m.name })}</DialogTitle>
            <DialogDescription>
              {shared
                ? t("deleteModel.sharedBody", { apps: appList(apps) })
                : m.imported
                  ? t("deleteModel.importedBody")
                  : t("deleteModel.aloneBody", { size })}
            </DialogDescription>
          </div>
          <div className="flex flex-wrap justify-end gap-2">
            <Button variant="ghost" size="sm" onClick={() => onAnswer("cancel")}>
              {t("common.cancel")}
            </Button>
            {shared ? (
              <>
                <Button variant="outline" size="sm" onClick={() => onAnswer("parlato")}>
                  {t("deleteModel.fromParlato")}
                </Button>
                <Button variant="destructive" size="sm" onClick={() => onAnswer("everywhere")}>
                  {t("deleteModel.everywhere", { size })}
                </Button>
              </>
            ) : (
              <Button variant="destructive" size="sm" onClick={() => onAnswer(m.imported ? "parlato" : "everywhere")}>
                {t("common.delete")}
              </Button>
            )}
          </div>
        </DialogContent>
      )}
    </Dialog>
  );
}
