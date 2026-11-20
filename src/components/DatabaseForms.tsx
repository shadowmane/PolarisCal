// Choosing the database this computer uses, and copying data to another database.

import { Alert, Button, Modal, toast } from "@heroui/react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { CheckCircle2, FolderOpen } from "lucide-react";
import { useEffect, useState } from "react";
import type { ConnectionChoice } from "../bindings/ConnectionChoice";
import type { CopyReport } from "../bindings/CopyReport";
import type { DatabaseConfig } from "../bindings/DatabaseConfig";
import type { SharedFilePreview } from "../bindings/SharedFilePreview";
import { ApiError, api } from "../lib/api";
import {
  BACKENDS,
  type Backend,
  DEFAULT_PORTS,
  ENCRYPTION_OPTIONS,
  blankDatabase,
  describeCheck,
  isServer,
} from "../lib/database";
import { type FieldErrors, fieldErrors } from "../lib/forms";
import { NumberInput, SelectInput, SwitchInput, TextInput } from "./fields";

const EXAMPLE_SHARED_FILE = String.raw`\\server\share\PolarisCal\polariscal-connection.json`;

function message(e: unknown): string {
  return e instanceof ApiError ? e.message : String(e);
}

function Notice({ status, title, children }: { status: "success" | "danger" | "warning"; title: string; children?: React.ReactNode }) {
  return (
    <Alert status={status}>
      <Alert.Indicator />
      <Alert.Content>
        <Alert.Title>{title}</Alert.Title>
        {children && <Alert.Description className="[overflow-wrap:anywhere]">{children}</Alert.Description>}
      </Alert.Content>
    </Alert>
  );
}

// ---------------------------------------------------------------------------
// The connection fields
// ---------------------------------------------------------------------------

/** Edits a database connection: a SQLite file or a server with its sign-in details. */
export function DatabaseFields({
  value,
  onChange,
  errors,
  hasSavedPassword = false,
}: {
  value: DatabaseConfig;
  onChange: (next: DatabaseConfig) => void;
  errors: FieldErrors;
  hasSavedPassword?: boolean;
}) {
  async function browseFile() {
    const picked = await openDialog({
      multiple: false,
      directory: false,
      title: "Choose a PolarisCal database file",
      filters: [{ name: "PolarisCal database", extensions: ["db", "sqlite", "sqlite3"] }],
    });
    if (typeof picked === "string") onChange({ backend: "sqlite", path: picked });
  }

  return (
    <div className="flex flex-col gap-4">
      <SelectInput<Backend>
        label="Database type"
        value={value.backend}
        onChange={(b) => b && b !== value.backend && onChange(blankDatabase(b, value))}
        options={BACKENDS}
        isRequired
      />

      {value.backend === "sqlite" ? (
        <div className="flex items-start gap-2">
          <TextInput
            label="Database file"
            value={value.path}
            onChange={(path) => onChange({ ...value, path })}
            error={errors.path}
            placeholder={String.raw`C:\PolarisCal\polariscal.db`}
            description="To start a new database, type the path of a file that doesn't exist yet."
            className="flex-1"
          />
          <Button className="mt-6" variant="secondary" onPress={() => void browseFile()}>
            <FolderOpen className="size-4" /> Browse
          </Button>
        </div>
      ) : (
        <ServerFields value={value} onChange={onChange} errors={errors} hasSavedPassword={hasSavedPassword} />
      )}
    </div>
  );
}

