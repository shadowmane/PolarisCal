// A card in the lower-right corner when a new version is available, on every screen
// (sign-in included), so updating doesn't need a sign-in first.

import { Button, Card } from "@heroui/react";
import { ArrowDownCircle, X } from "lucide-react";
import { useEffect, useState } from "react";
import { checkOnStartup, installUpdate, setNoticeDismissed, useAvailableUpdate, useNoticeDismissed } from "../lib/updates";

export function UpdateNotice({ enabled, signedIn }: { enabled: boolean; signedIn: boolean }) {
  const update = useAvailableUpdate();
  const dismissed = useNoticeDismissed();
  const [progress, setProgress] = useState<number | null>(null);
  const [installing, setInstalling] = useState(false);
  const [error, setError] = useState<string>();

  // One quiet check per launch, unless an administrator turned checks off.
  useEffect(() => {
    if (enabled) checkOnStartup();
  }, [enabled]);

  if (!update || (dismissed && !installing)) return null;

  async function install() {
    if (!update) return;
    setInstalling(true);
    setError(undefined);
    try {
      await installUpdate(update, setProgress);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setInstalling(false);
    }
  }

  const status = !installing
    ? null
    : progress == null
      ? "Downloading…"
      : progress < 1
        ? `Downloading… ${Math.round(progress * 100)}%`
        : "Installing; PolarisCal will restart…";

  return (
    <div role="status" aria-live="polite" className="fixed right-6 bottom-6 z-[100] w-[22rem] print:hidden">
      <Card className="gap-3 p-4 shadow-lg ring-1 ring-separator">
        <div className="flex items-start gap-3">
          <span className="grid size-8 shrink-0 place-items-center rounded-lg bg-accent-soft text-accent-soft-foreground">
            <ArrowDownCircle className="size-4" />
          </span>
          <div className="min-w-0 flex-1">
            <div className="font-medium">PolarisCal {update.version} is available</div>
            <div className="text-xs text-muted">
              {signedIn
                ? "Updating restarts PolarisCal, so save any open work first."
                : "Update now, then sign in to the new version."}
            </div>
          </div>
          {!installing && (
            <Button
              isIconOnly
              size="sm"
              variant="ghost"
              aria-label="Remind me later"
              onPress={() => setNoticeDismissed(true)}
            >
              <X className="size-4" />
            </Button>
          )}
        </div>
        {update.body && !installing && (
          <p className="line-clamp-3 text-xs whitespace-pre-line text-muted">{update.body}</p>
        )}
        {status && <p className="text-sm text-muted">{status}</p>}
        {error && <p className="text-sm text-danger">The update couldn't be installed: {error}</p>}
        <div className="flex justify-end gap-2">
          {!installing && (
            <Button size="sm" variant="tertiary" onPress={() => setNoticeDismissed(true)}>
              Later
            </Button>
          )}
          <Button size="sm" isPending={installing} onPress={() => void install()}>
            Update now
          </Button>
        </div>
      </Card>
    </div>
  );
}
