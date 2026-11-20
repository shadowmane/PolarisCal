import type { CalInterval } from "../bindings/CalInterval";
import type { ConditionReceived } from "../bindings/ConditionReceived";
import type { ConditionReturned } from "../bindings/ConditionReturned";
import type { DueStatus } from "../bindings/DueStatus";
import type { LookupKind } from "../bindings/LookupKind";

export const EMPTY = "—";

const dateFormat = new Intl.DateTimeFormat(undefined, { dateStyle: "medium" });

/** Formats a 'YYYY-MM-DD' date without shifting it through UTC. */
export function formatDate(iso: string | null | undefined): string {
  if (!iso) return EMPTY;
  const [y, m, d] = iso.split("-").map(Number);
  return dateFormat.format(new Date(y, m - 1, d));
}

const money = new Intl.NumberFormat(undefined, { style: "currency", currency: "USD" });

export function formatCents(cents: number | null | undefined): string {
  return cents == null ? EMPTY : money.format(cents / 100);
}

export function formatInterval(interval: CalInterval | null): string {
  if (!interval) return EMPTY;
  const unit = interval.unit === "months" ? "month" : "day";
  return `${interval.value} ${unit}${interval.value === 1 ? "" : "s"}`;
}

type ChipColor = "default" | "accent" | "success" | "warning" | "danger";

export const dueStatusMeta: Record<DueStatus, { label: string; color: ChipColor }> = {
  overdue: { label: "Overdue", color: "danger" },
  due_soon: { label: "Due soon", color: "warning" },
  current: { label: "Current", color: "success" },
  no_schedule: { label: "No schedule", color: "default" },
  not_tracked: { label: "Not tracked", color: "default" },
};

export const conditionReceivedMeta: Record<ConditionReceived, { label: string; color: ChipColor }> = {
  pass: { label: "Pass", color: "success" },
  out_of_tolerance: { label: "Out of tolerance", color: "warning" },
  inoperative: { label: "Inoperative", color: "danger" },
};

export const conditionReturnedMeta: Record<ConditionReturned, { label: string; color: ChipColor }> = {
  pass: { label: "Pass", color: "success" },
  fail: { label: "Fail", color: "danger" },
  limited: { label: "Limited calibration", color: "warning" },
};

export const lookupKindMeta: Record<LookupKind, { label: string; singular: string }> = {
  location: { label: "Locations", singular: "Location" },
  department: { label: "Departments", singular: "Department" },
  owner: { label: "Owners", singular: "Owner" },
  manufacturer: { label: "Manufacturers", singular: "Manufacturer" },
  provider: { label: "Calibration Providers", singular: "Provider" },
  status: { label: "Statuses", singular: "Status" },
};

export const LOOKUP_KINDS: LookupKind[] = [
  "location",
  "department",
  "owner",
  "manufacturer",
  "provider",
  "status",
];
