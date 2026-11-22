import { Alert, Button, Modal, Table, toast } from "@heroui/react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { ImagePlus, KeyRound, Plus, Trash2, Users } from "lucide-react";
import { useEffect, useState } from "react";
import type { Role } from "../bindings/Role";
import type { User } from "../bindings/User";
import { EmptyNote, ErrorAlert, Loading, PageHeader, StatusChip } from "../components/common";
import { SelectInput, SwitchInput, TextInput } from "../components/fields";
import { ApiError, api } from "../lib/api";
import { useAuth } from "../lib/auth";
import { EMPTY, formatDateTime } from "../lib/format";
import { type FieldErrors, fieldErrors } from "../lib/forms";
import { ROLES, roleMeta } from "../lib/permissions";
import { useAsync } from "../lib/useAsync";

const MIN_PASSWORD = 10;
const roleOptions = ROLES.map((r) => ({ id: r, label: roleMeta[r].label, description: roleMeta[r].description }));

function DialogShell({
  isOpen,
  onOpenChange,
  title,
  busy,
  submitLabel,
  formId,
  children,
}: {
  isOpen: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  busy: boolean;
  submitLabel: string;
  formId: string;
  children: React.ReactNode;
}) {
  return (
    <Modal>
      <Modal.Backdrop isOpen={isOpen} onOpenChange={onOpenChange} isDismissable={!busy}>
        <Modal.Container size="md" scroll="inside">
          <Modal.Dialog>
            <Modal.CloseTrigger />
            <Modal.Header>
              <Modal.Heading>{title}</Modal.Heading>
            </Modal.Header>
            <Modal.Body>{children}</Modal.Body>
            <Modal.Footer>
              <Button slot="close" variant="tertiary" isDisabled={busy}>
                Cancel
              </Button>
              <Button type="submit" form={formId} isPending={busy}>
                {submitLabel}
              </Button>
            </Modal.Footer>
          </Modal.Dialog>
        </Modal.Container>
      </Modal.Backdrop>
    </Modal>
  );
}

function Banner({ message }: { message?: string }) {
  if (!message) return null;
  return (
    <Alert status="danger">
      <Alert.Indicator />
      <Alert.Content>
        <Alert.Description>{message}</Alert.Description>
      </Alert.Content>
    </Alert>
  );
}

/** Applies an API error to a form: field messages where possible, otherwise a banner. */
function applyError(e: unknown, setErrors: (f: FieldErrors) => void, setMessage: (m?: string) => void) {
  const err = e instanceof ApiError ? e : new ApiError("database", String(e));
  if (err.kind === "duplicate") return setErrors({ username: err.message });
  const fields = fieldErrors(err);
  setErrors(fields);
  setMessage(Object.keys(fields).length === 0 || err.fields.some((f) => !f.field) ? err.message : undefined);
}

