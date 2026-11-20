import { describe, expect, it } from "vitest";
import { can } from "./permissions";

describe("permissions", () => {
  it("lets admins do everything", () => {
    expect(can("admin", "admin")).toBe(true);
    expect(can("admin", "edit_records")).toBe(true);
  });

  it("lets technicians edit records but not administer", () => {
    expect(can("technician", "edit_records")).toBe(true);
    expect(can("technician", "admin")).toBe(false);
  });

  it("keeps viewers read-only", () => {
    expect(can("viewer", "edit_records")).toBe(false);
    expect(can("viewer", "admin")).toBe(false);
  });

  it("denies everything without a role", () => {
    expect(can(null, "edit_records")).toBe(false);
    expect(can(undefined, "admin")).toBe(false);
  });
});
