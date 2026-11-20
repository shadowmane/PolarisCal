import { describe, expect, it } from "vitest";
import { blankDatabase, describeCheck } from "./database";

describe("blankDatabase", () => {
  it("starts a server connection with sensible defaults", () => {
    expect(blankDatabase("postgres")).toEqual({
      backend: "postgres",
      host: "",
      port: null,
      instance: null,
      database: "polariscal",
      auth: { method: "password", username: "", password: null },
      encryption: "prefer",
    });
  });

  it("keeps the server, database, and account when switching server type, but not the port", () => {
    const pg = { ...blankDatabase("postgres"), host: "db1", port: 6543, auth: { method: "password" as const, username: "cal", password: "x" } };
    const mssql = blankDatabase("sql_server", pg);
    expect(mssql).toMatchObject({ backend: "sql_server", host: "db1", port: null, auth: { method: "password", username: "cal", password: null } });
  });

  it("drops Windows sign-in when leaving SQL Server", () => {
    const mssql = { ...blankDatabase("sql_server"), auth: { method: "windows" as const } };
    expect(blankDatabase("mysql", mssql)).toMatchObject({ auth: { method: "password", username: "" } });
    expect(blankDatabase("sql_server", mssql)).toMatchObject({ auth: { method: "windows" } });
  });

  it("keeps a SQLite path only when staying on SQLite", () => {
    expect(blankDatabase("sqlite", { backend: "sqlite", path: "C:\\a.db" })).toEqual({ backend: "sqlite", path: "C:\\a.db" });
    expect(blankDatabase("sqlite", blankDatabase("postgres"))).toEqual({ backend: "sqlite", path: "" });
  });
});

describe("describeCheck", () => {
  const pg = blankDatabase("postgres");
  const check = { server_version: "PostgreSQL 17.2", schema_version: 5, user_count: 3, exists: true };

  it("explains a new SQLite file", () => {
    expect(describeCheck({ backend: "sqlite", path: "x.db" }, { ...check, exists: false })).toMatch(/doesn't exist yet/);
  });

  it("explains an empty server database", () => {
    expect(describeCheck(pg, { ...check, schema_version: null, user_count: null })).toBe(
      "Connected to PostgreSQL 17.2. PolarisCal hasn't been set up in this database yet; it will be when you save.",
    );
  });

  it("counts existing accounts", () => {
    expect(describeCheck(pg, check)).toMatch(/with 3 user accounts\.$/);
    expect(describeCheck(pg, { ...check, user_count: 1 })).toMatch(/with 1 user account\.$/);
  });
});
