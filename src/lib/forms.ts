// Conversions between editable form state and the API input types.
// Pure functions, unit-tested in forms.test.ts.

import { type DateValue, parseDate } from "@internationalized/date";
import type { Asset } from "../bindings/Asset";
import type { AssetInput } from "../bindings/AssetInput";
import type { AssetKind } from "../bindings/AssetKind";
import type { CalibratedBy } from "../bindings/CalibratedBy";
import type { CalibrationInput } from "../bindings/CalibrationInput";
import type { CalibrationRecord } from "../bindings/CalibrationRecord";
import type { ConditionReceived } from "../bindings/ConditionReceived";
import type { ConditionReturned } from "../bindings/ConditionReturned";
import type { IntervalUnit } from "../bindings/IntervalUnit";
import type { LookupInput } from "../bindings/LookupInput";
import type { LookupItem } from "../bindings/LookupItem";
import type { LookupKind } from "../bindings/LookupKind";
import { ApiError } from "./api";

/** Field name → message. Field names match the Rust input structs. */
export type FieldErrors = Record<string, string>;

export type Converted<T> = { ok: true; value: T } | { ok: false; errors: FieldErrors };

// ---------------------------------------------------------------------------
// Primitive conversions
// ---------------------------------------------------------------------------

/** 'YYYY-MM-DD' → CalendarDate for HeroUI date pickers. */
export function toDateValue(iso: string | null | undefined): DateValue | null {
  if (!iso) return null;
  try {
    return parseDate(iso);
  } catch {
    return null;
  }
}

/** Date picker value → 'YYYY-MM-DD'. */
export function fromDateValue(value: DateValue | null | undefined): string | null {
  if (!value) return null;
  const pad = (n: number, w = 2) => String(n).padStart(w, "0");
  return `${pad(value.year, 4)}-${pad(value.month)}-${pad(value.day)}`;
}

/** Dollars (from a number field) → integer cents. Empty or invalid → null. */
export function dollarsToCents(dollars: number | null | undefined): number | null {
  if (dollars == null || Number.isNaN(dollars)) return null;
  return Math.round(dollars * 100);
}

export function centsToDollars(cents: number | null | undefined): number | null {
  return cents == null ? null : cents / 100;
}

/** Trimmed text, or null when blank. */
export function optionalText(value: string): string | null {
  const t = value.trim();
  return t === "" ? null : t;
}

/** Collects per-field messages from an API validation error. */
export function fieldErrors(error: unknown): FieldErrors {
  if (!(error instanceof ApiError)) return {};
  const out: FieldErrors = {};
  for (const f of error.fields) {
    if (f.field && !out[f.field]) out[f.field] = f.message;
  }
  return out;
}

/** Lookups offered in a picker: active ones, plus the current value even if it was deactivated. */
export function pickable(items: LookupItem[] | undefined, current: number | null): LookupItem[] {
  return (items ?? []).filter((i) => i.is_active || i.id === current);
}

// ---------------------------------------------------------------------------
// Instruments & Systems
// ---------------------------------------------------------------------------

export interface AssetForm {
  asset_tag: string;
  kind: AssetKind;
  parent_id: number | null;
  description: string;
  manufacturer_id: number | null;
  model_number: string;
  serial_number: string;
  location_id: number | null;
  department_id: number | null;
  owner_id: number | null;
  status_id: number | null;
  warranty_end_date: string | null;
  interval_value: number | null;
  interval_unit: IntervalUnit;
  interval_end_of_month: boolean;
  in_service_date: string | null;
  first_cal_date: string | null;
  last_cal_date: string | null;
  notes: string;
}

export function emptyAssetForm(defaults: Partial<AssetForm> = {}): AssetForm {
  return {
    asset_tag: "",
    kind: "instrument",
    parent_id: null,
    description: "",
    manufacturer_id: null,
    model_number: "",
    serial_number: "",
    location_id: null,
    department_id: null,
    owner_id: null,
    status_id: null,
    warranty_end_date: null,
    interval_value: 12,
    interval_unit: "months",
    interval_end_of_month: false,
    in_service_date: null,
    first_cal_date: null,
    last_cal_date: null,
    notes: "",
    ...defaults,
  };
}

export function assetToForm(a: Asset): AssetForm {
  return {
    asset_tag: a.asset_tag,
    kind: a.kind,
    parent_id: a.parent_id,
    description: a.description,
    manufacturer_id: a.manufacturer_id,
    model_number: a.model_number ?? "",
    serial_number: a.serial_number ?? "",
    location_id: a.location_id,
    department_id: a.department_id,
    owner_id: a.owner_id,
    status_id: a.status_id,
    warranty_end_date: a.warranty_end_date,
    interval_value: a.cal_interval?.value ?? null,
    interval_unit: a.cal_interval?.unit ?? "months",
    interval_end_of_month: a.cal_interval?.end_of_month ?? false,
    in_service_date: a.in_service_date,
    first_cal_date: a.first_cal_date,
    last_cal_date: a.last_cal_date_entered,
    notes: a.notes ?? "",
  };
}

