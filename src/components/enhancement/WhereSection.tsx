// Parlato: [01] where the AI runs (this PC vs an online service).

import { useTranslation } from "react-i18next";
import { Cloud, Laptop } from "lucide-react";
import { SectionHeading } from "@/components/ui/section";
import { cn } from "@/lib/utils";

export function WhereSection({
  where,
  chooseWhere,
}: {
  where: "local" | "cloud" | null;
  chooseWhere: (next: "local" | "cloud") => Promise<void>;
}) {
  const { t } = useTranslation();
  const whereCards = [
    { id: "local" as const, Icon: Laptop, title: t("ai.localTitle"), desc: t("ai.localDesc"), eg: t("ai.localEg"), rec: true },
    { id: "cloud" as const, Icon: Cloud, title: t("ai.cloudTitle"), desc: t("ai.cloudDesc"), eg: t("ai.cloudEg"), rec: false },
  ];

  return (
    <section className="flex flex-col gap-2.5">
      <SectionHeading title={t("ai.whereTitle")} />
      <div role="radiogroup" aria-label={t("ai.whereTitle")} className="grid gap-3 sm:grid-cols-2">
        {whereCards.map((w) => {
          const sel = where === w.id;
          return (
            <button
              key={w.id}
              type="button"
              role="radio"
              aria-checked={sel}
              onClick={() => chooseWhere(w.id)}
              className={cn(
                "flex items-start gap-3.5 rounded-lg border-[1.5px] bg-card p-4 text-left transition-colors",
                sel ? "border-edge bg-accent shadow-[var(--sel-shadow)]" : "border-input hover:border-edge",
              )}
            >
              <span
                className={cn(
                  "mt-0.5 flex h-[18px] w-[18px] flex-none items-center justify-center rounded-full border-[1.5px] bg-card",
                  sel ? "border-foreground" : "border-input",
                )}
              >
                <span className={cn("h-2 w-2 rounded-full bg-foreground", !sel && "opacity-0")} />
              </span>
              <span className="flex min-w-0 flex-col gap-1">
                <span className="flex flex-wrap items-center gap-2 font-display font-bold">
                  <w.Icon className="h-4 w-4" />
                  {w.title}
                  {w.rec && (
                    <span className="rounded-full bg-highlight px-2 py-px font-sans text-[11px] font-semibold text-[#141416]">
                      {t("ai.recommended")}
                    </span>
                  )}
                </span>
                <span className="text-[13px] leading-[18px] text-pretty text-muted-foreground">{w.desc}</span>
                <span className="font-mono text-[11px] text-muted-foreground">{w.eg}</span>
              </span>
            </button>
          );
        })}
      </div>
    </section>
  );
}
