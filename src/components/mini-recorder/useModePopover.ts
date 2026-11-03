import { useCallback, useEffect, useRef, useState } from "react";
import { emit } from "@tauri-apps/api/event";
import { api } from "@/lib/tauri";
import { POPOVER_GRACE_MS } from "./types";

// Popover Mode : survol du bouton (cette fenetre) + survol du popover
// (remonte par l'autre fenetre via "recorder:popover-hover").
export function useModePopover() {
  const [hoverButton, setHoverButton] = useState(false);
  const [hoverPopover, setHoverPopover] = useState(false);
  const popoverOpenRef = useRef(false);
  const closeTimerRef = useRef<number | null>(null);
  const modeButtonRef = useRef<HTMLButtonElement>(null);
  const pillRef = useRef<HTMLDivElement>(null);

  const openPopover = useCallback(async () => {
    const btn = modeButtonRef.current;
    const pill = pillRef.current;
    if (!btn || !pill) return;
    const p = pill.getBoundingClientRect();
    popoverOpenRef.current = true;
    try {
      await emit("recorder:popover-refresh", {});
      // Parlato: the picker is centred on the pill, not on the button.
      await api.openRecorderPopover(p.left + p.width / 2, p.top, p.bottom);
    } catch (e) {
      console.error(e);
    }
  }, []);

  const closePopover = useCallback(() => {
    popoverOpenRef.current = false;
    api.closeRecorderPopover().catch(console.error);
  }, []);

  const wantPopover = hoverButton || hoverPopover;
  useEffect(() => {
    if (closeTimerRef.current !== null) {
      window.clearTimeout(closeTimerRef.current);
      closeTimerRef.current = null;
    }
    if (wantPopover) {
      if (!popoverOpenRef.current) openPopover();
    } else if (popoverOpenRef.current) {
      closeTimerRef.current = window.setTimeout(closePopover, POPOVER_GRACE_MS);
    }
    return () => {
      if (closeTimerRef.current !== null) {
        window.clearTimeout(closeTimerRef.current);
        closeTimerRef.current = null;
      }
    };
  }, [wantPopover, openPopover, closePopover]);

  return {
    setHoverButton,
    setHoverPopover,
    popoverOpenRef,
    modeButtonRef,
    pillRef,
    openPopover,
    closePopover,
  };
}
