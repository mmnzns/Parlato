import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

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
  return slot < 9 ? `Alt+${slot + 1}` : "Alt+0";
}
