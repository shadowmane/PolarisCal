// Typed wrappers around the Rust commands in src-tauri/src/commands.rs.
// Types come from src/bindings, generated from the Rust models by ts-rs (`cargo test`).

import { invoke, isTauri } from "@tauri-apps/api/core";
import type { AppInfo } from "../bindings/AppInfo";
import type { AppSettings } from "../bindings/AppSettings";
import type { Asset } from "../bindings/Asset";
import type { AssetFilter } from "../bindings/AssetFilter";
import type { AssetInput } from "../bindings/AssetInput";
import type { AuditEntry } from "../bindings/AuditEntry";
import type { AuditFilter } from "../bindings/AuditFilter";
import type { AuthStatus } from "../bindings/AuthStatus";
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
import type { NewUser } from "../bindings/NewUser";
import type { User } from "../bindings/User";
import type { UserUpdate } from "../bindings/UserUpdate";

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

/** Fired when a command reports the session ended or locked, so the auth gate re-checks. */
export const AUTH_CHANGED_EVENT = "polariscal:auth-changed";

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
    const err = isCommandError(e) ? new ApiError(e.kind, e.message, e.fields) : new ApiError("database", String(e));
    if ((err.kind === "locked" || err.kind === "unauthenticated") && !SESSION_COMMANDS.has(command)) {
      window.dispatchEvent(new Event(AUTH_CHANGED_EVENT));
    }
    throw err;
  }
}

/** Commands whose auth errors are expected answers, not a sign the session changed. */
const SESSION_COMMANDS = new Set(["sign_in", "unlock_session", "setup_admin"]);

export const api = {
  appInfo: () => call<AppInfo>("app_info"),

  // Session
  authStatus: () => call<AuthStatus>("auth_status"),
  setupAdmin: (input: NewUser) => call<User>("setup_admin", { input }),
  signIn: (username: string, password: string) => call<User>("sign_in", { username, password }),
  signOut: () => call<void>("sign_out"),
  lockSession: () => call<void>("lock_session"),
  unlockSession: (password: string) => call<User>("unlock_session", { password }),
  touchSession: () => call<void>("touch_session"),
  changePassword: (currentPassword: string, newPassword: string) =>
    call<User>("change_password", { current_password: currentPassword, new_password: newPassword }),

  // Users and settings (admin)
  listUsers: () => call<User[]>("list_users"),
  createUser: (input: NewUser) => call<User>("create_user", { input }),
  updateUser: (id: number, expectedVersion: number, input: UserUpdate) =>
    call<User>("update_user", { id, expected_version: expectedVersion, input }),
  resetPassword: (id: number, newPassword: string, mustChange: boolean) =>
    call<User>("reset_password", { id, new_password: newPassword, must_change: mustChange }),
  getSettings: () => call<AppSettings>("get_settings"),
  updateSettings: (settings: AppSettings) => call<AppSettings>("update_settings", { settings }),
  listAudit: (filter: Partial<AuditFilter> = {}) =>
    call<AuditEntry[]>("list_audit", {
      filter: { asset_id: null, entity: null, user_id: null, before_id: null, limit: null, ...filter },
    }),

  // Pick lists
  listLookups: (kind: LookupKind, includeInactive = false) =>
    call<LookupItem[]>("list_lookups", { kind, include_inactive: includeInactive }),
  createLookup: (kind: LookupKind, input: LookupInput) =>
    call<LookupItem>("create_lookup", { kind, input }),
  updateLookup: (kind: LookupKind, id: number, input: LookupInput) =>
    call<LookupItem>("update_lookup", { kind, id, input }),

  // Instruments and Systems
  listAssets: (filter: Partial<AssetFilter> = {}) =>
    call<Asset[]>("list_assets", {
      filter: { search: null, kind: null, parent_id: null, include_archived: false, ...filter },
    }),
  getAsset: (id: number) => call<Asset>("get_asset", { id }),
  createAsset: (input: AssetInput) => call<Asset>("create_asset", { input }),
  updateAsset: (id: number, expectedVersion: number, input: AssetInput, reason: string) =>
    call<Asset>("update_asset", { id, expected_version: expectedVersion, input, reason }),
  setAssetArchived: (id: number, expectedVersion: number, archived: boolean, reason: string) =>
    call<Asset>("set_asset_archived", { id, expected_version: expectedVersion, archived, reason }),

  // Calibration records
  listCalibrations: (filter: Partial<CalibrationFilter> = {}) =>
    call<CalibrationRecord[]>("list_calibrations", {
      filter: { asset_id: null, include_archived: false, ...filter },
    }),
  getCalibration: (id: number) => call<CalibrationRecord>("get_calibration", { id }),
  createCalibration: (input: CalibrationInput) =>
    call<CalibrationRecord>("create_calibration", { input }),
  updateCalibration: (id: number, expectedVersion: number, input: CalibrationInput, reason: string) =>
    call<CalibrationRecord>("update_calibration", {
      id,
      expected_version: expectedVersion,
      input,
      reason,
    }),
  setCalibrationArchived: (id: number, expectedVersion: number, archived: boolean, reason: string) =>
    call<CalibrationRecord>("set_calibration_archived", {
      id,
      expected_version: expectedVersion,
      archived,
      reason,
    }),

  dueSummary: () => call<DueSummary>("due_summary"),
};
