// Counts shown as red badges in the sidebar (password reset requests, check-out requests).
// They can change on any computer sharing the database, so each is checked now and then,
// whenever the window regains focus, and whenever this computer changes something.

import { useEffect, useState } from "react";

const CHECK_EVERY_MS = 60_000;

/** Tells badges listening for `event` to check again. */
export function countChanged(event: string) {
  window.dispatchEvent(new Event(event));
}

/** The latest result of `load`; 0 when `enabled` is false. */
export function useLiveCount(load: () => Promise<number>, enabled: boolean, event: string): number {
  const [count, setCount] = useState(0);
  useEffect(() => {
    if (!enabled) {
      setCount(0);
      return;
    }
    let live = true;
    const check = () => {
      load()
        .then((n) => live && setCount(n))
        .catch(() => undefined);
    };
    check();
    const timer = window.setInterval(check, CHECK_EVERY_MS);
    window.addEventListener("focus", check);
    window.addEventListener(event, check);
    return () => {
      live = false;
      window.clearInterval(timer);
      window.removeEventListener("focus", check);
      window.removeEventListener(event, check);
    };
    // `load` is a module-level API call; re-subscribing on every render isn't wanted.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [enabled, event]);
  return count;
}
