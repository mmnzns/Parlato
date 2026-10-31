// Parlato: pure helpers shared by the History screen (formatting, day
// grouping, failure text).

import type { useTranslation } from "react-i18next";
import { translateError } from "@/lib/translateError";

/// Si le texte est une erreur formate "Transcription Failed: PARLA_ERR:...",
/// extrait le code et le traduit. Sinon retourne le texte tel quel.
export function localizeFailureText(t: ReturnType<typeof useTranslation>["t"], raw: string): string {
  const prefix = "Transcription Failed: ";
  if (!raw.startsWith(prefix)) return raw;
  const code = raw.slice(prefix.length);
  return translateError(t, code);
}

export const PAGE_SIZE = 20;

export type Filter = "all" | "ai" | "failed";

export function formatClock(secs: number | null): string | null {
  if (!secs || secs <= 0) return null;
  const total = Math.round(secs);
  return `${Math.floor(total / 60)}:${(total % 60).toString().padStart(2, "0")}`;
}

export function formatSecs(secs: number | null): string | null {
  if (!secs || secs <= 0) return null;
  return `${secs.toFixed(1)} s`;
}

export function formatTime(iso: string): string {
  try {
    return new Date(iso).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  } catch {
    return iso;
  }
}

export function dayKey(iso: string): string {
  const d = new Date(iso);
  return `${d.getFullYear()}-${d.getMonth()}-${d.getDate()}`;
}

export function dayLabel(iso: string, t: ReturnType<typeof useTranslation>["t"]): string {
  const d = new Date(iso);
  const today = new Date();
  const yesterday = new Date();
  yesterday.setDate(today.getDate() - 1);
  if (dayKey(iso) === dayKey(today.toISOString())) return t("hist.today");
  if (dayKey(iso) === dayKey(yesterday.toISOString())) return t("hist.yesterday");
  return d.toLocaleDateString([], { weekday: "long", month: "long", day: "numeric" });
}
