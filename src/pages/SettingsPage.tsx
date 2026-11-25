import { Button, Card, Tabs, toast, ToggleButton, ToggleButtonGroup } from "@heroui/react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { ArrowDownCircle, ArrowRightLeft, Database, FolderOpen, Monitor, Moon, RefreshCw, Sun } from "lucide-react";
import type { AppSettings } from "../bindings/AppSettings";
import type { PasswordPolicy } from "../bindings/PasswordPolicy";
import { type ReactNode, useEffect, useState } from "react";
import { useSearchParams } from "react-router";
import { ErrorAlert, Loading, LockableTitle, PageHeader } from "../components/common";
import { CertificateTemplateSettings } from "../components/CertificateTemplateSettings";
import { ConnectionDialog, CopyDatabaseDialog } from "../components/DatabaseForms";
import { STORE_BACKEND_NAMES } from "../lib/database";
import { ApiError, api } from "../lib/api";
import { useAsync } from "../lib/useAsync";
import { NumberInput, SelectInput, SwitchInput, TextInput } from "../components/fields";
import { CURRENCY_SYMBOLS, setCurrencySymbol } from "../lib/currency";
import { DATE_FORMATS, type DateFormatId, formatYmd, setDateFormat } from "../lib/dates";
import { checkForUpdate, describeCheckError, installUpdate, useAvailableUpdate } from "../lib/updates";
import { useAuth } from "../lib/auth";
import { fieldErrors } from "../lib/forms";
import { describePasswordRules } from "../lib/password";
import { roleMeta } from "../lib/permissions";
import { type ThemeChoice, useTheme } from "../lib/theme";

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
        <LockableTitle locked={!isAdmin}>Security</LockableTitle>
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

const MIN_LENGTH_RANGE = [8, 128] as const;
const EXPIRY_DAYS_RANGE = [1, 3650] as const;
const DEFAULT_EXPIRY_DAYS = 90;

