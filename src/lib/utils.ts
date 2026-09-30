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
