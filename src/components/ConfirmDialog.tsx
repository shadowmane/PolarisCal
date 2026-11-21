import { AlertDialog, Button } from "@heroui/react";
import { type ReactNode, useEffect, useState } from "react";
import { ApiError } from "../lib/api";
import { TextAreaInput } from "./fields";

/**
 * A modal yes/no question. With `askReason`, the user may also say why (recommended); the
 * reason is passed to `onConfirm` and stored in the audit trail. `onConfirm` may be
 * async; the dialog waits for it and shows its error if it fails.
 */
export function ConfirmDialog({
  isOpen,
  onOpenChange,
  title,
  children,
  confirmLabel,
  tone = "danger",
  askReason = false,
  reasonLabel = "Reason for change",
  onConfirm,
}: {
  isOpen: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  children: ReactNode;
  confirmLabel: string;
  tone?: "danger" | "accent";
  askReason?: boolean;
  reasonLabel?: string;
  onConfirm: (reason: string) => Promise<unknown> | void;
}) {
  const [busy, setBusy] = useState(false);
  const [reason, setReason] = useState("");
  const [error, setError] = useState<string>();

  useEffect(() => {
    if (isOpen) {
      setReason("");
      setError(undefined);
    }
  }, [isOpen]);

  async function confirm() {
    setBusy(true);
    try {
      await onConfirm(reason.trim());
      onOpenChange(false);
    } catch (e) {
      setError(e instanceof ApiError || e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <AlertDialog>
      <AlertDialog.Backdrop isOpen={isOpen} onOpenChange={onOpenChange}>
        <AlertDialog.Container size={askReason ? "md" : "sm"}>
          <AlertDialog.Dialog>
            <AlertDialog.Header>
              <AlertDialog.Icon status={tone === "danger" ? "danger" : "accent"} />
              <AlertDialog.Heading>{title}</AlertDialog.Heading>
            </AlertDialog.Header>
            <AlertDialog.Body>
              <div className="flex flex-col gap-4">
                <div className="text-sm text-muted">{children}</div>
                {askReason && (
                  <TextAreaInput
                    label={reasonLabel}
                    value={reason}
                    onChange={(v) => {
                      setReason(v);
                      setError(undefined);
                    }}
                    error={error}
                    rows={2}
                    description="Optional, but recommended. Recorded in the audit trail."
                  />
                )}
                {!askReason && error && <p className="text-sm text-danger">{error}</p>}
              </div>
            </AlertDialog.Body>
            <AlertDialog.Footer>
              <Button slot="close" variant="tertiary" isDisabled={busy}>
                Cancel
              </Button>
              <Button variant={tone === "danger" ? "danger" : "primary"} isPending={busy} onPress={confirm}>
                {confirmLabel}
              </Button>
            </AlertDialog.Footer>
          </AlertDialog.Dialog>
        </AlertDialog.Container>
      </AlertDialog.Backdrop>
    </AlertDialog>
  );
}
