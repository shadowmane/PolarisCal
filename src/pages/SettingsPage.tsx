import { Button, Card, toast, ToggleButton, ToggleButtonGroup, useTheme } from "@heroui/react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { ArrowRightLeft, Database, FolderOpen, Monitor, Moon, Sun } from "lucide-react";
import type { AppSettings } from "../bindings/AppSettings";
import { type ReactNode, useEffect, useState } from "react";
import { ErrorAlert, Loading, PageHeader } from "../components/common";
import { ConnectionDialog, CopyDatabaseDialog } from "../components/DatabaseForms";
import { STORE_BACKEND_NAMES } from "../lib/database";
import { ApiError, api } from "../lib/api";
import { useAsync } from "../lib/useAsync";
import { NumberInput, TextInput } from "../components/fields";
import { useAuth } from "../lib/auth";
import { fieldErrors } from "../lib/forms";
import { roleMeta } from "../lib/permissions";

function SecurityCard({ settings, onSaved }: { settings?: AppSettings; onSaved: () => void }) {
  const { can, idleLockMinutes, refresh, user } = useAuth();
  const isAdmin = can("admin");
  const [minutes, setMinutes] = useState<number | null>(idleLockMinutes);
  const [error, setError] = useState<string>();
  const [busy, setBusy] = useState(false);
  useEffect(() => setMinutes(idleLockMinutes), [idleLockMinutes]);

  async function save() {
    if (minutes == null) return setError("Enter a number of minutes.");
    if (!settings) return;
    setBusy(true);
    try {
      await api.updateSettings({ ...settings, idle_lock_minutes: minutes });
      await refresh();
      onSaved();
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

const EXAMPLE_SHARE = String.raw`\\server\share\PolarisCal`;

function AttachmentsFolderCard({ settings, onSaved }: { settings?: AppSettings; onSaved: () => void }) {
  const { can } = useAuth();
  const isAdmin = can("admin");
  const [folder, setFolder] = useState("");
  const [error, setError] = useState<string>();
  const [busy, setBusy] = useState<"test" | "save" | null>(null);
  useEffect(() => setFolder(settings?.attachments_root ?? ""), [settings]);

  async function browse() {
    const picked = await openDialog({
      directory: true,
      multiple: false,
      title: "Choose the attachments folder",
      defaultPath: folder || undefined,
    });
    if (typeof picked === "string") {
      setFolder(picked);
      setError(undefined);
    }
  }

  async function test() {
    setBusy("test");
    try {
      await api.checkAttachmentsFolder(folder);
      setError(undefined);
      toast.success("PolarisCal can write to this folder");
    } catch (e) {
      setError(e instanceof ApiError ? e.message : String(e));
    } finally {
      setBusy(null);
    }
  }

  async function save() {
    if (!settings) return;
    setBusy("save");
    try {
      await api.updateSettings({ ...settings, attachments_root: folder.trim() || null });
      setError(undefined);
      toast.success(folder.trim() ? "Attachments folder saved" : "Attachments folder cleared");
      onSaved();
    } catch (e) {
      const fields = fieldErrors(e);
      setError(fields.attachments_root ?? (e instanceof ApiError ? e.message : String(e)));
    } finally {
      setBusy(null);
    }
  }

  return (
    <Card>
      <Card.Header>
        <Card.Title>Attachments folder</Card.Title>
        <Card.Description>
          Certificates and other files are copied here, in a folder per item. Use a network share (for example
          {EXAMPLE_SHARE}) that everyone using PolarisCal can reach with the same path.
        </Card.Description>
      </Card.Header>
      <Card.Content>
        {isAdmin ? (
          <div className="flex flex-col gap-3">
            <div className="flex items-start gap-2">
              <TextInput
                label="Folder"
                value={folder}
                onChange={(v) => {
                  setFolder(v);
                  setError(undefined);
                }}
                error={error}
                placeholder={EXAMPLE_SHARE}
                className="flex-1"
              />
              <Button className="mt-6" variant="secondary" onPress={() => void browse()}>
                <FolderOpen className="size-4" /> Browse
              </Button>
            </div>
            <div className="flex gap-2">
              <Button
                variant="tertiary"
                isDisabled={!folder.trim()}
                isPending={busy === "test"}
                onPress={() => void test()}
              >
                Test access
              </Button>
              <Button isPending={busy === "save"} onPress={() => void save()}>
                Save
              </Button>
            </div>
            <p className="text-xs text-muted">
              Changing the folder doesn't move existing files. Copy them to the new location yourself, keeping
              the same folder structure.
            </p>
          </div>
        ) : (
          <p className="text-sm break-all">
            {settings?.attachments_root ?? (
              <span className="text-muted">Not set. An administrator needs to choose a folder before files can be attached.</span>
            )}
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

export function SettingsPage() {
  const { data, error, loading, reload } = useAsync(() => api.appInfo(), []);
  const { theme, setTheme } = useTheme("system");
  const settings = useAsync(() => api.getSettings(), []);
  const { can, refresh } = useAuth();
  const isAdmin = can("admin");
  const [dialog, setDialog] = useState<"connection" | "copy" | null>(null);
  const connection = useAsync(() => (isAdmin ? api.getConnectionSettings() : Promise.resolve(null)), [isAdmin]);

  return (
    <>
      <PageHeader title="Settings" />
      {error && <ErrorAlert error={error} onRetry={reload} />}
      {loading && !data && <Loading />}

      <div className="flex max-w-3xl flex-col gap-6">
        {data && (
          <Card>
            <Card.Header>
              <Card.Title>Database connection</Card.Title>
              <Card.Description>
                {isAdmin
                  ? "Where this computer's PolarisCal keeps its data: a SQLite file, or a PostgreSQL, MySQL/MariaDB, or SQL Server database."
                  : "Where PolarisCal keeps its data. An administrator can change this."}
              </Card.Description>
            </Card.Header>
            <Card.Content>
              {data.connection_error && (
                <ErrorAlert error={{ kind: "not_connected", message: data.connection_error }} />
              )}
              <dl className="divide-y divide-separator">
                <Row label="Connected to">{data.database}</Row>
                {data.store && (
                  <>
                    <Row label="Database type">{STORE_BACKEND_NAMES[data.store.backend] ?? data.store.backend}</Row>
                    <Row label="Schema version">{data.store.schema_version}</Row>
                    <Row label="Records">
                      {data.store.asset_count} items · {data.store.calibration_count} calibrations
                    </Row>
                  </>
                )}
                {isAdmin && (
                  <Row label="Settings from">
                    {connection.data?.shared_settings_file ? (
                      <>
                        Shared settings file <span className="text-muted">{connection.data.shared_settings_file}</span>
                      </>
                    ) : (
                      "This computer"
                    )}
                  </Row>
                )}
                <Row label="Config file">{data.config_path}</Row>
              </dl>
            </Card.Content>
            {isAdmin && (
              <Card.Footer className="flex gap-2">
                <Button variant="secondary" onPress={() => setDialog("connection")}>
                  <Database className="size-4" /> Change connection…
                </Button>
              </Card.Footer>
            )}
          </Card>
        )}

        {isAdmin && data?.store && (
          <Card>
            <Card.Header>
              <Card.Title>Copy data to another database</Card.Title>
              <Card.Description>
                Moves everything, including users and the audit trail, into a new empty database: for example from
                this SQLite file to a SQL Server, or from a server back to a SQLite file. Row counts are checked
                afterwards, and the copy is recorded in both audit trails.
              </Card.Description>
            </Card.Header>
            <Card.Footer>
              <Button variant="secondary" onPress={() => setDialog("copy")}>
                <ArrowRightLeft className="size-4" /> Copy data…
              </Button>
            </Card.Footer>
          </Card>
        )}

        <ConnectionDialog
          isOpen={dialog === "connection"}
          onOpenChange={(open) => setDialog(open ? "connection" : null)}
          onSaved={() => void refresh()}
        />
        <CopyDatabaseDialog
          isOpen={dialog === "copy"}
          onOpenChange={(open) => setDialog(open ? "copy" : null)}
          currentBackend={data?.store?.backend}
          onSwitched={() => void refresh()}
        />

        <SecurityCard settings={settings.data} onSaved={settings.reload} />
        <AttachmentsFolderCard settings={settings.data} onSaved={settings.reload} />

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