function NewUserDialog({
  isOpen,
  onOpenChange,
  onSaved,
}: {
  isOpen: boolean;
  onOpenChange: (o: boolean) => void;
  onSaved: () => void;
}) {
  const [username, setUsername] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [email, setEmail] = useState("");
  const [role, setRole] = useState<Role>("technician");
  const [password, setPassword] = useState("");
  const [mustChange, setMustChange] = useState(true);
  const [errors, setErrors] = useState<FieldErrors>({});
  const [message, setMessage] = useState<string>();
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!isOpen) return;
    setUsername("");
    setDisplayName("");
    setEmail("");
    setRole("technician");
    setPassword("");
    setMustChange(true);
    setErrors({});
    setMessage(undefined);
  }, [isOpen]);

  async function save() {
    const local: FieldErrors = {};
    if (!displayName.trim()) local.display_name = "Name is required.";
    if (!username.trim()) local.username = "Username is required.";
    if (password.length < MIN_PASSWORD) local.password = `Use at least ${MIN_PASSWORD} characters.`;
    if (Object.keys(local).length) return setErrors(local);
    setBusy(true);
    try {
      const user = await api.createUser({
        username,
        display_name: displayName,
        email: email || null,
        role,
        password,
        must_change_password: mustChange,
      });
      toast.success(`Added ${user.display_name}`);
      onSaved();
      onOpenChange(false);
    } catch (e) {
      applyError(e, setErrors, setMessage);
    } finally {
      setBusy(false);
    }
  }

  return (
    <DialogShell isOpen={isOpen} onOpenChange={onOpenChange} title="Add user" busy={busy} submitLabel="Add user" formId="new-user">
      <form
        id="new-user"
        noValidate
        className="flex flex-col gap-4 p-1"
        onSubmit={(e) => {
          e.preventDefault();
          void save();
        }}
      >
        <Banner message={message} />
        <TextInput label="Name" value={displayName} onChange={setDisplayName} error={errors.display_name} isRequired autoFocus />
        <TextInput label="Username" value={username} onChange={setUsername} error={errors.username} isRequired />
        <TextInput label="Email" type="email" value={email} onChange={setEmail} error={errors.email} />
        <SelectInput<Role> label="Role" value={role} onChange={(r) => r && setRole(r)} options={roleOptions} isRequired allowEmpty={false} />
        <TextInput
          label="Temporary password"
          type="password"
          value={password}
          onChange={setPassword}
          error={errors.password}
          isRequired
          autoComplete="new-password"
          description={`At least ${MIN_PASSWORD} characters. Give it to the user privately.`}
        />
        <SwitchInput
          label="Require a new password at first sign-in"
          isSelected={mustChange}
          onChange={setMustChange}
        />
      </form>
    </DialogShell>
  );
}

/**
 * The user's signature, printed on certificates they calibrate or approve. Saved as soon
 * as it's chosen or removed (separately from the rest of the dialog).
 */
function SignatureField({ user }: { user: User }) {
  const [signature, setSignature] = useState<string | null>();
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    setSignature(undefined);
    api
      .getUserSignature(user.id)
      .then(setSignature)
      .catch(() => setSignature(null));
  }, [user.id]);

  async function change(next: string | null) {
    setBusy(true);
    try {
      await api.setUserSignature(user.id, next);
      setSignature(next);
      toast.success(next ? `Signature saved for ${user.display_name}` : `Signature removed for ${user.display_name}`);
    } catch (e) {
      toast.danger(e instanceof ApiError ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  async function choose() {
    const path = await openDialog({
      title: "Choose a signature image",
      multiple: false,
      filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "gif", "webp", "bmp", "svg"] }],
    });
    if (typeof path !== "string") return;
    try {
      await change(await api.readImageFile(path));
    } catch (e) {
      toast.danger(e instanceof ApiError ? e.message : String(e));
    }
  }

  return (
    <div className="flex flex-col gap-2 border-t border-separator pt-4">
      <div>
        <p className="text-sm font-medium">Signature</p>
        <p className="text-xs text-muted">
          Printed on certificates this user calibrates or approves. A scan or photo of a signature on white or a
          transparent background works best. Saved straight away.
        </p>
      </div>
      <div className="flex flex-wrap items-center gap-3">
        {signature === undefined ? (
          <span className="text-sm text-muted">Loading…</span>
        ) : signature ? (
          <img src={signature} alt="Signature" className="h-14 max-w-56 rounded bg-white object-contain p-1" />
        ) : (
          <span className="text-sm text-muted">No signature saved</span>
        )}
        <Button variant="secondary" size="sm" isDisabled={busy || signature === undefined} onPress={() => void choose()}>
          <ImagePlus className="size-4" /> {signature ? "Replace…" : "Upload…"}
        </Button>
        {signature && (
          <Button variant="tertiary" size="sm" isDisabled={busy} onPress={() => void change(null)}>
            <Trash2 className="size-4" /> Remove
          </Button>
        )}
      </div>
    </div>
  );
}

