// Cinq cercles de rating pour vitesse / precision, alignes sur VoiceInk
// progressDotsWithNumber dans Views/AI Models/WhisperModelCardView.swift.
//
// Parlato: Workbench style (docs/design/v1): five small monochrome bars,
// filled in the foreground color, no colored scale and no number by default.

import { cn } from "@/lib/utils";

type Props = {
  /** Note entre 0 et 1 (ex 0.95 = 95%). */
  value: number;
  className?: string;
  /** Affiche aussi le nombre formate ("9.5"). False par defaut. */
  showLabel?: boolean;
};

export function RatingDots({ value, className, showLabel = false }: Props) {
  // VoiceInk : `Int(value * 10 / 2)` = floor(value * 5).
  const filled = Math.floor(value * 5);
  const display = (value * 10).toFixed(1);

  return (
    <span className={cn("inline-flex items-center gap-1.5", className)} aria-label={`${display} / 10`}>
      <span className="inline-flex items-center gap-[3px]">
        {Array.from({ length: 5 }, (_, i) => (
          <span
            key={i}
            aria-hidden="true"
            className={cn("h-2 w-3.5 rounded-[2px]", i < filled ? "bg-foreground" : "bg-border")}
          />
        ))}
      </span>
      {showLabel && <span className="font-mono text-[10px] text-muted-foreground">{display}</span>}
    </span>
  );
}
