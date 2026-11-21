// Report definitions: each builder turns loaded records into a table that is shown on
// screen, printed, and exported to Excel unchanged.

import type { Asset } from "../bindings/Asset";
import type { CalibrationRecord } from "../bindings/CalibrationRecord";
import type { ColumnKind } from "../bindings/ColumnKind";
import type { ExportCell } from "../bindings/ExportCell";
import type { ExportSheet } from "../bindings/ExportSheet";
import { conditionReceivedMeta, conditionReturnedMeta, formatInterval } from "./format";

export interface ReportColumn {
  header: string;
  kind: ColumnKind;
  /** Excel column width in characters. */
  width?: number;
}

export interface ReportTable {
  title: string;
  subtitle?: string;
  columns: ReportColumn[];
  rows: ExportCell[][];
  /** Row ids (asset ids) for linking rows back to items; same order as `rows`. */
  rowLinks?: (number | null)[];
}

export function toSheet(report: ReportTable): ExportSheet {
  return {
    title: report.title,
    subtitle: report.subtitle ?? null,
    columns: report.columns.map((c) => ({ header: c.header, kind: c.kind, width: c.width ?? null })),
    rows: report.rows,
  };
}

/** Today's date as 'YYYY-MM-DD' in local time. */
export function isoToday(now = new Date()): string {
  const p = (n: number) => String(n).padStart(2, "0");
  return `${now.getFullYear()}-${p(now.getMonth() + 1)}-${p(now.getDate())}`;
}

function utcDay(iso: string): number {
  const [y, m, d] = iso.split("-").map(Number);
  return Date.UTC(y, m - 1, d) / 86_400_000;
}

/** Whole days from `from` to `to` (negative when `to` is earlier). */
export function daysBetween(from: string, to: string): number {
  return Math.round(utcDay(to) - utcDay(from));
}

/** 'YYYY-MM-DD' `days` after `iso` (negative goes back). */
export function addDays(iso: string, days: number): string {
  const d = new Date(utcDay(iso) * 86_400_000 + days * 86_400_000);
  return d.toISOString().slice(0, 10);
}

const t = (s: string | null | undefined): ExportCell => (s == null || s === "" ? null : s);

function makeModel(a: Asset): string | null {
  const parts = [a.manufacturer_name, a.model_number].filter(Boolean);
  return parts.length ? parts.join(" ") : null;
}

// ---------------------------------------------------------------------------
// Due and overdue
// ---------------------------------------------------------------------------

export interface DueOptions {
  /** Include items due within this many days; 0 means overdue only. */
  withinDays: number;
  locationId: number | null;
  departmentId: number | null;
  ownerId: number | null;
}

export interface Named {
  location?: string;
  department?: string;
  owner?: string;
}

function filterNote(names: Named): string {
  return [names.location, names.department, names.owner].filter(Boolean).join(" · ");
}

export function dueReport(assets: Asset[], options: DueOptions, today: string, names: Named = {}): ReportTable {
  const limit = addDays(today, options.withinDays);
  const due = assets
    .filter(
      (a) =>
        !a.is_archived &&
        a.next_cal_date != null &&
        (a.due_status === "overdue" || a.due_status === "due_soon" || a.due_status === "current") &&
        (options.withinDays === 0 ? a.next_cal_date < today : a.next_cal_date <= limit) &&
        (options.locationId == null || a.location_id === options.locationId) &&
        (options.departmentId == null || a.department_id === options.departmentId) &&
        (options.ownerId == null || a.owner_id === options.ownerId),
    )
    .sort((x, y) => (x.next_cal_date! < y.next_cal_date! ? -1 : x.next_cal_date! > y.next_cal_date! ? 1 : x.asset_tag.localeCompare(y.asset_tag)));

  const window = options.withinDays === 0 ? "Overdue items" : `Overdue or due within ${options.withinDays} days`;
  const note = filterNote(names);
  return {
    title: "Due and overdue",
    subtitle: [window, `as of ${today}`, note].filter(Boolean).join(" · "),
    columns: [
      { header: "Asset tag", kind: "text", width: 14 },
      { header: "Description", kind: "text", width: 32 },
      { header: "Make / model", kind: "text", width: 22 },
      { header: "Serial", kind: "text", width: 14 },
      { header: "Location", kind: "text", width: 18 },
      { header: "Department", kind: "text", width: 16 },
      { header: "Owner", kind: "text", width: 16 },
      { header: "Last calibrated", kind: "date" },
      { header: "Due", kind: "date" },
      { header: "Days left", kind: "number", width: 10 },
    ],
    rows: due.map((a) => [
      a.asset_tag,
      a.description,
      t(makeModel(a)),
      t(a.serial_number),
      t(a.location_name),
      t(a.department_name),
      t(a.owner_name),
      t(a.last_cal_date),
      a.next_cal_date,
      daysBetween(today, a.next_cal_date!),
    ]),
    rowLinks: due.map((a) => a.id),
  };
}