export function formToAssetInput(f: AssetForm): Converted<AssetInput> {
  const errors: FieldErrors = {};
  if (!f.asset_tag.trim()) errors.asset_tag = "Asset tag is required.";
  if (!f.description.trim()) errors.description = "Description is required.";
  if (f.status_id == null) errors.status_id = "Status is required.";
  const hasInterval = f.interval_value != null && !Number.isNaN(f.interval_value);
  if (hasInterval && (f.interval_value! < 1 || !Number.isInteger(f.interval_value))) {
    errors.cal_interval = "Interval must be a whole number of at least 1.";
  }
  if (Object.keys(errors).length > 0) return { ok: false, errors };

  return {
    ok: true,
    value: {
      asset_tag: f.asset_tag.trim(),
      kind: f.kind,
      parent_id: f.kind === "instrument" ? f.parent_id : null,
      description: f.description.trim(),
      manufacturer_id: f.manufacturer_id,
      model_number: optionalText(f.model_number),
      serial_number: optionalText(f.serial_number),
      location_id: f.location_id,
      department_id: f.department_id,
      owner_id: f.owner_id,
      status_id: f.status_id!,
      warranty_end_date: f.warranty_end_date,
      cal_interval: hasInterval
        ? { value: f.interval_value!, unit: f.interval_unit, end_of_month: f.interval_end_of_month }
        : null,
      in_service_date: f.in_service_date,
      first_cal_date: f.first_cal_date,
      last_cal_date: f.last_cal_date,
      notes: optionalText(f.notes),
    },
  };
}

// ---------------------------------------------------------------------------
// Calibration records
// ---------------------------------------------------------------------------

export interface CalibrationForm {
  asset_id: number | null;
  cal_date: string | null;
  /** A calibration provider, or the manufacturer. */
  calibrated_by: CalibratedBy;
  calibrated_by_id: number | null;
  technician: string;
  /** Dollars as typed; converted to cents on save. */
  cost: number | null;
  certificate_number: string;
  condition_received: ConditionReceived | null;
  condition_returned: ConditionReturned | null;
  notes: string;
}

export function emptyCalibrationForm(defaults: Partial<CalibrationForm> = {}): CalibrationForm {
  return {
    asset_id: null,
    cal_date: null,
    calibrated_by: "provider",
    calibrated_by_id: null,
    technician: "",
    cost: null,
    certificate_number: "",
    condition_received: null,
    condition_returned: null,
    notes: "",
    ...defaults,
  };
}

export function calibrationToForm(r: CalibrationRecord): CalibrationForm {
  return {
    asset_id: r.asset_id,
    cal_date: r.cal_date,
    calibrated_by: r.calibrated_by,
    calibrated_by_id: r.calibrated_by_id,
    technician: r.technician ?? "",
    cost: centsToDollars(r.cost_cents),
    certificate_number: r.certificate_number ?? "",
    condition_received: r.condition_received,
    condition_returned: r.condition_returned,
    notes: r.notes ?? "",
  };
}

export function formToCalibrationInput(f: CalibrationForm): Converted<CalibrationInput> {
  const errors: FieldErrors = {};
  if (f.asset_id == null) errors.asset_id = "Choose the item that was calibrated.";
  if (!f.cal_date) errors.cal_date = "Calibration date is required.";
  if (f.calibrated_by_id == null)
    errors.calibrated_by_id =
      f.calibrated_by === "manufacturer" ? "Choose the manufacturer." : "Choose the calibration provider.";
  if (!f.condition_received) errors.condition_received = "Condition received is required.";
  if (!f.condition_returned) errors.condition_returned = "Condition returned is required.";
  const cents = dollarsToCents(f.cost);
  if (cents != null && cents < 0) errors.cost_cents = "Cost cannot be negative.";
  if (Object.keys(errors).length > 0) return { ok: false, errors };

  return {
    ok: true,
    value: {
      asset_id: f.asset_id!,
      cal_date: f.cal_date!,
      calibrated_by: f.calibrated_by,
      calibrated_by_id: f.calibrated_by_id!,
      technician: optionalText(f.technician),
      cost_cents: cents,
      certificate_number: optionalText(f.certificate_number),
      condition_received: f.condition_received!,
      condition_returned: f.condition_returned!,
      notes: optionalText(f.notes),
    },
  };
}

// ---------------------------------------------------------------------------
// Pick-list entries
// ---------------------------------------------------------------------------

export function emptyLookupInput(kind: LookupKind, sortOrder = 0): LookupInput {
  return {
    name: "",
    description: null,
    is_active: true,
    sort_order: sortOrder,
    is_internal: kind === "provider" ? false : null,
    tracks_due: kind === "status" ? true : null,
  };
}

export function lookupToInput(item: LookupItem): LookupInput {
  return {
    name: item.name,
    description: item.description,
    is_active: item.is_active,
    sort_order: item.sort_order,
    is_internal: item.is_internal,
    tracks_due: item.tracks_due,
  };
}
