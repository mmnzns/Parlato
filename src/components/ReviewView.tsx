// "Review before pasting" editor, rendered in its own small window
// ("recorder-review", see src-tauri/src/review.rs).
//
// The finished dictation waits here instead of being pasted. Enter pastes it
// into the app the person was typing in, Shift+Enter adds a line, Esc (or
// Cancel) keeps it in History only. The window can take the keyboard only
// while it is open (macOS panel flag, ordinary window on Windows).

import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { api } from "@/lib/tauri";

export function ReviewView() {
  const { t } = useTranslation();
  const [text, setText] = useState<string | null>(null);
  const area = useRef<HTMLTextAreaElement>(null);
  // Set once a choice is made, so a second Enter or click does nothing.
  const done = useRef(false);

  // The backend holds the text; this window asks for it when it loads.
  useEffect(() => {
    api
      .reviewText()
      .then((value) => setText(value ?? ""))
      .catch(console.error);
  }, []);

  // Focus the field with the cursor at the end once the text has loaded, and
  // again whenever the window gets the keyboard (the panel may take it a
  // moment late). Only the first load moves the cursor.
  const loaded = text !== null;
  const placed = useRef(false);
  useEffect(() => {
    const focus = () => {
      const el = area.current;
      if (!el) return;
      el.focus();
      if (!placed.current) {
        placed.current = true;
        el.setSelectionRange(el.value.length, el.value.length);
      }
    };
    if (loaded) focus();
    window.addEventListener("focus", focus);
    return () => window.removeEventListener("focus", focus);
  }, [loaded]);

  function paste() {
    if (done.current) return;
    done.current = true;
    api.reviewPaste(text ?? "").catch(console.error);
  }

  function cancel() {
    if (done.current) return;
    done.current = true;
    api.reviewCancel().catch(console.error);
  }

  return (
    <div className="flex h-screen w-screen items-end justify-center bg-transparent">
      <div className="flex h-full w-full flex-col gap-2 rounded-[14px] bg-[#141416] p-3 text-white ring-[1.5px] ring-inset ring-[#B5E61D]">
        <textarea
          ref={area}
          autoFocus
          value={text ?? ""}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => {
            // Not while an input method is composing (Enter confirms the word).
            if (e.nativeEvent.isComposing) return;
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              paste();
            } else if (e.key === "Escape") {
              e.preventDefault();
              cancel();
            }
          }}
          aria-label={t("review.textLabel")}
          spellCheck
          className="min-h-0 flex-1 resize-none rounded-[8px] border border-white/15 bg-black/30 p-2 text-[13px] leading-5 text-white outline-none focus:border-[#B5E61D]"
        />
        <div className="flex items-center gap-2">
          <span className="min-w-0 flex-1 truncate text-[11px] text-white/60" title={t("review.hint")}>
            {t("review.hint")}
          </span>
          <button
            type="button"
            onClick={cancel}
            className="h-7 shrink-0 rounded-full px-3 text-[12px] font-medium text-white/80 hover:bg-white/10 hover:text-white"
          >
            {t("review.cancel")}
          </button>
          <button
            type="button"
            onClick={paste}
            className="h-7 shrink-0 rounded-full bg-[#B5E61D] px-3 text-[12px] font-semibold text-black hover:brightness-95"
          >
            {t("review.paste")}
          </button>
        </div>
      </div>
    </div>
  );
}
