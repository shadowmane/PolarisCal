// Typed wrappers around the Rust commands in src-tauri/src/commands.rs.
// Types come from src/bindings, generated from the Rust models by ts-rs (`cargo test`).

import { invoke, isTauri } from "@tauri-apps/api/core";
import type { AppInfo } from "../bindings/AppInfo";
import type { Asset } from "../bindings/Asset";
import type { AssetFilter } from "../bindings/AssetFilter";
import type { AssetInput } from "../bindings/AssetInput";
import type { CalibrationFilter } from "../bindings/CalibrationFilter";
import type { CalibrationInput } from "../bindings/CalibrationInput";
import type { CalibrationRecord } from "../bindings/CalibrationRecord";
import type { CommandError } from "../bindings/CommandError";
import type { DueSummary } from "../bindings/DueSummary";
import type { ErrorKind } from "../bindings/ErrorKind";
import type { FieldError } from "../bindings/FieldError";
import type { LookupInput } from "../bindings/LookupInput";
import type { LookupItem } from "../bindings/LookupItem";
import type { LookupKind } from "../bindings/LookupKind";

export class ApiError extends Error {
  readonly kind: ErrorKind | "unavailable";
  readonly fields: FieldError[];

  constructor(kind: ApiError["kind"], message: string, fields: FieldError[] = []) {
    super(message);
    this.name = "ApiError";
    this.kind = kind;
    this.fields = fields;
  }
}

function isCommandError(e: unknown): e is CommandError {
  return typeof e === "object" && e !== null && "kind" in e && "message" in e;
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!isTauri()) {
    throw new ApiError(
      "unavailable",
      "The PolarisCal backend is not available. Run the app with `npm run tauri dev`.",
    );
  }
  try {
    return await invoke<T>(command, args);
  } catch (e) {
    if (isCommandError(e)) throw new ApiError(e.kind, e.message, e.fields);
    throw new ApiError("database", String(e));
  }
}

export const api = {
  appInfo: () => call<AppInfo>("app_info"),

  listLookups: (kind: LookupKind, includeInactive = false) =>
    call<LookupItem[]>("list_lookups", { kind, include_inactive: includeInactive }),
  createLookup: (kind: LookupKind, input: LookupInput) =>
    call<LookupItem>("create_lookup", { kind, input }),
  updateLookup: (kind: LookupKind, id: number, input: LookupInput) =>
    call<LookupItem>("update_lookup", { kind, id, input }),

  listAssets: (filter: Partial<AssetFilter> = {}) =>
    call<Asset[]>("list_assets", {
      filter: { search: null, kind: null, parent_id: null, include_archived: false, ...filter },
    }),
  getAsset: (id: number) => call<Asset>("get_asset", { id }),
  createAsset: (input: AssetInput) => call<Asset>("create_asset", { input }),
  updateAsset: (id: number, expectedVersion: number, input: AssetInput) =>
    call<Asset>("update_asset", { id, expected_version: expectedVersion, input }),
  setAssetArchived: (id: number, expectedVersion: number, archived: boolean) =>
    call<Asset>("set_asset_archived", { id, expected_version: expectedVersion, archived }),

  listCalibrations: (filter: Partial<CalibrationFilter> = {}) =>
    call<CalibrationRecord[]>("list_calibrations", {
      filter: { asset_id: null, include_archived: false, ...filter },
    }),
  getCalibration: (id: number) => call<CalibrationRecord>("get_calibration", { id }),
  createCalibration: (input: CalibrationInput) =>
    call<CalibrationRecord>("create_calibration", { input }),
  updateCalibration: (id: number, expectedVersion: number, input: CalibrationInput) =>
    call<CalibrationRecord>("update_calibration", { id, expected_version: expectedVersion, input }),
  setCalibrationArchived: (id: number, expectedVersion: number, archived: boolean) =>
    call<CalibrationRecord>("set_calibration_archived", {
      id,
      expected_version: expectedVersion,
      archived,
    }),

  dueSummary: () => call<DueSummary>("due_summary"),
};
