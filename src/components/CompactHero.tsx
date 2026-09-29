// Bandeau page : breadcrumb + title + description.
//
// Reference VoiceInk Views/Components/CompactHeroSection.swift : icon
// 28pt hierarchique primary + title 22pt bold + description 14pt
// secondary, padding vertical 20.
//
// Parlato: Workbench page header (docs/design/v1). Left-aligned, no icon:
// mono breadcrumb, 28px display title, muted description.

export function CompactHero({
  crumb,
  title,
  description,
}: {
  crumb?: string;
  title: string;
  description: string;
}) {
  return (
    <header className="flex min-w-0 flex-col gap-1.5">
      {crumb && (
        <span className="font-mono text-[11px] leading-[14px] text-muted-foreground">{crumb}</span>
      )}
      <h1 className="font-display text-[28px] leading-8 font-extrabold tracking-[-0.035em]">
        {title}
      </h1>
      <p className="max-w-[560px] text-sm text-pretty text-muted-foreground">{description}</p>
    </header>
  );
}