// ---------------------------------------------------------------------------
// Out of tolerance
// ---------------------------------------------------------------------------

/** Calibrations where the item arrived out of tolerance or inoperative, newest first. */
export function outOfToleranceReport(
  calibrations: CalibrationRecord[],
  assets: Asset[],
  from: string,
  to: string,
): ReportTable {
  const byId = new Map(assets.map((a) => [a.id, a]));
  const found = calibrations
    .filter((c) => !c.is_archived && c.condition_received !== "pass" && c.cal_date >= from && c.cal_date <= to)
    .sort((x, y) => (x.cal_date < y.cal_date ? 1 : x.cal_date > y.cal_date ? -1 : y.id - x.id));
  return {
    title: "Out of tolerance",
    subtitle: `Received out of tolerance or inoperative, ${from} to ${to}`,
    columns: [
      { header: "Calibrated", kind: "date" },
      { header: "Asset tag", kind: "text", width: 14 },
      { header: "Description", kind: "text", width: 30 },
      { header: "Location", kind: "text", width: 18 },
      { header: "Calibrated by", kind: "text", width: 22 },
      { header: "Technician", kind: "text", width: 16 },
      { header: "Certificate", kind: "text", width: 16 },
      { header: "As received", kind: "text", width: 16 },
      { header: "As returned", kind: "text", width: 18 },
      { header: "Notes", kind: "text", width: 40 },
    ],
    rows: found.map((c) => {
      const a = byId.get(c.asset_id);
      return [
        c.cal_date,
        c.asset_tag,
        t(a?.description),
        t(a?.location_name),
        c.calibrated_by_name,
        t(c.technician),
        t(c.certificate_number),
        conditionReceivedMeta[c.condition_received].label,
        conditionReturnedMeta[c.condition_returned].label,
        t(c.notes),
      ];
    }),
    rowLinks: found.map((c) => c.asset_id),
  };
}

// ---------------------------------------------------------------------------
// Register and item history
// ---------------------------------------------------------------------------

export function registerReport(assets: Asset[], includeArchived: boolean, note = ""): ReportTable {
  const items = assets
    .filter((a) => includeArchived || !a.is_archived)
    .sort((x, y) => x.asset_tag.localeCompare(y.asset_tag));
  return {
    title: "Instrument register",
    subtitle: [`${items.length} item${items.length === 1 ? "" : "s"}`, includeArchived ? "including archived" : "", note]
      .filter(Boolean)
      .join(" · "),
    columns: [
      { header: "Asset tag", kind: "text", width: 14 },
      { header: "Type", kind: "text", width: 11 },
      { header: "Part of", kind: "text", width: 12 },
      { header: "Description", kind: "text", width: 30 },
      { header: "Manufacturer", kind: "text", width: 18 },
      { header: "Model", kind: "text", width: 14 },
      { header: "Serial", kind: "text", width: 14 },
      { header: "Location", kind: "text", width: 18 },
      { header: "Department", kind: "text", width: 16 },
      { header: "Owner", kind: "text", width: 16 },
      { header: "Status", kind: "text", width: 16 },
      { header: "Interval", kind: "text", width: 11 },
      { header: "In service", kind: "date" },
      { header: "Warranty ends", kind: "date" },
      { header: "First calibrated", kind: "date" },
      { header: "Last calibrated", kind: "date" },
      { header: "Next due", kind: "date" },
    ],
    rows: items.map((a) => [
      a.asset_tag,
      a.kind === "system" ? "System" : "Instrument",
      t(a.parent_tag),
      a.description,
      t(a.manufacturer_name),
      t(a.model_number),
      t(a.serial_number),
      t(a.location_name),
      t(a.department_name),
      t(a.owner_name),
      a.is_archived ? `${a.status_name} (archived)` : a.status_name,
      a.cal_interval ? formatInterval(a.cal_interval) : null,
      t(a.in_service_date),
      t(a.warranty_end_date),
      t(a.first_cal_date),
      t(a.last_cal_date),
      t(a.next_cal_date),
    ]),
    rowLinks: items.map((a) => a.id),
  };
}

