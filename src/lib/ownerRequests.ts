// Check-out requests waiting for a Technician or Administrator, for the badge on
// Available Inventory.

import { api } from "./api";
import { countChanged, useLiveCount } from "./liveCount";

const CHANGED_EVENT = "polariscal:owner-requests-changed";

/** Tells the badge to check again (after a request is made or decided). */
export function ownerRequestsChanged() {
  countChanged(CHANGED_EVENT);
}

/** How many requests are waiting; 0 when `enabled` is false. */
export function usePendingOwnerRequestCount(enabled: boolean): number {
  return useLiveCount(api.pendingOwnerRequests, enabled, CHANGED_EVENT);
}
