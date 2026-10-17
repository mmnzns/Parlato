import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";
import { isMac } from "@/lib/platform";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

/**
 * Libelle d'un raccourci Power Mode (config.shortcut_slot, 0-base).
 * 0..8 -> Alt+1..Alt+9, 9 -> Alt+0 (comme VoiceInk Option+1..0). Retourne
 * null sans raccourci ou pour un slot invalide.
 */
export function powerShortcutLabel(slot: number | null | undefined): string | null {
  if (slot == null || slot < 0 || slot > 9) return null;
  // Parlato: the same key is called Option on a Mac.
  const mod = isMac ? "Option" : "Alt";
  return slot < 9 ? `${mod}+${slot + 1}` : `${mod}+0`;
}

/** Parlato: what stands for a Power mode in the pill and its menu. Modes
 *  made in Parlato have no icon ("*"), so they show their first letter. */
export function modeBadge(emoji: string | null | undefined, name: string): string {
  const e = (emoji ?? "").trim();
  if (e && e !== "*") return e;
  return Array.from(name.trim())[0]?.toLocaleUpperCase() ?? "*";
}
