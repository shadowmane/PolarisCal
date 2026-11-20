import { Card, ToggleButton, ToggleButtonGroup, useTheme } from "@heroui/react";
import { Monitor, Moon, Sun } from "lucide-react";
import type { ReactNode } from "react";
import { ErrorAlert, Loading, PageHeader } from "../components/common";
import { api } from "../lib/api";
import { useAsync } from "../lib/useAsync";

function Row({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="grid grid-cols-[180px_1fr] gap-4 py-2.5 text-sm">
      <dt className="text-muted">{label}</dt>
      <dd className="min-w-0 break-all">{children}</dd>
    </div>
  );
}

const BACKEND_NAMES: Record<string, string> = {
  sqlite: "SQLite (local file)",
  postgres: "PostgreSQL",
  mysql: "MySQL / MariaDB",
  mssql: "Microsoft SQL Server",
};

export function SettingsPage() {
  const { data, error, loading, reload } = useAsync(() => api.appInfo(), []);
  const { theme, setTheme } = useTheme("system");

  return (
    <>
      <PageHeader title="Settings" />
      {error && <ErrorAlert error={error} onRetry={reload} />}
      {loading && !data && <Loading />}

      <div className="flex max-w-3xl flex-col gap-6">
        {data && (
          <Card>
            <Card.Header>
              <Card.Title>Database</Card.Title>
              <Card.Description>
                Switching to a remote PostgreSQL, MySQL, or SQL Server database arrives in a later phase.
              </Card.Description>
            </Card.Header>
            <Card.Content>
              {data.connection_error && (
                <ErrorAlert error={{ kind: "not_connected", message: data.connection_error }} />
              )}
              <dl className="divide-y divide-separator">
                <Row label="Configured">{data.database}</Row>
                {data.store && (
                  <>
                    <Row label="Backend">{BACKEND_NAMES[data.store.backend] ?? data.store.backend}</Row>
                    <Row label="Location">{data.store.location}</Row>
                    <Row label="Schema version">{data.store.schema_version}</Row>
                    <Row label="Records">
                      {data.store.asset_count} items · {data.store.calibration_count} calibrations
                    </Row>
                  </>
                )}
                <Row label="Config file">{data.config_path}</Row>
              </dl>
            </Card.Content>
          </Card>
        )}

        <Card>
          <Card.Header>
            <Card.Title>Appearance</Card.Title>
          </Card.Header>
          <Card.Content>
            <ToggleButtonGroup
              selectionMode="single"
              disallowEmptySelection
              selectedKeys={[theme]}
              onSelectionChange={(keys) => {
                const [next] = [...keys];
                if (next) setTheme(String(next));
              }}
              aria-label="Theme"
            >
              <ToggleButton id="light">
                <Sun className="size-4" /> Light
              </ToggleButton>
              <ToggleButton id="dark">
                <Moon className="size-4" /> Dark
              </ToggleButton>
              <ToggleButton id="system">
                <Monitor className="size-4" /> System
              </ToggleButton>
            </ToggleButtonGroup>
          </Card.Content>
        </Card>

        {data && <p className="text-xs text-muted">PolarisCal version {data.version}</p>}
      </div>
    </>
  );
}