function EditUserDialog({
  user,
  onOpenChange,
  onSaved,
}: {
  user: User | null;
  onOpenChange: (o: boolean) => void;
  onSaved: () => void;
}) {
  const { user: me } = useAuth();
  const [displayName, setDisplayName] = useState("");
  const [email, setEmail] = useState("");
  const [role, setRole] = useState<Role>("viewer");
  const [active, setActive] = useState(true);
  const [errors, setErrors] = useState<FieldErrors>({});
  const [message, setMessage] = useState<string>();
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!user) return;
    setDisplayName(user.display_name);
    setEmail(user.email ?? "");
    setRole(user.role);
    setActive(user.is_active);
    setErrors({});
    setMessage(undefined);
  }, [user]);

  async function save() {
    if (!user) return;
    if (!displayName.trim()) return setErrors({ display_name: "Name is required." });
    setBusy(true);
    try {
      await api.updateUser(user.id, user.row_version, {
        display_name: displayName,
        email: email || null,
        role,
        is_active: active,
      });
      toast.success(`Saved ${displayName}`);
      onSaved();
      onOpenChange(false);
    } catch (e) {
      applyError(e, setErrors, setMessage);
      const err = e as ApiError;
      if (err?.fields?.some((f) => f.field === "role" || f.field === "is_active")) setMessage(err.message);
    } finally {
      setBusy(false);
    }
  }

  const isMe = user?.id === me.id;
  return (
    <DialogShell
      isOpen={!!user}
      onOpenChange={onOpenChange}
      title={user ? `Edit ${user.username}` : ""}
      busy={busy}
      submitLabel="Save"
      formId="edit-user"
    >
      <form
        id="edit-user"
        noValidate
        className="flex flex-col gap-4 p-1"
        onSubmit={(e) => {
          e.preventDefault();
          void save();
        }}
      >
        <Banner message={message} />
        <TextInput label="Name" value={displayName} onChange={setDisplayName} error={errors.display_name} isRequired />
        <TextInput label="Email" type="email" value={email} onChange={setEmail} error={errors.email} />
        <SelectInput<Role> label="Role" value={role} onChange={(r) => r && setRole(r)} options={roleOptions} isRequired allowEmpty={false} />
        <SwitchInput
          label="Active"
          description={isMe ? "You can't deactivate your own account." : "Inactive users can't sign in. Their history is kept."}
          isSelected={active}
          onChange={setActive}
          isDisabled={isMe}
        />
        {user && <SignatureField user={user} />}
      </form>
    </DialogShell>
  );
}

function ResetPasswordDialog({
  user,
  onOpenChange,
}: {
  user: User | null;
  onOpenChange: (o: boolean) => void;
}) {
  const [password, setPassword] = useState("");
  const [mustChange, setMustChange] = useState(true);
  const [errors, setErrors] = useState<FieldErrors>({});
  const [message, setMessage] = useState<string>();
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!user) return;
    setPassword("");
    setMustChange(true);
    setErrors({});
    setMessage(undefined);
  }, [user]);

  async function save() {
    if (!user) return;
    if (password.length < MIN_PASSWORD) return setErrors({ password: `Use at least ${MIN_PASSWORD} characters.` });
    setBusy(true);
    try {
      await api.resetPassword(user.id, password, mustChange);
      toast.success(`Password reset for ${user.display_name}`);
      onOpenChange(false);
    } catch (e) {
      applyError(e, setErrors, setMessage);
    } finally {
      setBusy(false);
    }
  }

  return (
    <DialogShell
      isOpen={!!user}
      onOpenChange={onOpenChange}
      title={user ? `Reset password for ${user.username}` : ""}
      busy={busy}
      submitLabel="Reset password"
      formId="reset-password"
    >
      <form
        id="reset-password"
        noValidate
        className="flex flex-col gap-4 p-1"
        onSubmit={(e) => {
          e.preventDefault();
          void save();
        }}
      >
        <Banner message={message} />
        <TextInput
          label="New temporary password"
          type="password"
          value={password}
          onChange={setPassword}
          error={errors.password}
          isRequired
          autoFocus
          autoComplete="new-password"
          description={`At least ${MIN_PASSWORD} characters.`}
        />
        <SwitchInput label="Require a new password at next sign-in" isSelected={mustChange} onChange={setMustChange} />
      </form>
    </DialogShell>
  );
}

