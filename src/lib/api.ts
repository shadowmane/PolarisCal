// Typed wrappers around the Rust commands in src-tauri/src/commands.rs.
// Types come from src/bindings, generated from the Rust models by ts-rs (`cargo test`).

import { invoke, isTauri } from "@tauri-apps/api/core";
import type { AppInfo } from "../bindings/AppInfo";
import type { AppSettings } from "../bindings/AppSettings";
import type { Asset } from "../bindings/Asset";
import type { AssetFilter } from "../bindings/AssetFilter";
import type { AssetInput } from "../bindings/AssetInput";
import type { Attachment } from "../bindings/Attachment";
import type { AttachmentCategory } from "../bindings/AttachmentCategory";
import type { AttachmentFilter } from "../bindings/AttachmentFilter";
import type { AuditEntry } from "../bindings/AuditEntry";
import type { AuditFilter } from "../bindings/AuditFilter";
import type { AuthStatus } from "../bindings/AuthStatus";
import type { CalibrationFilter } from "../bindings/CalibrationFilter";
import type { CalibrationInput } from "../bindings/CalibrationInput";
import type { CalibrationRecord } from "../bindings/CalibrationRecord";
import type { Certificate } from "../bindings/Certificate";
import type { CertificateDetails } from "../bindings/CertificateDetails";
import type { CertificateFilter } from "../bindings/CertificateFilter";
import type { CertificatePaper } from "../bindings/CertificatePaper";
import type { CertificateSigner } from "../bindings/CertificateSigner";
import type { CertificateSummary } from "../bindings/CertificateSummary";
import type { CertificateTemplate } from "../bindings/CertificateTemplate";
import type { CommandError } from "../bindings/CommandError";
import type { ConnectionCheck } from "../bindings/ConnectionCheck";
import type { ConnectionChoice } from "../bindings/ConnectionChoice";
import type { ConnectionSettings } from "../bindings/ConnectionSettings";
import type { ConnectionStatus } from "../bindings/ConnectionStatus";
import type { CopyReport } from "../bindings/CopyReport";
import type { DatabaseConfig } from "../bindings/DatabaseConfig";
import type { DueSummary } from "../bindings/DueSummary";
import type { ErrorKind } from "../bindings/ErrorKind";
import type { ExportSheet } from "../bindings/ExportSheet";
import type { FieldError } from "../bindings/FieldError";
import type { LastCost } from "../bindings/LastCost";
import type { LookupInput } from "../bindings/LookupInput";
import type { LookupItem } from "../bindings/LookupItem";
import type { LookupKind } from "../bindings/LookupKind";
import type { NewUser } from "../bindings/NewUser";
import type { SharedFilePreview } from "../bindings/SharedFilePreview";
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
const SESSION_COMMANDS = new Set(["sign_in", "unlock_session", "setup_admin", "unlock_connection_settings"]);

