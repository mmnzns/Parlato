// Parlato: one file size formatter for the whole app. Decimal units (1 MB =
// 1,000,000 bytes, as macOS and Hugging Face count), with the unit written
// the way the app's language writes it ("1.5 MB", "1,5 Mo", "1,5 MB").

import i18n from "@/i18n";

const UNITS = [
  { min: 1e9, unit: "gigabyte", digits: 1 },
  { min: 1e6, unit: "megabyte", digits: 0 },
  { min: 1e3, unit: "kilobyte", digits: 0 },
  { min: 0, unit: "byte", digits: 0 },
] as const;

export function formatBytes(bytes: number | null | undefined): string {
  if (bytes == null || !Number.isFinite(bytes)) return "-";
  const { min, unit, digits } = UNITS.find((u) => bytes >= u.min) ?? UNITS[UNITS.length - 1];
  const value = min > 0 ? bytes / min : bytes;
  // One decimal below 10 MB so small models do not all read "1 MB".
  const fraction = unit === "megabyte" && value < 10 ? 1 : digits;
  try {
    return new Intl.NumberFormat(i18n.resolvedLanguage ?? "en", {
      style: "unit",
      unit,
      unitDisplay: "short",
      maximumFractionDigits: fraction,
    }).format(value);
  } catch {
    return `${value.toFixed(fraction)} ${unit}`;
  }
}
