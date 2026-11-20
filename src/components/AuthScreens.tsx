// Full-screen views shown before the app: first-run setup, sign-in, forced password
// change, the idle lock screen, and connection problems.

import { Alert, Button, Form, Spinner } from "@heroui/react";
import { Lock, Sparkle } from "lucide-react";
import { type ReactNode, useState } from "react";
import type { User } from "../bindings/User";
import { ApiError, api } from "../lib/api";
import { type FieldErrors, fieldErrors } from "../lib/forms";
import { TextInput } from "./fields";

const MIN_PASSWORD = 10;

function Brand() {
  return (
    <div className="mb-6 flex flex-col items-center gap-3 text-center">
      <div className="grid size-12 place-items-center rounded-xl bg-accent text-accent-foreground">
        <Sparkle className="size-6" strokeWidth={2.5} />
      </div>
      <div>
        <div className="text-xl font-semibold tracking-tight">PolarisCal</div>
        <div className="text-xs text-muted">Calibration tracking</div>
      </div>
    </div>
  );
}

function AuthLayout({ title, description, children }: { title: string; description?: ReactNode; children: ReactNode }) {
  return (
    <div className="grid h-full place-items-center overflow-y-auto bg-background p-6">
      <div className="w-full max-w-sm">
        <Brand />
        <div className="rounded-2xl bg-surface p-6 shadow-surface">
          <h1 className="text-lg font-semibold">{title}</h1>
          {description && <p className="mt-1 mb-5 text-sm text-muted">{description}</p>}
          {!description && <div className="mb-5" />}
          {children}
        </div>
      </div>
    </div>
  );
}

function ErrorBanner({ message }: { message?: string }) {
  if (!message) return null;
  return (
    <Alert status="danger" className="mb-4">
      <Alert.Indicator />
      <Alert.Content>
        <Alert.Description>{message}</Alert.Description>
      </Alert.Content>
    </Alert>
  );
}

