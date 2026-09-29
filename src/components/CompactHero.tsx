// Bandeau page : icon + title + description.
//
// Reference VoiceInk Views/Components/CompactHeroSection.swift : icon
// 28pt hierarchique primary + title 22pt bold + description 14pt
// secondary, padding vertical 20.

import type { LucideIcon } from "lucide-react";

export function CompactHero({
  icon: Icon,
  title,
  description,
}: {
  icon: LucideIcon;
  title: string;
  description: string;
}) {
  return (
    <div className="flex flex-col items-center gap-2 px-6 py-8 text-center">
      <div className="flex h-12 w-12 items-center justify-center rounded-full bg-accent text-accent-foreground">
        <Icon className="h-6 w-6" />
      </div>
      <h2 className="text-xl font-bold leading-tight">{title}</h2>
      <p className="max-w-prose text-sm text-muted-foreground">{description}</p>
    </div>
  );
}