export function UsersPage() {
  const { can } = useAuth();
  const { data, error, loading, reload } = useAsync(() => api.listUsers(), []);
  const [adding, setAdding] = useState(false);
  const [editing, setEditing] = useState<User | null>(null);
  const [resetting, setResetting] = useState<User | null>(null);
  const byId = new Map((data ?? []).map((u) => [u.id, u]));

  if (!can("admin")) {
    return <ErrorAlert error={{ kind: "forbidden", message: "Only administrators can manage users." }} />;
  }

  return (
    <>
      <PageHeader
        title="Users"
        description="Who can sign in to PolarisCal and what they can do. Accounts are deactivated, never deleted, so the audit trail stays intact."
        actions={
          <Button onPress={() => setAdding(true)}>
            <Plus className="size-4" /> Add user
          </Button>
        }
      />
      {error && <ErrorAlert error={error} onRetry={reload} />}
      {loading && !data ? (
        <Loading />
      ) : (
        <Table>
          <Table.ScrollContainer>
            <Table.Content
              aria-label="Users"
              onRowAction={(key) => setEditing(byId.get(Number(key)) ?? null)}
            >
              <Table.Header>
                <Table.Column isRowHeader>Name</Table.Column>
                <Table.Column>Username</Table.Column>
                <Table.Column>Role</Table.Column>
                <Table.Column>Last sign-in</Table.Column>
                <Table.Column>State</Table.Column>
                <Table.Column>
                  <span className="sr-only">Actions</span>
                </Table.Column>
              </Table.Header>
              <Table.Body
                items={data ?? []}
                renderEmptyState={() => <EmptyNote icon={<Users className="size-5" />} title="No users" />}
              >
                {(u) => (
                  <Table.Row id={u.id} className={`cursor-pointer ${u.is_active ? "" : "opacity-55"}`}>
                    <Table.Cell className="font-medium">
                      {u.display_name}
                      {u.email && <div className="text-xs font-normal text-muted">{u.email}</div>}
                    </Table.Cell>
                    <Table.Cell>{u.username}</Table.Cell>
                    <Table.Cell>{roleMeta[u.role].label}</Table.Cell>
                    <Table.Cell>{u.last_sign_in_at ? formatDateTime(u.last_sign_in_at) : EMPTY}</Table.Cell>
                    <Table.Cell>
                      <div className="flex flex-wrap gap-1.5">
                        <StatusChip color={u.is_active ? "success" : "default"}>
                          {u.is_active ? "Active" : "Inactive"}
                        </StatusChip>
                        {u.must_change_password && <StatusChip color="warning">Password change pending</StatusChip>}
                      </div>
                    </Table.Cell>
                    <Table.Cell>
                      <Button size="sm" variant="ghost" onPress={() => setResetting(u)}>
                        <KeyRound className="size-4" /> Reset password
                      </Button>
                    </Table.Cell>
                  </Table.Row>
                )}
              </Table.Body>
            </Table.Content>
          </Table.ScrollContainer>
        </Table>
      )}

      <NewUserDialog isOpen={adding} onOpenChange={setAdding} onSaved={reload} />
      <EditUserDialog user={editing} onOpenChange={(o) => !o && setEditing(null)} onSaved={reload} />
      <ResetPasswordDialog user={resetting} onOpenChange={(o) => !o && setResetting(null)} />
    </>
  );
}
