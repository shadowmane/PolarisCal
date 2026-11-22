// The organization's date format (an admin setting), shared by every date display.

import { useSyncExternalStore } from "react";

export type DateFormatId =
  | "system"
  | "mdy_numeric"
  | "mdy_medium"
  | "mdy_long"
  | "iso"
  | "dmy_slash"
  | "dmy_dot"
  | "dmy_dash"
  | "dmy_medium"
  | "dmy_long"
  | "dmy_mil"
  | "ymd_slash";

/**
 * The formats offered in Settings (the same ids as `AppSettings::DATE_FORMATS` in Rust).
 * `inputLocale` sets the order of the day, month and year boxes in date fields: an
 * English locale that writes dates in that order, so the placeholders stay in English.
 */
export const DATE_FORMATS: { id: DateFormatId; pattern: string; usedIn: string; inputLocale?: string }[] = [
  { id: "system", pattern: "Windows region setting", usedIn: "Each PC shows dates its own way" },
  { id: "mdy_numeric", pattern: "MM/DD/YYYY", usedIn: "United States", inputLocale: "en-US" },
  { id: "mdy_medium", pattern: "Mon D, YYYY", usedIn: "United States", inputLocale: "en-US" },
  { id: "mdy_long", pattern: "Month D, YYYY", usedIn: "United States", inputLocale: "en-US" },
  { id: "iso", pattern: "YYYY-MM-DD", usedIn: "ISO 8601 international standard", inputLocale: "en-CA" },
  { id: "dmy_slash", pattern: "DD/MM/YYYY", usedIn: "UK, most of Europe, Latin America, Australia", inputLocale: "en-GB" },
  { id: "dmy_dot", pattern: "DD.MM.YYYY", usedIn: "Germany, Central and Eastern Europe", inputLocale: "en-GB" },
  { id: "dmy_dash", pattern: "DD-MM-YYYY", usedIn: "Netherlands, India", inputLocale: "en-GB" },
  { id: "dmy_medium", pattern: "D Mon YYYY", usedIn: "UK, international", inputLocale: "en-GB" },
  { id: "dmy_long", pattern: "D Month YYYY", usedIn: "UK, international", inputLocale: "en-GB" },
  { id: "dmy_mil", pattern: "DD-Mon-YYYY", usedIn: "Military and laboratory records", inputLocale: "en-GB" },
  { id: "ymd_slash", pattern: "YYYY/MM/DD", usedIn: "Japan, China, Korea, Taiwan", inputLocale: "en-ZA" },
];

const MONTHS = [
  "January",
  "February",
  "March",
  "April",
  "May",
  "June",
  "July",
  "August",
  "September",
  "October",
  "November",
  "December",
];

const systemDate = new Intl.DateTimeFormat(undefined, { dateStyle: "medium" });

/** A calendar date in `format`. `m` is 1–12. */
export function formatYmd(y: number, m: number, d: number, format: DateFormatId): string {
  const dd = String(d).padStart(2, "0");
  const mm = String(m).padStart(2, "0");
  const yyyy = String(y).padStart(4, "0");
  const month = MONTHS[m - 1] ?? "";
  const mon = month.slice(0, 3);
  switch (format) {
    case "mdy_numeric":
      return `${mm}/${dd}/${yyyy}`;
    case "mdy_medium":
      return `${mon} ${d}, ${yyyy}`;
    case "mdy_long":
      return `${month} ${d}, ${yyyy}`;
    case "iso":
      return `${yyyy}-${mm}-${dd}`;
    case "dmy_slash":
      return `${dd}/${mm}/${yyyy}`;
    case "dmy_dot":
      return `${dd}.${mm}.${yyyy}`;
    case "dmy_dash":
      return `${dd}-${mm}-${yyyy}`;
    case "dmy_medium":
      return `${d} ${mon} ${yyyy}`;
    case "dmy_long":
      return `${d} ${month} ${yyyy}`;
    case "dmy_mil":
      return `${dd}-${mon}-${yyyy}`;
    case "ymd_slash":
      return `${yyyy}/${mm}/${dd}`;
    default:
      return systemDate.format(new Date(y, m - 1, d));
  }
}

let current: DateFormatId = "system";
const listeners = new Set<() => void>();

export function dateFormat(): DateFormatId {
  return current;
}

export function setDateFormat(next: string | null | undefined) {
  const value = DATE_FORMATS.some((f) => f.id === next) ? (next as DateFormatId) : "system";
  if (value === current) return;
  current = value;
  for (const l of listeners) l();
}

/** Re-renders the caller when the date format changes. */
export function useDateFormat(): DateFormatId {
  return useSyncExternalStore(
    (l) => {
      listeners.add(l);
      return () => listeners.delete(l);
    },
    () => current,
  );
}

/** The locale whose day/month/year order date fields use, or undefined for the PC's own. */
export function dateInputLocale(format: DateFormatId = current): string | undefined {
  return DATE_FORMATS.find((f) => f.id === format)?.inputLocale;
}