export const api = {
  appInfo: () => call<AppInfo>("app_info"),

  // Session
  authStatus: () => call<AuthStatus>("auth_status"),
  setupAdmin: (input: NewUser) => call<User>("setup_admin", { input }),
  signIn: (username: string, password: string) => call<User>("sign_in", { username, password }),
  /** From the sign-in screen; succeeds whether or not the account exists. */
  requestPasswordReset: (username: string) => call<void>("request_password_reset", { username }),
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
  /** Accounts waiting for a password reset. Checked in the background: not activity. */
  passwordResetRequests: () => call<number>("password_reset_requests"),
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
  deleteLookup: (kind: LookupKind, id: number) => call<void>("delete_lookup", { kind, id }),

  // Instruments and Systems
  listAssets: (filter: Partial<AssetFilter> = {}) =>
    call<Asset[]>("list_assets", {
      filter: { search: null, kind: null, parent_id: null, include_archived: false, ...filter },
    }),
  getAsset: (id: number) => call<Asset>("get_asset", { id }),
  createAsset: (input: AssetInput) => call<Asset>("create_asset", { input }),
  updateAsset: (id: number, expectedVersion: number, input: AssetInput, reason: string) =>
    call<Asset>("update_asset", { id, expected_version: expectedVersion, input, reason }),
  addToSystem: (systemId: number, instrumentIds: number[]) =>
    call<void>("add_to_system", { system_id: systemId, instrument_ids: instrumentIds }),
  setAssetArchived: (id: number, expectedVersion: number, archived: boolean, reason: string) =>
    call<Asset>("set_asset_archived", { id, expected_version: expectedVersion, archived, reason }),

  // Calibration records
  listCalibrations: (filter: Partial<CalibrationFilter> = {}) =>
    call<CalibrationRecord[]>("list_calibrations", {
      filter: { asset_id: null, include_archived: false, ...filter },
    }),
  getCalibration: (id: number) => call<CalibrationRecord>("get_calibration", { id }),
  /** What a provider last charged for a service type on the same make and model as the item. */
  lastCost: (providerId: number, serviceTypeId: number, assetId: number) =>
    call<LastCost | null>("last_cost", { provider_id: providerId, service_type_id: serviceTypeId, asset_id: assetId }),
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

  // Attachments
  listAttachments: (filter: Partial<AttachmentFilter> = {}) =>
    call<Attachment[]>("list_attachments", {
      filter: { asset_id: null, calibration_id: null, include_archived: false, include_shared_manuals: false, ...filter },
    }),
  addAttachments: (args: {
    assetId: number;
    calibrationId?: number | null;
    category: AttachmentCategory;
    paths: string[];
    description?: string | null;
  }) =>
    call<Attachment[]>("add_attachments", {
      asset_id: args.assetId,
      calibration_id: args.calibrationId ?? null,
      category: args.category,
      paths: args.paths,
      description: args.description ?? null,
    }),
  openAttachment: (id: number, force = false) => call<void>("open_attachment", { id, force }),
  revealAttachment: (id: number) => call<void>("reveal_attachment", { id }),
  /** An attached photo's image bytes, for showing it on the page. */
  attachmentImage: (id: number) => call<ArrayBuffer>("attachment_image", { id }),
  setAttachmentArchived: (id: number, expectedVersion: number, archived: boolean, reason: string) =>
    call<Attachment>("set_attachment_archived", { id, expected_version: expectedVersion, archived, reason }),
  checkAttachmentsFolder: (path: string) => call<void>("check_attachments_folder", { path }),

  // Database connection (this computer) and copying data between databases
  getConnectionSettings: () => call<ConnectionSettings>("get_connection_settings"),
  /** Green, yellow, or red, with what's connected. Works without signing in. */
  connectionStatus: () => call<ConnectionStatus>("connection_status"),
  /** Tries the configured database again if it couldn't be opened. */
  retryConnection: () => call<ConnectionStatus>("retry_connection"),
  /** An administrator of the current database unlocks the connection settings for a while. */
  unlockConnectionSettings: (username: string, password: string) =>
    call<void>("unlock_connection_settings", { username, password }),
  readSharedSettings: (path: string) => call<SharedFilePreview>("read_shared_settings", { path }),
  testConnection: (choice: ConnectionChoice) => call<ConnectionCheck>("test_connection", { choice }),
  saveConnection: (choice: ConnectionChoice) => call<ConnectionSettings>("save_connection", { choice }),
  copyDatabase: (target: DatabaseConfig) => call<CopyReport>("copy_database", { target }),

  // Reports
  exportXlsx: (path: string, sheets: ExportSheet[]) => call<void>("export_xlsx", { path, sheets }),

  // Certificates
  getCertificateTemplate: () => call<CertificateTemplate>("get_certificate_template"),
  updateCertificateTemplate: (template: CertificateTemplate) =>
    call<CertificateTemplate>("update_certificate_template", { template }),
  listCertificates: (filter: Partial<CertificateFilter> = {}) =>
    call<CertificateSummary[]>("list_certificates", { filter: { search: null, asset_id: null, ...filter } }),
  getCertificate: (id: number) => call<Certificate>("get_certificate", { id }),
  createCertificate: (details: CertificateDetails) => call<Certificate>("create_certificate", { details }),
  /** Reads a logo or signature image file as a data URL. */
  readImageFile: (path: string) => call<string>("read_image_file", { path }),
  listCertificateSigners: () => call<CertificateSigner[]>("list_certificate_signers"),
  getUserSignature: (userId: number) => call<string | null>("get_user_signature", { user_id: userId }),
  setUserSignature: (userId: number, signature: string | null) =>
    call<void>("set_user_signature", { user_id: userId, signature }),
  /**
   * Writes what the window shows for printing (the certificate) to a PDF, appends the
   * test data PDF if given, then opens the file if asked.
   */
  saveCertificatePdf: (path: string, paper: CertificatePaper, testData: string | null = null, open = true) =>
    call<void>("save_certificate_pdf", { path, paper, test_data: testData, open }),
};
