import { Alert, Button, Card, Chip, Spinner } from "@heroui/react";
import { Lock } from "lucide-react";
import type { ReactNode } from "react";
import type { ApiError } from "../lib/api";
import { useCurrencySymbol } from "../lib/currency";
import { formatCents } from "../lib/format";

export function PageHeader({
  title,
  description,
  actions,
}: {
  title: string;
  description?: string;
  actions?: ReactNode;
}) {
  return (
    <header className="mb-6 flex items-end justify-between gap-4">
      <div>
        <h1 className="text-2xl font-semibold tracking-tight">{title}</h1>
        {description && <p className="mt-1 text-sm text-muted">{description}</p>}
      </div>
      {actions && <div className="flex shrink-0 items-center gap-2">{actions}</div>}
    </header>
  );
}

/** A card title with a red lock in front when the signed-in user can't change the section. */
export function LockableTitle({
  locked,
  children,
  why = "Only an administrator can change this.",
}: {
  locked: boolean;
  children: ReactNode;
  why?: string;
}) {
  return (
    <Card.Title className="flex items-center gap-2">
      {locked && (
        <span title={why} className="inline-flex shrink-0">
          <Lock className="size-4 text-danger" strokeWidth={2.25} aria-hidden />
          <span className="sr-only">Locked. {why}</span>
        </span>
      )}
      {children}
    </Card.Title>
  );
}

export function ErrorAlert({
  error,
  onRetry,
}: {
  error: Pick<ApiError, "kind" | "message">;
  onRetry?: () => void;
}) {
  const title =
    error.kind === "unavailable"
      ? "Backend not available"
      : error.kind === "not_connected"
        ? "Not connected to a database"
        : "Something went wrong";
  return (
    <Alert status="danger" className="mb-6">
      <Alert.Indicator />
      <Alert.Content>
        <Alert.Title>{title}</Alert.Title>
        <Alert.Description>{error.message}</Alert.Description>
      </Alert.Content>
      {onRetry && (
        <Button size="sm" variant="danger-soft" onPress={onRetry}>
          Retry
        </Button>
      )}
    </Alert>
  );
}

/** An amount in cents, in the organization's currency; updates if the currency changes. */
export function Money({ cents }: { cents: number | null | undefined }) {
  useCurrencySymbol();
  return <>{formatCents(cents)}</>;
}

export function Loading({ label = "Loading…" }: { label?: string }) {
  return (
    <div className="flex items-center justify-center gap-3 py-16 text-sm text-muted">
      <Spinner size="sm" />
      {label}
    </div>
  );
}

export function EmptyNote({ icon, title, children }: { icon: ReactNode; title: string; children?: ReactNode }) {
  return (
    <div className="flex flex-col items-center gap-2 px-6 py-14 text-center">
      <div className="mb-1 grid size-11 place-items-center rounded-full bg-default text-muted">{icon}</div>
      <div className="font-medium">{title}</div>
      {children && <div className="max-w-md text-sm text-muted">{children}</div>}
    </div>
  );
}

type ChipColor = "default" | "accent" | "success" | "warning" | "danger";

export function StatusChip({ color, children }: { color: ChipColor; children: ReactNode }) {
  return (
    <Chip size="sm" variant="soft" color={color}>
      {children}
    </Chip>
  );
}