export function historyReport(asset: Asset, calibrations: CalibrationRecord[]): ReportTable {
  const records = calibrations
    .filter((c) => c.asset_id === asset.id && !c.is_archived)
    .sort((x, y) => (x.cal_date < y.cal_date ? 1 : x.cal_date > y.cal_date ? -1 : y.id - x.id));
  const identity = [asset.description, makeModel(asset), asset.serial_number ? `S/N ${asset.serial_number}` : null]
    .filter(Boolean)
    .join(" · ");
  return {
    title: `Calibration history: ${asset.asset_tag}`,
    subtitle: identity,
    columns: [
      { header: "Calibrated", kind: "date" },
      { header: "Calibrated by", kind: "text", width: 22 },
      { header: "Technician", kind: "text", width: 16 },
      { header: "Certificate", kind: "text", width: 16 },
      { header: "As received", kind: "text", width: 16 },
      { header: "As returned", kind: "text", width: 18 },
      { header: "Cost", kind: "money" },
      { header: "Next due", kind: "date" },
      { header: "Notes", kind: "text", width: 40 },
    ],
    rows: records.map((c) => [
      c.cal_date,
      c.calibrated_by_name,
      t(c.technician),
      t(c.certificate_number),
      conditionReceivedMeta[c.condition_received].label,
      conditionReturnedMeta[c.condition_returned].label,
      c.cost_cents,
      t(c.next_due_date),
      t(c.notes),
    ]),
  };
}

// ---------------------------------------------------------------------------
// Labels
// ---------------------------------------------------------------------------

export interface LabelData {
  assetId: number;
  tag: string;
  description: string;
  calibrated: string | null;
  due: string | null;
  by: string | null;
}

/** One label per item: its last active calibration's date and who did it, and the next due date. */
export function labelData(assets: Asset[], calibrations: CalibrationRecord[]): LabelData[] {
  const latest = new Map<number, CalibrationRecord>();
  for (const c of calibrations) {
    if (c.is_archived) continue;
    const seen = latest.get(c.asset_id);
    if (!seen || c.cal_date > seen.cal_date || (c.cal_date === seen.cal_date && c.id > seen.id)) latest.set(c.asset_id, c);
  }
  return assets.map((a) => {
    const c = latest.get(a.id);
    const by = c ? [c.calibrated_by_name, c.technician].filter(Boolean).join(", ") : null;
    return {
      assetId: a.id,
      tag: a.asset_tag,
      description: a.description,
      calibrated: a.last_cal_date,
      due: a.next_cal_date,
      by,
    };
  });
}

export interface LabelSheet {
  id: string;
  label: string;
  description: string;
  /** CSS page size for @page. */
  page: string;
  columns: number;
  rows: number;
  width: string;
  height: string;
  marginTop: string;
  marginLeft: string;
  columnGap: string;
  rowGap: string;
  /** Room for the description line. */
  roomy: boolean;
}

export const LABEL_SHEETS: LabelSheet[] = [
  {
    id: "letter-30",
    label: "Letter, 30 per sheet (Avery 5160 size)",
    description: '2⅝ × 1 in labels, 3 across, 10 down.',
    page: "letter",
    columns: 3,
    rows: 10,
    width: "2.625in",
    height: "1in",
    marginTop: "0.5in",
    marginLeft: "0.1875in",
    columnGap: "0.125in",
    rowGap: "0in",
    roomy: false,
  },
  {
    id: "letter-10",
    label: "Letter, 10 per sheet (Avery 5163 size)",
    description: "4 × 2 in labels, 2 across, 5 down.",
    page: "letter",
    columns: 2,
    rows: 5,
    width: "4in",
    height: "2in",
    marginTop: "0.5in",
    marginLeft: "0.15625in",
    columnGap: "0.1875in",
    rowGap: "0in",
    roomy: true,
  },
  {
    id: "a4-14",
    label: "A4, 14 per sheet (L7163 size)",
    description: "99.1 × 38.1 mm labels, 2 across, 7 down.",
    page: "A4",
    columns: 2,
    rows: 7,
    width: "99.1mm",
    height: "38.1mm",
    marginTop: "15.1mm",
    marginLeft: "4.65mm",
    columnGap: "2.5mm",
    rowGap: "0mm",
    roomy: true,
  },
  {
    id: "roll",
    label: "Label printer, one per label (2¼ × 1¼ in)",
    description: "For Dymo, Brother, or Zebra-style label printers.",
    page: "2.25in 1.25in",
    columns: 1,
    rows: 1,
    width: "2.25in",
    height: "1.25in",
    marginTop: "0in",
    marginLeft: "0in",
    columnGap: "0in",
    rowGap: "0in",
    roomy: false,
  },
];

/** Splits labels into pages, leaving `skip` empty positions on the first page (for part-used sheets). */
export function paginateLabels<T>(labels: T[], perPage: number, skip: number): (T | null)[][] {
  const cells: (T | null)[] = [...Array(Math.max(0, Math.min(skip, perPage - 1))).fill(null), ...labels];
  const pages: (T | null)[][] = [];
  for (let i = 0; i < cells.length; i += perPage) pages.push(cells.slice(i, i + perPage));
  return pages;
}
