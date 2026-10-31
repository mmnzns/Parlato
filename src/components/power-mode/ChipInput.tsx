import { Plus } from "lucide-react";

/** "+ Add ..." chip that turns into a small text input. */
export function ChipInput({
  draft,
  setDraft,
  onOpen,
  onCommit,
  label,
  placeholder,
}: {
  draft: string | null;
  setDraft: (v: string | null) => void;
  /** Parlato: what the button does instead of opening the text box. */
  onOpen?: () => void;
  onCommit: () => void;
  label: string;
  placeholder: string;
}) {
  if (draft === null) {
    return (
      <button
        type="button"
        onClick={() => (onOpen ? onOpen() : setDraft(""))}
        className="flex h-8 items-center gap-1.5 rounded-full border-[1.5px] border-dashed border-input px-3 text-xs font-semibold text-muted-foreground hover:border-edge hover:text-foreground"
      >
        <Plus className="h-3.5 w-3.5" />
        {label}
      </button>
    );
  }
  return (
    <input
      autoFocus
      value={draft}
      onChange={(e) => setDraft(e.target.value)}
      onKeyDown={(e) => {
        if (e.key === "Enter") onCommit();
        if (e.key === "Escape") setDraft(null);
      }}
      onBlur={onCommit}
      placeholder={placeholder}
      aria-label={label}
      className="h-8 w-48 rounded-full border-[1.5px] border-foreground bg-background px-3 font-mono text-xs outline-none"
    />
  );
}