function ServerFields({
  value,
  onChange,
  errors,
  hasSavedPassword,
}: {
  value: DatabaseConfig;
  onChange: (next: DatabaseConfig) => void;
  errors: FieldErrors;
  hasSavedPassword: boolean;
}) {
  if (!isServer(value)) return null;
  const sqlServer = value.backend === "sql_server";
  const auth = value.auth;

  return (
    <>
      <div className="grid grid-cols-[1fr_140px] gap-3">
        <TextInput
          label="Server"
          value={value.host}
          onChange={(host) => onChange({ ...value, host })}
          error={errors.host}
          placeholder="db.example.local"
          isRequired
        />
        <NumberInput
          label="Port"
          value={value.port}
          onChange={(port) => onChange({ ...value, port: port == null ? null : Math.trunc(port) })}
          error={errors.port}
          minValue={1}
          step={1}
          formatOptions={{ useGrouping: false }}
          description={`Blank: ${value.instance ? "found automatically" : DEFAULT_PORTS[value.backend]}`}
        />
      </div>
      {sqlServer && (
        <TextInput
          label="Instance name"
          value={value.instance ?? ""}
          onChange={(instance) => onChange({ ...value, instance: instance || null })}
          placeholder="Leave blank for the default instance"
          description="For a named instance such as SQLEXPRESS. Without a port, SQL Server Browser finds it."
        />
      )}
      <TextInput
        label="Database name"
        value={value.database}
        onChange={(database) => onChange({ ...value, database })}
        error={errors.database}
        isRequired
        description="Create an empty database for PolarisCal first; PolarisCal sets up its tables."
      />

      {sqlServer ? (
        <SelectInput<"windows" | "password">
          label="Sign in with"
          value={auth.method}
          onChange={(m) =>
            m &&
            onChange({
              ...value,
              auth: m === "windows" ? { method: "windows" } : { method: "password", username: "", password: null },
            })
          }
          options={[
            {
              id: "windows",
              label: "Windows login",
              description: "Each person's own Windows account. No password is stored.",
            },
            { id: "password", label: "SQL Server account", description: "A username and password stored in the settings." },
          ]}
          error={errors.auth}
          isRequired
        />
      ) : (
        <p className="text-xs text-muted">
          {value.backend === "postgres"
            ? "PostgreSQL doesn't support Windows login from PolarisCal. Use a database account. Leave the password blank to use the one saved in this Windows user's pgpass.conf."
            : "MySQL and MariaDB don't support Windows login from PolarisCal. Use a database account."}
        </p>
      )}

      {auth.method === "password" && (
        <div className="grid grid-cols-2 gap-3">
          <TextInput
            label="Username"
            value={auth.username}
            onChange={(username) => onChange({ ...value, auth: { ...auth, username } })}
            error={errors.username}
            autoComplete="off"
            isRequired
          />
          <TextInput
            label="Password"
            type="password"
            value={auth.password ?? ""}
            onChange={(password) => onChange({ ...value, auth: { ...auth, password: password || null } })}
            placeholder={hasSavedPassword ? "Saved; leave blank to keep it" : undefined}
            autoComplete="new-password"
          />
        </div>
      )}

      <SelectInput
        label="Encryption"
        value={value.encryption}
        onChange={(encryption) => encryption && onChange({ ...value, encryption })}
        options={ENCRYPTION_OPTIONS}
        isRequired
      />
    </>
  );
}

// ---------------------------------------------------------------------------
// Changing the connection
// ---------------------------------------------------------------------------

type Mode = "local" | "shared";

