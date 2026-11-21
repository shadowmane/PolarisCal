import type { Role } from "../bindings/Role";

/**
 * What the UI should offer a role. Mirrors `Role::can_edit_records` / `Role::is_admin`
 * in Rust; the backend enforces the same rules on every command.
 */
export type Permission =
  /** Create, edit, and archive Instruments, Systems, and Service Records; add list entries. */
  | "edit_records"
  /** Users, pick lists, settings, and the full audit log. */
  | "admin";

export function can(role: Role | null | undefined, permission: Permission): boolean {
  switch (permission) {
    case "edit_records":
      return role === "admin" || role === "technician";
    case "admin":
      return role === "admin";
  }
}

export const roleMeta: Record<Role, { label: string; description: string }> = {
  admin: { label: "Administrator", description: "Everything, including users, lists, and settings." },
  technician: {
    label: "Technician",
    description: "Create, edit, and archive instruments, systems, and service records, and add list entries.",
  },
  viewer: { label: "Viewer", description: "Read-only access." },
};

export const ROLES: Role[] = ["admin", "technician", "viewer"];
