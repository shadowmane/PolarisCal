import { formatMoney } from "./currency";
import { dateFormat, formatYmd } from "./dates";
import type { AttachmentCategory } from "../bindings/AttachmentCategory";
import type { AuditAction } from "../bindings/AuditAction";
import type { AuditEntity } from "../bindings/AuditEntity";
import type { CalInterval } from "../bindings/CalInterval";
import type { ConditionReceived } from "../bindings/ConditionReceived";
import type { ConditionReturned } from "../bindings/ConditionReturned";
import type { DueStatus } from "../bindings/DueStatus";
import type { LookupKind } from "../bindings/LookupKind";

export const EMPTY = "—";

/** Formats a 'YYYY-MM-DD' date in the organization's date format, without shifting it through UTC. */
export function formatDate(iso: string | null | undefined): string {
  if (!iso) return EMPTY;
  const [y, m, d] = iso.split("-").map(Number);
  return formatYmd(y, m, d, dateFormat());
}

/** Money in the organization's currency (Settings). Components showing it call `useCurrencySymbol`. */
export function formatCents(cents: number | null | undefined): string {
  return cents == null ? EMPTY : formatMoney(cents / 100);
}

/** The interval, or "From its Instruments" for an aggregate System. */
export function formatSchedule(a: { cal_interval: CalInterval | null; cal_aggregate: boolean }): string {
  return a.cal_aggregate ? "From its Instruments" : formatInterval(a.cal_interval);
}

export function formatInterval(interval: CalInterval | null): string {
  if (!interval) return EMPTY;
  const unit = interval.unit === "months" ? "month" : "day";
  const eom = interval.end_of_month ? ", end of month" : "";
  return `${interval.value} ${unit}${interval.value === 1 ? "" : "s"}${eom}`;
}

type ChipColor = "default" | "accent" | "success" | "warning" | "danger";

export const dueStatusMeta: Record<DueStatus, { label: string; color: ChipColor }> = {
  overdue: { label: "Overdue", color: "danger" },
  extended: { label: "Calibration extended", color: "warning" },
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
  service_type: { label: "Service Types", singular: "Service type" },
};

export const LOOKUP_KINDS: LookupKind[] = [
  "location",
  "department",
  "owner",
  "manufacturer",
  "provider",
  "status",
  "service_type",
];

const dateTimeFormat = new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" });
const timeFormat = new Intl.DateTimeFormat(undefined, { timeStyle: "short" });

/** Formats an RFC 3339 timestamp in local time: the organization's date format, the PC's time format. */
export function formatDateTime(iso: string | null | undefined): string {
  if (!iso) return EMPTY;
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  const format = dateFormat();
  if (format === "system") return dateTimeFormat.format(d);
  return `${formatYmd(d.getFullYear(), d.getMonth() + 1, d.getDate(), format)}, ${timeFormat.format(d)}`;
}

export const auditActionMeta: Record<AuditAction, { label: string; color: ChipColor }> = {
  create: { label: "Created", color: "success" },
  update: { label: "Edited", color: "accent" },
  archive: { label: "Archived", color: "warning" },
  restore: { label: "Restored", color: "accent" },
  delete: { label: "Deleted", color: "danger" },
  sign_in: { label: "Signed in", color: "default" },
  sign_in_failed: { label: "Sign-in failed", color: "danger" },
  sign_out: { label: "Signed out", color: "default" },
  password_change: { label: "Password changed", color: "default" },
  password_reset: { label: "Password reset", color: "warning" },
  password_reset_request: { label: "Password reset requested", color: "danger" },
  password_expired: { label: "Password expired", color: "warning" },
  export: { label: "Data copied out", color: "default" },
  import: { label: "Data copied in", color: "accent" },
};

export const auditEntityLabel: Record<AuditEntity, string> = {
  asset: "Item",
  calibration: "Service record",
  lookup: "List entry",
  user: "User",
  session: "Session",
  settings: "Settings",
  attachment: "File",
  certificate: "Certificate",
  database: "Database",
};

export const attachmentCategoryMeta: Record<AttachmentCategory, { label: string; color: ChipColor }> = {
  certificate: { label: "Certificate", color: "accent" },
  document: { label: "Document", color: "default" },
  photo: { label: "Photo", color: "success" },
  service_manual: { label: "Service manual", color: "warning" },
  other: { label: "Other", color: "default" },
};

export const ATTACHMENT_CATEGORIES: AttachmentCategory[] = ["certificate", "photo", "service_manual", "document", "other"];

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB"];
  let v = bytes / 1024;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v < 10 ? v.toFixed(1) : Math.round(v)} ${units[i]}`;
}