/** Chooses which database this computer uses, directly or through a shared settings file. */
export function ConnectionDialog({
  isOpen,
  onOpenChange,
  onSaved,
}: {
  isOpen: boolean;
  onOpenChange: (open: boolean) => void;
  onSaved: () => void;
}) {
  const [mode, setMode] = useState<Mode>("local");
  const [database, setDatabase] = useState<DatabaseConfig>(blankDatabase("sqlite"));
  const [hasSavedPassword, setHasSavedPassword] = useState(false);
  const [sharedPath, setSharedPath] = useState("");
  const [preview, setPreview] = useState<SharedFilePreview>();
  const [editShared, setEditShared] = useState(false);
  const [errors, setErrors] = useState<FieldErrors>({});
  const [result, setResult] = useState<{ ok: boolean; text: string }>();
  const [busy, setBusy] = useState<"load" | "test" | "save" | null>(null);
  const [problem, setProblem] = useState<string>();

  useEffect(() => {
    if (!isOpen) return;
    setErrors({});
    setResult(undefined);
    setPreview(undefined);
    setEditShared(false);
    setBusy("load");
    api
      .getConnectionSettings()
      .then((s) => {
        setMode(s.shared_settings_file ? "shared" : "local");
        setSharedPath(s.shared_settings_file ?? "");
        if (s.database) setDatabase(s.database);
        setHasSavedPassword(s.has_password);
        setProblem(s.problem ?? undefined);
        if (s.shared_settings_file && s.database) {
          setPreview({ database: s.database, has_password: s.has_password, description: s.description });
        }
      })
      .catch((e) => setProblem(message(e)))
      .finally(() => setBusy(null));
  }, [isOpen]);

  function change(next: DatabaseConfig) {
    setDatabase(next);
    setErrors({});
    setResult(undefined);
  }

  function choice(): ConnectionChoice {
    if (mode === "local") return { source: "local", database };
    if (editShared) return { source: "write_shared_file", path: sharedPath, database };
    return { source: "shared_file", path: sharedPath };
  }

  function fail(e: unknown) {
    const fields = fieldErrors(e);
    setErrors(fields);
    if (Object.keys(fields).length === 0) setResult({ ok: false, text: message(e) });
  }

  async function loadShared(path = sharedPath) {
    setBusy("load");
    setResult(undefined);
    try {
      const p = await api.readSharedSettings(path);
      setPreview(p);
      setDatabase(p.database);
      setHasSavedPassword(p.has_password);
      setErrors({});
    } catch (e) {
      setPreview(undefined);
      fail(e);
    } finally {
      setBusy(null);
    }
  }

  async function browseShared() {
    const picked = await openDialog({
      multiple: false,
      directory: false,
      title: "Choose the shared settings file",
      filters: [{ name: "PolarisCal settings", extensions: ["json"] }],
    });
    if (typeof picked === "string") {
      setSharedPath(picked);
      await loadShared(picked);
    }
  }

  async function test() {
    setBusy("test");
    setResult(undefined);
    try {
      const check = await api.testConnection(choice());
      const tested = mode === "shared" && !editShared && preview ? preview.database : database;
      setResult({ ok: true, text: describeCheck(tested, check) });
    } catch (e) {
      fail(e);
    } finally {
      setBusy(null);
    }
  }

  async function save() {
    setBusy("save");
    setResult(undefined);
    try {
      const saved = await api.saveConnection(choice());
      toast.success(`Connected to ${saved.description}`);
      onOpenChange(false);
      onSaved();
    } catch (e) {
      fail(e);
    } finally {
      setBusy(null);
    }
  }

  const showFields = mode === "local" || editShared;
  const sharedReady = mode === "shared" && sharedPath.trim() !== "" && (editShared || !!preview);

  return (
    <Modal>
      <Modal.Backdrop isOpen={isOpen} onOpenChange={onOpenChange} isDismissable={busy !== "save"}>
        <Modal.Container size="lg" scroll="inside">
          <Modal.Dialog>
            <Modal.CloseTrigger />
            <Modal.Header>
              <Modal.Heading>Database connection</Modal.Heading>
            </Modal.Header>
            <Modal.Body>
              <form
                id="connection-form"
                noValidate
                className="flex flex-col gap-4 p-1"
                onSubmit={(e) => {
                  e.preventDefault();
                  void save();
                }}
              >
                {problem && <Notice status="warning" title="The current settings have a problem">{problem}</Notice>}

                <SelectInput<Mode>
                  label="Where the connection settings come from"
                  value={mode}
                  onChange={(m) => {
                    if (!m) return;
                    setMode(m);
                    setErrors({});
                    setResult(undefined);
                  }}
                  options={[
                    {
                      id: "local",
                      label: "This computer",
                      description: "Settings are kept on this computer only.",
                    },
                    {
                      id: "shared",
                      label: "A shared settings file",
                      description: "Every computer follows one file on a network share, so the connection is set up once.",
                    },
                  ]}
                  isRequired
                />

                {mode === "shared" && (
                  <>
                    <div className="flex items-start gap-2">
                      <TextInput
                        label="Shared settings file"
                        value={sharedPath}
                        onChange={(v) => {
                          setSharedPath(v);
                          setPreview(undefined);
                          setErrors({});
                          setResult(undefined);
                        }}
                        error={errors.shared_settings_file}
                        placeholder={EXAMPLE_SHARED_FILE}
                        className="flex-1"
                      />
                      <Button className="mt-6" variant="secondary" onPress={() => void browseShared()}>
                        <FolderOpen className="size-4" /> Browse
                      </Button>
                      {!editShared && (
                        <Button
                          className="mt-6"
                          variant="tertiary"
                          isDisabled={!sharedPath.trim()}
                          isPending={busy === "load"}
                          onPress={() => void loadShared()}
                        >
                          Read
                        </Button>
                      )}
                    </div>
                    {preview && !editShared && (
                      <p className="text-sm">
                        <span className="text-muted">This file connects to </span>
                        {preview.description}
                        {preview.has_password && <span className="text-muted"> (with a saved password)</span>}.
                      </p>
                    )}
                    <SwitchInput
                      label={preview ? "Change the connection in this file" : "Create this file with the connection below"}
                      description="Writes the file for every computer that follows it. A password entered below is stored in the file, so limit who can read the share."
                      isSelected={editShared}
                      onChange={(v) => {
                        setEditShared(v);
                        setResult(undefined);
                      }}
                    />
                  </>
                )}

                {showFields && (
                  <DatabaseFields value={database} onChange={change} errors={errors} hasSavedPassword={hasSavedPassword} />
                )}

                {result &&
                  (result.ok ? (
                    <Notice status="success" title="Connection works">
                      {result.text}
                    </Notice>
                  ) : (
                    <Notice status="danger" title="Couldn't connect">
                      {result.text}
                    </Notice>
                  ))}

                <p className="text-xs text-muted">
                  Saving connects to the database (setting PolarisCal up in it if it's new), then signs you out so you
                  can sign in there. Data isn't moved; use Copy data to another database for that.
                </p>
              </form>
            </Modal.Body>
            <Modal.Footer>
              <Button slot="close" variant="tertiary" isDisabled={busy === "save"}>
                Cancel
              </Button>
              <Button
                variant="secondary"
                isPending={busy === "test"}
                isDisabled={busy === "save" || (mode === "shared" && !sharedReady)}
                onPress={() => void test()}
              >
                Test connection
              </Button>
              <Button
                type="submit"
                form="connection-form"
                isPending={busy === "save"}
                isDisabled={busy === "test" || (mode === "shared" && !sharedReady)}
              >
                Save and connect
              </Button>
            </Modal.Footer>
          </Modal.Dialog>
        </Modal.Container>
      </Modal.Backdrop>
    </Modal>
  );
}

// ---------------------------------------------------------------------------
// Copying all data to another database
// ---------------------------------------------------------------------------

/** Copies everything to another, empty database and shows the verified counts. */
export function CopyDatabaseDialog({
  isOpen,
  onOpenChange,
  currentBackend,
  onSwitched,
}: {
  isOpen: boolean;
  onOpenChange: (open: boolean) => void;
  /** StoreInfo.backend of the database in use, to suggest the other kind. */
  currentBackend?: string;
  onSwitched: () => void;
}) {
  const [target, setTarget] = useState<DatabaseConfig>(blankDatabase("postgres"));
  const [errors, setErrors] = useState<FieldErrors>({});
  const [result, setResult] = useState<{ ok: boolean; text: string }>();
  const [report, setReport] = useState<CopyReport>();
  const [busy, setBusy] = useState<"test" | "copy" | "switch" | null>(null);

  useEffect(() => {
    if (!isOpen) return;
    setTarget(blankDatabase(currentBackend === "sqlite" ? "postgres" : "sqlite"));
    setErrors({});
    setResult(undefined);
    setReport(undefined);
  }, [isOpen, currentBackend]);

  function fail(e: unknown) {
    const fields = fieldErrors(e);
    setErrors(fields);
    if (Object.keys(fields).length === 0) setResult({ ok: false, text: message(e) });
  }

  async function test() {
    setBusy("test");
    setResult(undefined);
    try {
      const check = await api.testConnection({ source: "local", database: target });
      const inUse = (check.user_count ?? 0) > 0;
      setResult({
        ok: !inUse,
        text: inUse
          ? "This database already has PolarisCal data. Copy into a new, empty database instead."
          : describeCheck(target, check).replace("when you save", "when you copy"),
      });
    } catch (e) {
      fail(e);
    } finally {
      setBusy(null);
    }
  }

  async function copy() {
    setBusy("copy");
    setResult(undefined);
    try {
      setReport(await api.copyDatabase(target));
    } catch (e) {
      fail(e);
    } finally {
      setBusy(null);
    }
  }

  async function switchToTarget() {
    setBusy("switch");
    try {
      const saved = await api.saveConnection({ source: "local", database: target });
      toast.success(`This computer now uses ${saved.description}`);
      onOpenChange(false);
      onSwitched();
    } catch (e) {
      setResult({ ok: false, text: message(e) });
    } finally {
      setBusy(null);
    }
  }

  const total = report?.tables.reduce((sum, t) => sum + t.copied_rows, 0) ?? 0;

  return (
    <Modal>
      <Modal.Backdrop isOpen={isOpen} onOpenChange={onOpenChange} isDismissable={!busy}>
        <Modal.Container size="lg" scroll="inside">
          <Modal.Dialog>
            <Modal.CloseTrigger />
            <Modal.Header>
              <Modal.Heading>{report ? "Copy complete" : "Copy data to another database"}</Modal.Heading>
            </Modal.Header>
            <Modal.Body>
              {report ? (
                <div className="flex flex-col gap-4 p-1">
                  <Notice status="success" title={`Copied and verified ${total} rows`}>
                    From {report.source} to {report.target}. Every table's row count matches.
                  </Notice>
                  <table className="w-full text-sm">
                    <thead>
                      <tr className="border-b border-separator text-left text-muted">
                        <th className="py-1.5 font-medium">Table</th>
                        <th className="py-1.5 text-right font-medium">Source</th>
                        <th className="py-1.5 text-right font-medium">Copied</th>
                      </tr>
                    </thead>
                    <tbody>
                      {report.tables.map((t) => (
                        <tr key={t.table} className="border-b border-separator last:border-b-0">
                          <td className="py-1.5">{t.label}</td>
                          <td className="py-1.5 text-right tabular-nums">{t.source_rows}</td>
                          <td className="py-1.5 text-right tabular-nums">
                            <span className="inline-flex items-center gap-1">
                              {t.copied_rows}
                              <CheckCircle2 className="size-3.5 text-success" />
                            </span>
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                  <p className="text-sm text-muted">
                    This computer still uses the original database. Switch now, or later from Database connection.
                    Other computers need to be switched too; a shared settings file does that for everyone at once.
                  </p>
                  {result && !result.ok && <Notice status="danger" title="Couldn't switch">{result.text}</Notice>}
                </div>
              ) : (
                <form
                  id="copy-form"
                  noValidate
                  className="flex flex-col gap-4 p-1"
                  onSubmit={(e) => {
                    e.preventDefault();
                    void copy();
                  }}
                >
                  <p className="text-sm text-muted">
                    Copies every item, calibration record, file record, list, user account (with passwords), setting,
                    and the full audit trail, keeping their ids. The target must be empty: a new SQLite file, or an
                    empty database created on the server. Ask everyone to stop editing until the copy finishes.
                    Attached files stay in the attachments folder.
                  </p>
                  <DatabaseFields value={target} onChange={(t) => (setTarget(t), setErrors({}), setResult(undefined))} errors={errors} />
                  {result &&
                    (result.ok ? (
                      <Notice status="success" title="Ready to copy">
                        {result.text}
                      </Notice>
                    ) : (
                      <Notice status="danger" title="Can't copy there">
                        {result.text}
                      </Notice>
                    ))}
                </form>
              )}
            </Modal.Body>
            <Modal.Footer>
              {report ? (
                <>
                  <Button slot="close" variant="tertiary" isDisabled={!!busy}>
                    Keep using the original
                  </Button>
                  <Button isPending={busy === "switch"} onPress={() => void switchToTarget()}>
                    Switch this computer to the copy
                  </Button>
                </>
              ) : (
                <>
                  <Button slot="close" variant="tertiary" isDisabled={!!busy}>
                    Cancel
                  </Button>
                  <Button variant="secondary" isPending={busy === "test"} isDisabled={busy === "copy"} onPress={() => void test()}>
                    Test connection
                  </Button>
                  <Button type="submit" form="copy-form" isPending={busy === "copy"} isDisabled={busy === "test"}>
                    Copy data
                  </Button>
                </>
              )}
            </Modal.Footer>
          </Modal.Dialog>
        </Modal.Container>
      </Modal.Backdrop>
    </Modal>
  );
}
