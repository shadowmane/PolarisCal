import { Button, Card, toast, ToggleButton, ToggleButtonGroup, useTheme } from "@heroui/react";
import { Monitor, Moon, Sun } from "lucide-react";
import { type ReactNode, useEffect, useState } from "react";
import { ErrorAlert, Loading, PageHeader } from "../components/common";
import { ApiError, api } from "../lib/api";
import { useAsync } from "../lib/useAsync";
import { NumberInput } from "../components/fields";
import { useAuth } from "../lib/auth";
import { fieldErrors } from "../lib/forms";
import { roleMeta } from "../lib/permissions";

function SecurityCard() {
  const { can, idleLockMinutes, refresh, user } = useAuth();
  const isAdmin = can("admin");
  const [minutes, setMinutes] = useState<number | null>(idleLockMinutes);
  const [error, setError] = useState<string>();
  const [busy, setBusy] = useState(false);
  useEffect(() => setMinutes(idleLockMinutes), [idleLockMinutes]);

  async function save() {
    if (minutes == null) return setError("Enter a number of minutes.");
    setBusy(true);
    try {
      await api.updateSettings({ idle_lock_minutes: minutes });
      await refresh();
      setError(undefined);
      toast.success("Security settings saved");
    } catch (e) {
      const fields = fieldErrors(e);
      setError(fields.idle_lock_minutes ?? (e instanceof ApiError ? e.message : String(e)));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Card>
      <Card.Header>
        <Card.Title>Security</Card.Title>
        <Card.Description>
          Signed in as {user.display_name} ({user.username}), {roleMeta[user.role].label}.
        </Card.Description>
      </Card.Header>
      <Card.Content>
        {isAdmin ? (
          <div className="flex items-start gap-3">
            <NumberInput
              label="Auto-lock after (minutes)"
              value={minutes}
              onChange={(v) => {
                setMinutes(v == null ? null : Math.trunc(v));
                setError(undefined);
              }}
              minValue={1}
              error={error}
              description="PolarisCal locks after this long without keyboard or mouse activity. Applies to everyone."
              className="max-w-xs"
            />
            <Button className="mt-6" variant="secondary" isPending={busy} onPress={() => void save()}>
              Save
            </Button>
          </div>
        ) : (
          <p className="text-sm text-muted">
            PolarisCal locks after {idleLockMinutes} minutes without activity. An administrator can change this.
          </p>
        )}
      </Card.Content>
    </Card>
  );
}

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

        <SecurityCard />

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
