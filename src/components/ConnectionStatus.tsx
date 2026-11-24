// The database connection at a glance: a light (green, yellow, red) and what's connected,
// in the sidebar and in the corner of the sign-in screens, where it also opens the
// connection settings.

import { Alert, Button, Modal, Tooltip } from "@heroui/react";
import { Database } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import type { ConnectionStatus } from "../bindings/ConnectionStatus";
import { ApiError, api } from "../lib/api";
import { ConnectionDialog } from "./DatabaseForms";
import { TextInput } from "./fields";

const SHORT_NAMES: Record<string, string> = {
  sqlite: "SQLite",
  postgres: "PostgreSQL",
  mysql: "MySQL",
  mssql: "SQL Server",
};

/** The connection's state, checked when shown, when the window regains focus, and on `reload`. */
export function useConnectionStatus() {
  const [status, setStatus] = useState<ConnectionStatus>();
  const [error, setError] = useState<string>();
  const reload = useCallback(async () => {
    try {
      setStatus(await api.connectionStatus());
      setError(undefined);
    } catch (e) {
      setError(e instanceof ApiError ? e.message : String(e));
    }
  }, []);
  useEffect(() => {
    void reload();
    const onFocus = () => void reload();
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, [reload]);
  return { status, error, reload };
}

/** "SQLite · schema v17", "SQL Server · can't connect", and so on. */
export function connectionLabel(status: ConnectionStatus | undefined, error?: string): string {
  if (!status) return error ? "Not connected" : "Connecting…";
  const name = SHORT_NAMES[status.backend ?? ""] ?? (status.is_file ? "SQLite" : "Database");
  if (status.state === "unavailable") return status.is_file ? `${name} · file unavailable` : `${name} · can't connect`;
  return status.schema_version != null ? `${name} · schema v${status.schema_version}` : name;
}

const LIGHT: Record<ConnectionStatus["state"], string> = {
  ready: "bg-success",
  not_set_up: "bg-warning",
  unavailable: "bg-danger",
};

export function ConnectionLight({ status, error }: { status?: ConnectionStatus; error?: string }) {
  const color = status ? LIGHT[status.state] : error ? "bg-danger" : "bg-default";
  return <span aria-hidden className={`size-2 shrink-0 rounded-full ${color}`} />;
}

/** What the tooltip says: where, then what the light means. */
export function ConnectionDetails({ status, error }: { status?: ConnectionStatus; error?: string }) {
  return (
    <div className="flex max-w-80 flex-col gap-1 text-xs">
      {status?.location && <p className="break-all">{status.location}</p>}
      <p className="text-muted">{status?.message ?? error ?? "Checking the database…"}</p>
    </div>
  );
}

/**
 * For the sign-in screens: shows the connection and opens its settings. If the database
 * works and has accounts, an administrator of it confirms who they are first.
 */
export function ConnectionButton({ onChanged }: { onChanged: () => void }) {
  const { status, error, reload } = useConnectionStatus();
  const [step, setStep] = useState<"admin" | "settings" | null>(null);

  function open() {
    setStep(status?.needs_admin ? "admin" : "settings");
  }

  return (
    <>
      <Tooltip delay={300}>
        <Button
          variant="ghost"
          size="sm"
          onPress={open}
          aria-label={`Database connection: ${connectionLabel(status, error)}. Change connection`}
          className="gap-2 text-xs text-muted"
        >
          <ConnectionLight status={status} error={error} />
          <Database className="size-3.5 shrink-0" />
          <span className="max-w-64 truncate">{connectionLabel(status, error)}</span>
        </Button>
        <Tooltip.Content placement="bottom start">
          <ConnectionDetails status={status} error={error} />
          <p className="mt-1 text-xs text-muted">Select to change the connection.</p>
        </Tooltip.Content>
      </Tooltip>
      <AdminCheckDialog
        isOpen={step === "admin"}
        onOpenChange={(o) => !o && setStep(null)}
        onConfirmed={() => setStep("settings")}
      />
      <ConnectionDialog
        isOpen={step === "settings"}
        onOpenChange={(o) => {
          if (!o) {
            setStep(null);
            void reload();
          }
        }}
        onSaved={() => {
          void reload();
          onChanged();
        }}
      />
    </>
  );
}

/** An administrator of the current database confirms who they are. */
function AdminCheckDialog({
  isOpen,
  onOpenChange,
  onConfirmed,
}: {
  isOpen: boolean;
  onOpenChange: (open: boolean) => void;
  onConfirmed: () => void;
}) {
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [message, setMessage] = useState<string>();
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    if (!isOpen) return;
    setUsername("");
    setPassword("");
    setMessage(undefined);
  }, [isOpen]);

  async function confirm() {
    if (!username.trim() || !password) return setMessage("Enter an administrator's username and password.");
    setBusy(true);
    try {
      await api.unlockConnectionSettings(username, password);
      onConfirmed();
    } catch (e) {
      setMessage(e instanceof ApiError ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Modal>
      <Modal.Backdrop isOpen={isOpen} onOpenChange={onOpenChange} isDismissable={!busy}>
        <Modal.Container size="sm">
          <Modal.Dialog>
            <Modal.CloseTrigger />
            <Modal.Header>
              <Modal.Heading>Administrator sign-in</Modal.Heading>
            </Modal.Header>
            <Modal.Body>
              <form
                id="connection-admin"
                noValidate
                className="flex flex-col gap-4 p-1"
                onSubmit={(e) => {
                  e.preventDefault();
                  void confirm();
                }}
              >
                <p className="text-sm text-muted">
                  This computer is connected to a database that's in use. To change the connection, an administrator of
                  that database signs in here first.
                </p>
                {message && (
                  <Alert status="danger">
                    <Alert.Indicator />
                    <Alert.Content>
                      <Alert.Description>{message}</Alert.Description>
                    </Alert.Content>
                  </Alert>
                )}
                <TextInput label="Username" value={username} onChange={setUsername} autoFocus autoComplete="username" />
                <TextInput
                  label="Password"
                  type="password"
                  value={password}
                  onChange={setPassword}
                  autoComplete="current-password"
                />
              </form>
            </Modal.Body>
            <Modal.Footer>
              <Button slot="close" variant="tertiary" isDisabled={busy}>
                Cancel
              </Button>
              <Button type="submit" form="connection-admin" isPending={busy}>
                Continue
              </Button>
            </Modal.Footer>
          </Modal.Dialog>
        </Modal.Container>
      </Modal.Backdrop>
    </Modal>
  );
}
