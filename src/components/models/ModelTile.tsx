// Parlato: shared Workbench card for the Speech model grid
// (docs/design/v1, screen "model"). The Whisper, Parakeet and cloud cards
// keep their own logic and render through this shell: kind + where on top,
// name + technical id, speed / accuracy bars, a meta line, then the action.

import type * as React from "react";
import { useTranslation } from "react-i18next";
import { Check, Cloud, Laptop } from "lucide-react";
import { RatingDots } from "@/components/RatingDots";
import { cn } from "@/lib/utils";

export function ModelTile({
  current,
  kind,
  local,
  name,
  tech,
  description,
  speed,
  accuracy,
  meta,
  action,
  trailing,
  children,
}: {
  current: boolean;
  /** Short family label, e.g. "Whisper", "Parakeet", a provider name. */
  kind: string;
  local: boolean;
  name: string;
  tech?: string;
  description?: string;
  speed?: number;
  accuracy?: number;
  meta?: React.ReactNode;
  /** Main action (download / use / add key), hidden when the model is in use. */
  action?: React.ReactNode;
  /** Shown at the right of the action row (e.g. delete). */
  trailing?: React.ReactNode;
  /** Extra content under the action: progress, errors, key form. */
  children?: React.ReactNode;
}) {
  const { t } = useTranslation();
  return (
    <div
      className={cn(
        "flex flex-col gap-3 rounded-lg border-[1.5px] bg-card p-4",
        current ? "border-edge bg-accent shadow-[var(--sel-shadow)]" : "border-input",
      )}
    >
      <div className="flex items-center justify-between gap-2 text-[11px]">
        <span className="rounded-full bg-muted px-2 py-px font-semibold">{kind}</span>
        <span className="flex items-center gap-1 text-muted-foreground">
          {local ? <Laptop className="h-3 w-3" /> : <Cloud className="h-3 w-3" />}
          {local ? t("speech.onPc") : t("speech.online")}
        </span>
      </div>

      <div className="flex min-w-0 flex-col gap-0.5">
        <span className="font-display text-[15px] leading-5 font-bold">{name}</span>
        {tech && <span className="truncate font-mono text-[11px] text-muted-foreground">{tech}</span>}
      </div>

      {description && (
        <p className="text-[13px] leading-[18px] text-pretty text-muted-foreground">{description}</p>
      )}

      {!!speed && speed > 0 && (
        <div className="grid grid-cols-[auto_1fr] items-center gap-x-3 gap-y-1 text-xs text-muted-foreground">
          <span>{t("speech.speed")}</span>
          <RatingDots value={speed} />
          <span>{t("speech.accuracy")}</span>
          <RatingDots value={accuracy ?? 0} />
        </div>
      )}

      {meta && <span className="font-mono text-[11px] leading-4 text-muted-foreground">{meta}</span>}

      <div className="mt-auto flex flex-col gap-2">
        <div className="flex items-center gap-2">
          <div className="flex min-w-0 flex-1 flex-col">
            {current ? (
              <span className="flex h-[34px] items-center gap-1.5 text-[13px] font-semibold text-positive">
                <Check className="h-4 w-4" />
                {t("speech.inUseBadge")}
              </span>
            ) : (
              action
            )}
          </div>
          {trailing}
        </div>
        {children}
      </div>
    </div>
  );
}

/** Thin download progress bar used by the model tiles. */
export function TileProgress({ pct, label }: { pct: number | null; label?: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-1">
      <div
        role="progressbar"
        aria-valuenow={pct ?? undefined}
        aria-valuemin={0}
        aria-valuemax={100}
        className="h-1.5 w-full overflow-hidden rounded-full bg-muted"
      >
        <div className="h-full bg-primary transition-[width]" style={{ width: `${pct ?? 0}%` }} />
      </div>
      {label && <span className="font-mono text-[11px] text-muted-foreground">{label}</span>}
    </div>
  );
}
