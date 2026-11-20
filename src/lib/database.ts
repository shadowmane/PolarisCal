// Helpers for the database connection and copy screens.

import type { ConnectionCheck } from "../bindings/ConnectionCheck";
import type { DatabaseConfig } from "../bindings/DatabaseConfig";
import type { Encryption } from "../bindings/Encryption";
import type { ServerConfig } from "../bindings/ServerConfig";

export type Backend = DatabaseConfig["backend"];
export type ServerBackend = Exclude<Backend, "sqlite">;

export const BACKENDS: { id: Backend; label: string; description: string }[] = [
  { id: "sqlite", label: "SQLite file", description: "A single file on this computer or a network share." },
  { id: "postgres", label: "PostgreSQL", description: "PostgreSQL 12 or later." },
  { id: "mysql", label: "MySQL or MariaDB", description: "MySQL 8 or MariaDB 10.5 or later." },
  { id: "sql_server", label: "Microsoft SQL Server", description: "SQL Server 2016 or later, or Azure SQL." },
];

export const DEFAULT_PORTS: Record<ServerBackend, number> = { postgres: 5432, mysql: 3306, sql_server: 1433 };

export const ENCRYPTION_OPTIONS: { id: Encryption; label: string; description: string }[] = [
  { id: "prefer", label: "When available", description: "Encrypt if the server supports it." },
  { id: "require", label: "Always", description: "Always encrypt; accept the server's certificate as is." },
  {
    id: "verify",
    label: "Always, with a trusted certificate",
    description: "Always encrypt, and only if this computer trusts the server's certificate.",
  },
  { id: "off", label: "Never", description: "Only for a trusted local network." },
];

/** Readable names for StoreInfo.backend values. */
export const STORE_BACKEND_NAMES: Record<string, string> = {
  sqlite: "SQLite file",
  postgres: "PostgreSQL",
  mysql: "MySQL / MariaDB",
  mssql: "Microsoft SQL Server",
};

export function isServer(config: DatabaseConfig): config is DatabaseConfig & ServerConfig & { backend: ServerBackend } {
  return config.backend !== "sqlite";
}

/** A blank configuration for `backend`, keeping what carries over from `from`. */
export function blankDatabase(backend: Backend, from?: DatabaseConfig): DatabaseConfig {
  if (backend === "sqlite") {
    return { backend: "sqlite", path: from?.backend === "sqlite" ? from.path : "" };
  }
  const previous = from && isServer(from) ? from : undefined;
  const auth =
    backend === "sql_server" && previous?.auth.method === "windows"
      ? previous.auth
      : { method: "password" as const, username: previous?.auth.method === "password" ? previous.auth.username : "", password: null };
  return {
    backend,
    host: previous?.host ?? "",
    port: null,
    instance: backend === "sql_server" ? (previous?.instance ?? null) : null,
    database: previous?.database ?? "polariscal",
    auth,
    encryption: previous?.encryption ?? "prefer",
  };
}

/** One sentence about what a connection test found. */
export function describeCheck(config: DatabaseConfig, check: ConnectionCheck): string {
  if (config.backend === "sqlite" && !check.exists) {
    return "The file doesn't exist yet. PolarisCal will create a new, empty database there when you save.";
  }
  const server = check.server_version ? `Connected to ${check.server_version}. ` : "Connected. ";
  if (check.schema_version == null) {
    return `${server}PolarisCal hasn't been set up in this database yet; it will be when you save.`;
  }
  const users = check.user_count ?? 0;
  if (users === 0) {
    return `${server}It holds a PolarisCal database with no user accounts yet.`;
  }
  return `${server}It holds a PolarisCal database with ${users} user account${users === 1 ? "" : "s"}.`;
}

/** The fields of a DatabaseConfig the backend reports validation errors for. */
export const DATABASE_FIELDS = ["path", "host", "port", "database", "username", "auth"] as const;
