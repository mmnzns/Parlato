// -- FailureText (Parlato) ----------------------------------------------------
//
// Why the dictation was not pasted, translated, in a 54 px band above the
// control bar. The recording stays in History for Transcribe again.

export function FailureText({ text }: { text: string }) {
  return (
    <div
      role="alert"
      className="line-clamp-3 h-[54px] overflow-hidden px-4 py-1.5 text-left text-[12px] leading-snug text-white/85"
    >
      {text}
    </div>
  );
}
