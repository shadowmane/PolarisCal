// Reservation dates (plain YYYY-MM-DD days, no time zones) and the sidebar badge.

import { api } from "./api";
import { countChanged, useLiveCount } from "./liveCount";

const CHANGED_EVENT = "polariscal:reservations-changed";

/** Tells the Reservations badge (and the Dashboard) to check again. */
export function reservationsChanged() {
  countChanged(CHANGED_EVENT);
}

/** Requests to end your reservations early, and (for Technicians and Administrators) handovers. */
export function useReservationActionCount(enabled: boolean): number {
  return useLiveCount(api.reservationActions, enabled, CHANGED_EVENT);
}

export function toYmd(d: Date): string {
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

/** Midday local time, so adding days never trips over daylight-saving changes. */
export function fromYmd(ymd: string): Date {
  const [y, m, d] = ymd.split("-").map(Number);
  return new Date(y, m - 1, d, 12);
}

export function todayYmd(): string {
  return toYmd(new Date());
}

export function addDays(ymd: string, days: number): string {
  const d = fromYmd(ymd);
  d.setDate(d.getDate() + days);
  return toYmd(d);
}

/** Days from `start` to `end`, counting both (1 for a single day). */
export function spanDays(start: string, end: string): number {
  return Math.round((fromYmd(end).getTime() - fromYmd(start).getTime()) / 86_400_000) + 1;
}
