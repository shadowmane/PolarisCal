// Password reset requests, for the badge administrators see on Users. Requests can come
// from any computer sharing the database, so the count is checked now and then, whenever
// the window regains focus, and whenever the Users page changes something.

import { useEffect, useState } from "react";
import { api } from "./api";

const CHANGED_EVENT = "polariscal:reset-requests-changed";
const CHECK_EVERY_MS = 60_000;

/** Tells the badge to check again (e.g. after a password reset). */
export function resetRequestsChanged() {
  window.dispatchEvent(new Event(CHANGED_EVENT));
}

/** The number of accounts waiting for a password reset; 0 when `enabled` is false. */
export function useResetRequestCount(enabled: boolean): number {
  const [count, setCount] = useState(0);
  useEffect(() => {
    if (!enabled) {
      setCount(0);
      return;
    }
    let live = true;
    const check = () => {
      api
        .passwordResetRequests()
        .then((n) => live && setCount(n))
        .catch(() => undefined);
    };
    check();
    const timer = window.setInterval(check, CHECK_EVERY_MS);
    window.addEventListener("focus", check);
    window.addEventListener(CHANGED_EVENT, check);
    return () => {
      live = false;
      window.clearInterval(timer);
      window.removeEventListener("focus", check);
      window.removeEventListener(CHANGED_EVENT, check);
    };
  }, [enabled]);
  return count;
}