/** Shared submit handling: busy flag, field errors, and a general message. */
function useSubmit() {
  const [busy, setBusy] = useState(false);
  const [errors, setErrors] = useState<FieldErrors>({});
  const [message, setMessage] = useState<string>();

  async function run(action: () => Promise<unknown>) {
    setBusy(true);
    setMessage(undefined);
    try {
      await action();
    } catch (e) {
      const fields = fieldErrors(e);
      setErrors(fields);
      if (Object.keys(fields).length === 0) setMessage(e instanceof ApiError ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  /** Wraps a setter so editing a field clears its error. */
  const field =
    (key: string, set: (v: string) => void) =>
    (v: string) => {
      set(v);
      setErrors(({ [key]: _cleared, ...rest }) => rest);
    };

  return { busy, errors, setErrors, message, setMessage, run, field };
}

function checkNewPassword(password: string, confirm: string): FieldErrors {
  const errors: FieldErrors = {};
  if (password.length < MIN_PASSWORD) errors.password = `Use at least ${MIN_PASSWORD} characters.`;
  if (confirm !== password) errors.confirm = "The passwords don't match.";
  return errors;
}

export function SplashScreen() {
  return (
    <div className="grid h-full place-items-center bg-background">
      <Spinner />
    </div>
  );
}

export function ConnectionErrorScreen({ message, onRetry }: { message: string; onRetry: () => void }) {
  return (
    <AuthLayout title="Can't open the database" description="PolarisCal couldn't connect to its database.">
      <ErrorBanner message={message} />
      <p className="mb-4 text-sm text-muted">
        Check that the database file or server is reachable, then try again. The connection is configured in
        PolarisCal's <code>config.json</code>.
      </p>
      <Button fullWidth onPress={onRetry}>
        Try again
      </Button>
    </AuthLayout>
  );
}

export function SetupScreen({ onDone }: { onDone: () => void }) {
  const [username, setUsername] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  const { busy, errors, setErrors, message, run, field } = useSubmit();

  function submit() {
    const local = checkNewPassword(password, confirm);
    if (!username.trim()) local.username = "Choose a username.";
    if (!displayName.trim()) local.display_name = "Enter your name.";
    if (Object.keys(local).length) return setErrors(local);
    void run(async () => {
      await api.setupAdmin({
        username,
        display_name: displayName,
        email: email || null,
        role: "admin",
        password,
        must_change_password: false,
      });
      onDone();
    });
  }

  return (
    <AuthLayout
      title="Set up PolarisCal"
      description="This database has no accounts yet. Create the first administrator; you can add everyone else afterwards."
    >
      <ErrorBanner message={message} />
      <Form
        validationBehavior="aria"
        className="flex flex-col gap-4"
        onSubmit={(e) => {
          e.preventDefault();
          submit();
        }}
      >
        <TextInput label="Your name" value={displayName} onChange={field("display_name", setDisplayName)} error={errors.display_name} isRequired autoFocus />
        <TextInput label="Username" value={username} onChange={field("username", setUsername)} error={errors.username} isRequired autoComplete="username" />
        <TextInput label="Email" type="email" value={email} onChange={field("email", setEmail)} error={errors.email} />
        <TextInput
          label="Password"
          type="password"
          value={password}
          onChange={field("password", setPassword)}
          error={errors.password}
          isRequired
          autoComplete="new-password"
          description={`At least ${MIN_PASSWORD} characters.`}
        />
        <TextInput
          label="Confirm password"
          type="password"
          value={confirm}
          onChange={field("confirm", setConfirm)}
          error={errors.confirm}
          isRequired
          autoComplete="new-password"
        />
        <Button type="submit" fullWidth isPending={busy}>
          Create administrator
        </Button>
      </Form>
    </AuthLayout>
  );
}

export function SignInScreen({ onDone }: { onDone: () => void }) {
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const { busy, message, setMessage, run } = useSubmit();

  return (
    <AuthLayout title="Sign in">
      <ErrorBanner message={message} />
      <Form
        validationBehavior="aria"
        className="flex flex-col gap-4"
        onSubmit={(e) => {
          e.preventDefault();
          if (!username.trim() || !password) return setMessage("Enter your username and password.");
          void run(async () => {
            await api.signIn(username, password);
            onDone();
          });
        }}
      >
        <TextInput label="Username" value={username} onChange={setUsername} autoFocus autoComplete="username" />
        <TextInput
          label="Password"
          type="password"
          value={password}
          onChange={setPassword}
          autoComplete="current-password"
        />
        <Button type="submit" fullWidth isPending={busy}>
          Sign in
        </Button>
      </Form>
      <p className="mt-4 text-center text-xs text-muted">Forgot your password? Ask an administrator to reset it.</p>
    </AuthLayout>
  );
}

/** Password change form, used full-screen (forced) and inside a dialog. */
export function ChangePasswordForm({ onDone, submitLabel = "Change password" }: { onDone: () => void; submitLabel?: string }) {
  const [current, setCurrent] = useState("");
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  const { busy, errors, setErrors, message, run, field } = useSubmit();

  return (
    <>
      <ErrorBanner message={message} />
      <Form
        validationBehavior="aria"
        className="flex flex-col gap-4"
        onSubmit={(e) => {
          e.preventDefault();
          const local = checkNewPassword(password, confirm);
          if (!current) local.current_password = "Enter your current password.";
          if (Object.keys(local).length) return setErrors(local);
          void run(async () => {
            await api.changePassword(current, password);
            onDone();
          });
        }}
      >
        <TextInput
          label="Current password"
          type="password"
          value={current}
          onChange={field("current_password", setCurrent)}
          error={errors.current_password}
          autoFocus
          autoComplete="current-password"
        />
        <TextInput
          label="New password"
          type="password"
          value={password}
          onChange={field("password", setPassword)}
          error={errors.password}
          autoComplete="new-password"
          description={`At least ${MIN_PASSWORD} characters.`}
        />
        <TextInput
          label="Confirm new password"
          type="password"
          value={confirm}
          onChange={field("confirm", setConfirm)}
          error={errors.confirm}
          autoComplete="new-password"
        />
        <Button type="submit" fullWidth isPending={busy}>
          {submitLabel}
        </Button>
      </Form>
    </>
  );
}

export function ChangePasswordScreen({
  user,
  onDone,
  onSignOut,
}: {
  user: User;
  onDone: () => void;
  onSignOut: () => void;
}) {
  return (
    <AuthLayout
      title="Choose a new password"
      description={`Welcome, ${user.display_name}. An administrator set a temporary password for your account; choose your own to continue.`}
    >
      <ChangePasswordForm onDone={onDone} submitLabel="Save and continue" />
      <Button variant="ghost" fullWidth className="mt-2" onPress={onSignOut}>
        Sign out
      </Button>
    </AuthLayout>
  );
}

export function LockScreen({
  user,
  onUnlocked,
  onSignOut,
}: {
  user: User;
  onUnlocked: () => void;
  onSignOut: () => void;
}) {
  const [password, setPassword] = useState("");
  const { busy, message, run } = useSubmit();

  return (
    <div className="fixed inset-0 z-[100] grid place-items-center bg-background/80 p-6 backdrop-blur-md">
      <div className="w-full max-w-sm rounded-2xl bg-surface p-6 shadow-surface">
        <div className="mb-5 flex items-center gap-3">
          <div className="grid size-10 place-items-center rounded-full bg-default text-muted">
            <Lock className="size-5" />
          </div>
          <div>
            <div className="font-semibold">PolarisCal is locked</div>
            <div className="text-sm text-muted">
              Signed in as {user.display_name} ({user.username})
            </div>
          </div>
        </div>
        <ErrorBanner message={message} />
        <Form
          validationBehavior="aria"
          className="flex flex-col gap-4"
          onSubmit={(e) => {
            e.preventDefault();
            void run(async () => {
              await api.unlockSession(password);
              setPassword("");
              onUnlocked();
            });
          }}
        >
          <TextInput
            label="Password"
            type="password"
            value={password}
            onChange={setPassword}
            autoFocus
            autoComplete="current-password"
          />
          <Button type="submit" fullWidth isPending={busy}>
            Unlock
          </Button>
        </Form>
        <Button variant="ghost" fullWidth className="mt-2" onPress={onSignOut}>
          Sign out
        </Button>
        <p className="mt-3 text-center text-xs text-muted">Unsaved changes are kept while locked.</p>
      </div>
    </div>
  );
}
