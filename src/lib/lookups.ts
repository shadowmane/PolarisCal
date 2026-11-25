import { UserRound } from "lucide-react";
import { createElement } from "react";
import type { LookupItem } from "../bindings/LookupItem";
import type { LookupKind } from "../bindings/LookupKind";
import type { Option } from "../components/fields";
import { api } from "./api";
import { LOOKUP_KINDS } from "./format";
import { pickable } from "./forms";
import { useAsync } from "./useAsync";

export type LookupMap = Record<LookupKind, LookupItem[]>;

/** Loads every pick list (including inactive entries) in one go. */
export function useLookups() {
  return useAsync(async () => {
    const lists = await Promise.all(LOOKUP_KINDS.map((k) => api.listLookups(k, true)));
    return Object.fromEntries(LOOKUP_KINDS.map((k, i) => [k, lists[i]])) as LookupMap;
  }, []);
}

/** Dropdown options for a list: active entries plus the current value. */
export function lookupOptions(
  map: LookupMap | undefined,
  kind: LookupKind,
  current: number | null,
): Option<number>[] {
  return pickable(map?.[kind], current).map((i) => ({
    id: i.id,
    label: i.is_active ? i.name : `${i.name} (inactive)`,
    // Owners linked to a user account get a user icon, so people can tell them from groups.
    icon:
      i.user_id != null
        ? createElement(UserRound, { className: "size-3.5 shrink-0 text-accent", "aria-label": "User account" })
        : undefined,
  }));
}
