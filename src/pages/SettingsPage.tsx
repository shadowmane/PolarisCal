import { Button, Card, toast, ToggleButton, ToggleButtonGroup, useTheme } from "@heroui/react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { ArrowDownCircle, ArrowRightLeft, Database, FolderOpen, Monitor, Moon, RefreshCw, Sun } from "lucide-react";
import type { AppSettings } from "../bindings/AppSettings";
import { type ReactNode, useEffect, useState } from "react";
import { ErrorAlert, Loading, PageHeader } from "../components/common";
import { ConnectionDialog, CopyDatabaseDialog } from "../components/DatabaseForms";
import { STORE_BACKEND_NAMES } from "../lib/database";
import { ApiError, api } from "../lib/api";
import { useAsync } from "../lib/useAsync";
import { NumberInput, SwitchInput, TextInput } from "../components/fields";
import { checkForUpdate, describeCheckError, installUpdate, useAvailableUpdate } from "../lib/updates";
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

function UpdatesCard({ version, settings, onSaved }: { version?: string; settings?: AppSettings; onSaved: () => void }) {
  const { can } = useAuth();
  const isAdmin = can("admin");
  const update = useAvailableUpdate();
  const [status, setStatus] = useState<{ ok: boolean; text: string }>();
  const [busy, setBusy] = useState<"check" | "install" | "toggle" | null>(null);
  const [progress, setProgress] = useState<number | null>(null);

  async function check() {
    setBusy("check");
    setStatus(undefined);
    try {
      const found = await checkForUpdate();
      setStatus({ ok: true, text: found ? `Version ${found.version} is available.` : "PolarisCal is up to date." });
    } catch (e) {
      setStatus({ ok: false, text: describeCheckError(e) });
    } finally {
      setBusy(null);
    }
  }

  async function install() {
    if (!update) return;
    setBusy("install");
    try {
      await installUpdate(update, setProgress);
    } catch (e) {
      setStatus({ ok: false, text: `The update couldn't be installed: ${e instanceof Error ? e.message : String(e)}` });
      setBusy(null);
    }
  }

  async function toggle(on: boolean) {
    if (!settings) return;
    setBusy("toggle");
    try {
      await api.updateSettings({ ...settings, update_checks: on });
      onSaved();
      toast.success(on ? "Automatic update checks turned on" : "Automatic update checks turned off");
    } catch (e) {
      toast.danger(e instanceof ApiError ? e.message : String(e));
    } finally {
      setBusy(null);
    }
  }

  return (
    <Card>
      <Card.Header>
        <Card.Title>Updates</Card.Title>
        <Card.Description>
          You're running PolarisCal {version ?? ""}. New versions are signed, so only official releases install.
        </Card.Description>
      </Card.Header>
      <Card.Content className="flex flex-col gap-4">
        {update && (
          <div className="rounded-xl border border-separator p-4">
            <div className="flex items-center justify-between gap-4">
              <div>
                <div className="font-medium">Version {update.version} is available</div>
                {update.date && <div className="text-xs text-muted">Released {update.date.slice(0, 10)}</div>}
              </div>
              <Button isPending={busy === "install"} onPress={() => void install()}>
                <ArrowDownCircle className="size-4" /> Install and restart
              </Button>
            </div>
            {update.body && <p className="mt-3 text-sm whitespace-pre-line text-muted">{update.body}</p>}
            {busy === "install" && (
              <p className="mt-3 text-sm text-muted">
                {progress == null ? "Downloading…" : progress < 1 ? `Downloading… ${Math.round(progress * 100)}%` : "Installing…"}
              </p>
            )}
          </div>
        )}
        {status && (
          <p className={`text-sm ${status.ok ? "" : "text-danger"}`}>{status.text}</p>
        )}
        <div>
          <Button variant="secondary" isPending={busy === "check"} isDisabled={busy === "install"} onPress={() => void check()}>
            <RefreshCw className="size-4" /> Check for updates
          </Button>
        </div>
        {isAdmin && settings && (
          <SwitchInput
            label="Check for updates automatically"
            description="Each computer looks for a new version when PolarisCal starts. Turn this off if IT deploys updates instead. Applies to everyone."
            isSelected={settings.update_checks}
            isDisabled={busy === "toggle"}
            onChange={(on) => void toggle(on)}
          />
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
        <UpdatesCard version={data?.version} settings={settings.data} onSaved={settings.reload} />

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

      </div>
    </>
  );
}