function PasswordRulesCard({ settings, onSaved }: { settings?: AppSettings; onSaved: () => void }) {
  const { can, refresh } = useAuth();
  const isAdmin = can("admin");
  const saved = settings?.password_policy;
  const [form, setForm] = useState<PasswordPolicy | null>(null);
  // Remembered so turning expiry off and on again keeps the number typed.
  const [days, setDays] = useState<number | null>(DEFAULT_EXPIRY_DAYS);
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    if (!saved) return;
    setForm(saved);
    setDays(saved.expiry_days ?? DEFAULT_EXPIRY_DAYS);
  }, [saved]);

  const set = <K extends keyof PasswordPolicy>(key: K, value: PasswordPolicy[K]) => {
    setForm((f) => (f ? { ...f, [key]: value } : f));
    setErrors({});
  };
  const dirty = !!form && !!saved && JSON.stringify(form) !== JSON.stringify(saved);

  async function save() {
    if (!form || !settings) return;
    const local: Record<string, string> = {};
    const [minLen, maxLen] = MIN_LENGTH_RANGE;
    if (!(form.min_length >= minLen && form.min_length <= maxLen)) {
      local.password_min_length = `Enter a length from ${minLen} to ${maxLen}.`;
    }
    const [minDays, maxDays] = EXPIRY_DAYS_RANGE;
    if (form.expiry_days != null && !(form.expiry_days >= minDays && form.expiry_days <= maxDays)) {
      local.password_expiry_days = `Enter ${minDays} to ${maxDays} days.`;
    }
    if (Object.keys(local).length) return setErrors(local);
    setBusy(true);
    try {
      await api.updateSettings({ ...settings, password_policy: form });
      await refresh();
      onSaved();
      toast.success("Password rules saved");
    } catch (e) {
      const fields = fieldErrors(e);
      if (Object.keys(fields).length) setErrors(fields);
      else toast.danger(e instanceof ApiError ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  const expires = saved?.expiry_days;
  return (
    <Card>
      <Card.Header>
        <LockableTitle locked={!isAdmin}>Password rules</LockableTitle>
        <Card.Description>
          What new passwords must contain, and how long a password lasts. Existing passwords keep working;
          the rules apply to the next one each person chooses, including when theirs expires.
        </Card.Description>
      </Card.Header>
      <Card.Content>
        {!isAdmin || !form ? (
          <p className="text-sm text-muted">
            {saved ? (
              <>
                {describePasswordRules(saved)}{" "}
                {expires ? `Passwords expire after ${expires} days.` : "Passwords don't expire."}
                {!isAdmin && " An administrator can change this."}
              </>
            ) : (
              "Loading…"
            )}
          </p>
        ) : (
          <div className="flex flex-col gap-4">
            <NumberInput
              label="Minimum length (characters)"
              value={form.min_length}
              onChange={(v) => set("min_length", v == null ? 0 : Math.trunc(v))}
              minValue={MIN_LENGTH_RANGE[0]}
              error={errors.password_min_length}
              className="max-w-xs"
            />
            <SwitchInput
              label="Require a capital letter"
              isSelected={form.require_uppercase}
              onChange={(v) => set("require_uppercase", v)}
            />
            <SwitchInput
              label="Require a number"
              isSelected={form.require_number}
              onChange={(v) => set("require_number", v)}
            />
            <SwitchInput
              label="Require a special character"
              description="Anything other than a letter, number, or space, such as ! # $ or -."
              isSelected={form.require_symbol}
              onChange={(v) => set("require_symbol", v)}
            />
            <SwitchInput
              label="Passwords expire"
              description="When a password is older than this, its owner must choose a new one the next time they sign in."
              isSelected={form.expiry_days != null}
              onChange={(on) => set("expiry_days", on ? (days ?? DEFAULT_EXPIRY_DAYS) : null)}
            />
            {form.expiry_days != null && (
              <NumberInput
                label="Expire after (days)"
                value={form.expiry_days}
                onChange={(v) => {
                  const n = v == null ? null : Math.trunc(v);
                  setDays(n);
                  set("expiry_days", n ?? 0);
                }}
                minValue={EXPIRY_DAYS_RANGE[0]}
                error={errors.password_expiry_days}
                className="max-w-xs"
              />
            )}
          </div>
        )}
      </Card.Content>
      {isAdmin && form && (
        <Card.Footer className="flex gap-2">
          <Button variant="secondary" isPending={busy} isDisabled={!dirty} onPress={() => void save()}>
            Save
          </Button>
          {dirty && (
            <Button variant="tertiary" isDisabled={busy} onPress={() => saved && setForm(saved)}>
              Discard changes
            </Button>
          )}
        </Card.Footer>
      )}
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

function CurrencyCard({ settings, onSaved }: { settings?: AppSettings; onSaved: () => void }) {
  const { can } = useAuth();
  const isAdmin = can("admin");
  const [busy, setBusy] = useState(false);
  const current = settings?.currency_symbol ?? "$";
  const options = CURRENCY_SYMBOLS.some((c) => c.id === current)
    ? CURRENCY_SYMBOLS
    : [...CURRENCY_SYMBOLS, { id: current, label: current }];

  async function choose(symbol: string | null) {
    if (!settings || !symbol || symbol === current) return;
    setBusy(true);
    try {
      const saved = await api.updateSettings({ ...settings, currency_symbol: symbol });
      setCurrencySymbol(saved.currency_symbol);
      onSaved();
      toast.success(`Amounts now show in ${saved.currency_symbol}`);
    } catch (e) {
      toast.danger(e instanceof ApiError ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Card>
      <Card.Header>
        <LockableTitle locked={!isAdmin}>Currency</LockableTitle>
        <Card.Description>
          The symbol shown with costs on service records, in reports, and in Excel exports. It doesn't convert
          amounts. Applies to everyone.
        </Card.Description>
      </Card.Header>
      <Card.Content>
        {isAdmin ? (
          <SelectInput<string>
            label="Currency symbol"
            value={current}
            onChange={(v) => void choose(v)}
            options={options}
            isDisabled={busy || !settings}
            isRequired
            allowEmpty={false}
            className="max-w-xs"
          />
        ) : (
          <p className="text-sm">
            Amounts are shown in <b>{current}</b>. An administrator can change this.
          </p>
        )}
      </Card.Content>
    </Card>
  );
}

/** Today in `format`, as an example. */
function exampleDate(format: DateFormatId): string {
  const now = new Date();
  return formatYmd(now.getFullYear(), now.getMonth() + 1, now.getDate(), format);
}

function DateFormatCard({ settings, onSaved }: { settings?: AppSettings; onSaved: () => void }) {
  const { can } = useAuth();
  const isAdmin = can("admin");
  const [busy, setBusy] = useState(false);
  const current = (settings?.date_format ?? "system") as DateFormatId;
  const options = DATE_FORMATS.map((f) =>
    f.id === "system"
      ? { id: f.id, label: f.pattern, description: `${exampleDate(f.id)} on this PC · ${f.usedIn}` }
      : { id: f.id, label: exampleDate(f.id), description: `${f.pattern} · ${f.usedIn}` },
  );
  const currentInfo = DATE_FORMATS.find((f) => f.id === current);

  async function choose(format: DateFormatId | null) {
    if (!settings || !format || format === current) return;
    setBusy(true);
    try {
      const saved = await api.updateSettings({ ...settings, date_format: format });
      setDateFormat(saved.date_format);
      onSaved();
      toast.success(`Dates now show as ${exampleDate(saved.date_format as DateFormatId)}`);
    } catch (e) {
      toast.danger(e instanceof ApiError ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Card>
      <Card.Header>
        <LockableTitle locked={!isAdmin}>Date format</LockableTitle>
        <Card.Description>
          How dates appear in tables, on screens, in printed reports and labels, and in Excel exports. Date boxes
          take the day, month, and year in the same order. Applies to everyone.
        </Card.Description>
      </Card.Header>
      <Card.Content>
        {isAdmin ? (
          <SelectInput<DateFormatId>
            label="Date format"
            value={current}
            onChange={(v) => void choose(v)}
            options={options}
            isDisabled={busy || !settings}
            isRequired
            allowEmpty={false}
            className="max-w-xs"
          />
        ) : (
          <p className="text-sm">
            Dates are shown as <b>{exampleDate(current)}</b>
            {current === "system" ? " (this PC's Windows region setting)" : ` (${currentInfo?.pattern})`}. An
            administrator can change this.
          </p>
        )}
      </Card.Content>
    </Card>
  );
}

const EXAMPLE_SHARE = String.raw`\\server\share\PolarisCal`;

type FolderKey = "attachments_root" | "photos_root" | "service_manuals_root" | "documents_root";

const ATTACHMENT_FOLDERS: { key: FolderKey; label: string; hint: string }[] = [
  {
    key: "attachments_root",
    label: "Calibration certificates",
    hint: "Required before files can be attached. Any type below that's left empty is saved here too.",
  },
  { key: "photos_root", label: "Photos", hint: "" },
  { key: "service_manuals_root", label: "Service manuals", hint: "" },
  { key: "documents_root", label: "Other documents", hint: "Documents and files of type Other." },
];

const NO_FOLDERS: Record<FolderKey, string> = {
  attachments_root: "",
  photos_root: "",
  service_manuals_root: "",
  documents_root: "",
};

function AttachmentsFolderCard({ settings, onSaved }: { settings?: AppSettings; onSaved: () => void }) {
  const { can } = useAuth();
  const isAdmin = can("admin");
  const [folders, setFolders] = useState<Record<FolderKey, string>>(NO_FOLDERS);
  const [errors, setErrors] = useState<Partial<Record<FolderKey, string>>>({});
  const [busy, setBusy] = useState<FolderKey | "save" | null>(null);
  useEffect(() => {
    if (!settings) return;
    setFolders({
      attachments_root: settings.attachments_root ?? "",
      photos_root: settings.photos_root ?? "",
      service_manuals_root: settings.service_manuals_root ?? "",
      documents_root: settings.documents_root ?? "",
    });
  }, [settings]);

  const set = (key: FolderKey, value: string) => {
    setFolders((f) => ({ ...f, [key]: value }));
    setErrors((e) => ({ ...e, [key]: undefined }));
  };

  async function browse(key: FolderKey, label: string) {
    const picked = await openDialog({
      directory: true,
      multiple: false,
      title: `Choose the folder for ${label.toLowerCase()}`,
      defaultPath: folders[key] || folders.attachments_root || undefined,
    });
    if (typeof picked === "string") set(key, picked);
  }

  async function test(key: FolderKey) {
    setBusy(key);
    try {
      await api.checkAttachmentsFolder(folders[key]);
      setErrors((e) => ({ ...e, [key]: undefined }));
      toast.success("PolarisCal can write to this folder");
    } catch (e) {
      setErrors((errs) => ({ ...errs, [key]: e instanceof ApiError ? e.message : String(e) }));
    } finally {
      setBusy(null);
    }
  }

  async function save() {
    if (!settings) return;
    setBusy("save");
    const value = (key: FolderKey) => folders[key].trim() || null;
    try {
      await api.updateSettings({
        ...settings,
        attachments_root: value("attachments_root"),
        photos_root: value("photos_root"),
        service_manuals_root: value("service_manuals_root"),
        documents_root: value("documents_root"),
      });
      setErrors({});
      toast.success("Attachments folders saved");
      onSaved();
    } catch (e) {
      const fields = fieldErrors(e) as Partial<Record<FolderKey, string>>;
      if (Object.keys(fields).length) setErrors(fields);
      else toast.danger(e instanceof ApiError ? e.message : String(e));
    } finally {
      setBusy(null);
    }
  }

  const certificates = folders.attachments_root.trim();
  return (
    <Card>
      <Card.Header>
        <LockableTitle locked={!isAdmin}>Attachments folders</LockableTitle>
        <Card.Description>
          Files are copied into these folders, in a folder per item, by type. Use network shares (for example
          {EXAMPLE_SHARE}) that everyone using PolarisCal can reach with the same path.
        </Card.Description>
      </Card.Header>
      <Card.Content>
        {isAdmin ? (
          <div className="flex flex-col gap-4">
            {ATTACHMENT_FOLDERS.map(({ key, label, hint }) => {
              const main = key === "attachments_root";
              return (
                <div key={key} className="flex flex-col gap-1">
                  <div className="flex items-start gap-2">
                    <TextInput
                      label={label}
                      value={folders[key]}
                      onChange={(v) => set(key, v)}
                      error={errors[key]}
                      placeholder={main ? EXAMPLE_SHARE : certificates ? "Same as calibration certificates" : ""}
                      description={hint || undefined}
                      className="flex-1"
                    />
                    <Button
                      className="mt-6"
                      variant="secondary"
                      aria-label={`Browse for the ${label.toLowerCase()} folder`}
                      onPress={() => void browse(key, label)}
                    >
                      <FolderOpen className="size-4" /> Browse
                    </Button>
                    <Button
                      className="mt-6"
                      variant="tertiary"
                      aria-label={`Test access to the ${label.toLowerCase()} folder`}
                      isDisabled={!folders[key].trim()}
                      isPending={busy === key}
                      onPress={() => void test(key)}
                    >
                      Test
                    </Button>
                  </div>
                </div>
              );
            })}
            <div>
              <Button isPending={busy === "save"} onPress={() => void save()}>
                Save
              </Button>
            </div>
            <p className="text-xs text-muted">
              Changing a folder applies to files attached from then on; it doesn't move existing files. To move them,
              copy them to the new location yourself, keeping the same folder structure.
            </p>
          </div>
        ) : (
          <dl className="flex flex-col gap-2 text-sm">
            {ATTACHMENT_FOLDERS.map(({ key, label }) => (
              <div key={key}>
                <dt className="text-xs text-muted">{label}</dt>
                <dd className="break-all">
                  {settings?.[key] ??
                    (key === "attachments_root" ? (
                      <span className="text-muted">
                        Not set. An administrator needs to choose a folder before files can be attached.
                      </span>
                    ) : (
                      <span className="text-muted">Same as calibration certificates</span>
                    ))}
                </dd>
              </div>
            ))}
          </dl>
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

const SETTINGS_TABS = ["appearance", "certificate", "administration"] as const;

/**
 * Side by side when the page has room for both (e.g. the window maximized), stacked
 * otherwise, `left` first. Goes by the page's own width, not the window's, so the sidebar
 * is accounted for. Needs an `@container` around it.
 */
function SettingsColumns({ left, right }: { left: ReactNode; right: ReactNode }) {
  return (
    <div className="grid max-w-5xl grid-cols-1 items-start gap-6 @6xl:max-w-[110rem] @6xl:grid-cols-2">
      <div className="flex min-w-0 flex-col gap-6">{left}</div>
      <div className="flex min-w-0 flex-col gap-6">{right}</div>
    </div>
  );
}

export function SettingsPage() {
  const { data, error, loading, reload } = useAsync(() => api.appInfo(), []);
  const { theme, setTheme } = useTheme();
  const settings = useAsync(() => api.getSettings(), []);
  const { can, refresh } = useAuth();
  const isAdmin = can("admin");
  const [dialog, setDialog] = useState<"connection" | "copy" | null>(null);
  const connection = useAsync(() => (isAdmin ? api.getConnectionSettings() : Promise.resolve(null)), [isAdmin]);
  // The open tab is kept in the address, so going back to Settings returns to it.
  const [params, setParams] = useSearchParams();
  const tab = SETTINGS_TABS.find((t) => t === params.get("tab")) ?? "appearance";

  return (
    <>
      <PageHeader title="Settings" />
      {error && <ErrorAlert error={error} onRetry={reload} />}
      {loading && !data && <Loading />}

      {/* In a flex row so the tabs take only the width they need. */}
      <div className="mb-6 flex">
      <Tabs selectedKey={tab} onSelectionChange={(k) => setParams({ tab: String(k) }, { replace: true })}>
        <Tabs.ListContainer>
          <Tabs.List aria-label="Settings sections" className="*:whitespace-nowrap">
            <Tabs.Tab id="appearance">
              Appearance
              <Tabs.Indicator />
            </Tabs.Tab>
            <Tabs.Tab id="certificate">
              Certificate Template
              <Tabs.Indicator />
            </Tabs.Tab>
            <Tabs.Tab id="administration">
              Administration
              <Tabs.Indicator />
            </Tabs.Tab>
          </Tabs.List>
        </Tabs.ListContainer>
      </Tabs>
      </div>

      {/* The certificate template has a preview beside it, so it gets the full width. */}
      <div className={tab === "certificate" ? "" : "@container"}>
        {tab === "appearance" && (
          <SettingsColumns
            left={
              <>
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
                    if (next) setTheme(String(next) as ThemeChoice);
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
            <CurrencyCard settings={settings.data} onSaved={settings.reload} />
              </>
            }
            right={<DateFormatCard settings={settings.data} onSaved={settings.reload} />}
          />
        )}

        {tab === "certificate" && <CertificateTemplateSettings />}

        {tab === "administration" && (
          <SettingsColumns
            left={
              <>
            {data && (
              <Card>
                <Card.Header>
                  <LockableTitle locked={!isAdmin}>Database connection</LockableTitle>
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
                          {data.store.asset_count} items · {data.store.calibration_count} service records
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
            <AttachmentsFolderCard settings={settings.data} onSaved={settings.reload} />
              </>
            }
            right={
              <>
                <SecurityCard settings={settings.data} onSaved={settings.reload} />
                <PasswordRulesCard settings={settings.data} onSaved={settings.reload} />
                <UpdatesCard version={data?.version} settings={settings.data} onSaved={settings.reload} />
              </>
            }
          />
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
      </div>
    </>
  );
}
