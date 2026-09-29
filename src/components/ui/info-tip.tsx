// InfoTip : petit bouton info.circle qui ouvre un popover avec du texte
// + un lien optionnel "En savoir plus".
//
// Reference VoiceInk Views/Components/InfoTip.swift : bouton info.circle
// secondary + popover "title / description / learn more URL". Pattern
// pervasif a cote des toggles importants.

import { Info } from "lucide-react";
import { useTranslation } from "react-i18next";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";

type Props = {
  /** Texte court, 2-3 phrases max. */
  children: React.ReactNode;
  /** URL optionnelle pour "Learn more". */
  learnMoreUrl?: string;
  /** Button label, defaults to the translated common.learnMore. */
  learnMoreLabel?: string;
  /** Alignement du popover, "start" par defaut. */
  align?: "start" | "center" | "end";
};

export function InfoTip({
  children,
  learnMoreUrl,
  learnMoreLabel,
  align = "start",
}: Props) {
  const { t } = useTranslation();
  const label = learnMoreLabel ?? t("common.learnMore");
  return (
    <Popover>
      <PopoverTrigger asChild>
        <button
          type="button"
          aria-label={t("common.info")}
          className="inline-flex h-4 w-4 items-center justify-center rounded-full text-muted-foreground transition-colors hover:text-foreground"
        >
          <Info className="h-3.5 w-3.5" />
        </button>
      </PopoverTrigger>
      <PopoverContent align={align} className="w-72 text-xs">
        <div className="space-y-2">
          <div className="text-muted-foreground">{children}</div>
          {learnMoreUrl && (
            <button
              type="button"
              onClick={() => openUrl(learnMoreUrl)}
              className="text-[11px] font-medium text-foreground underline decoration-primary underline-offset-2"
            >
              {label} {"->"}
            </button>
          )}
        </div>
      </PopoverContent>
    </Popover>
  );
}
