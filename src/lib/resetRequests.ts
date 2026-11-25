// Password reset requests, for the badge administrators see on Users.

import { api } from "./api";
import { countChanged, useLiveCount } from "./liveCount";

const CHANGED_EVENT = "polariscal:reset-requests-changed";

/** Tells the badge to check again (e.g. after a password reset). */
export function resetRequestsChanged() {
  countChanged(CHANGED_EVENT);
}

/** The number of accounts waiting for a password reset; 0 when `enabled` is false. */
export function useResetRequestCount(enabled: boolean): number {
  return useLiveCount(api.passwordResetRequests, enabled, CHANGED_EVENT);
}
