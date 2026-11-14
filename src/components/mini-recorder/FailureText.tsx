// -- FailureText (Parlato) ----------------------------------------------------
//
// Why the dictation was not pasted, translated, in a 54 px band above the
// control bar. The recording stays in History for Transcribe again.
// With `onFix` the band ends in a "Fix it" link that opens the main window on
// the page that solves it (two lines of text then, so the band keeps its size).

export function FailureText({ text, fixLabel, onFix }: { text: string; fixLabel?: string; onFix?: () => void }) {
  return (
    <div role="alert" className="flex h-[54px] flex-col overflow-hidden px-4 py-1.5 text-left text-[12px] leading-snug text-white/85">
      <span className={onFix ? "line-clamp-2 min-h-0" : "line-clamp-3 min-h-0"}>{text}</span>
      {onFix && (
        <button
          type="button"
          onClick={onFix}
          className="mt-auto self-start text-[11px] font-semibold text-[#FF8A5C] underline underline-offset-2 outline-none hover:text-white focus-visible:ring-1 focus-visible:ring-white/60"
        >
          {fixLabel}
        </button>
      )}
    </div>
  );
}
