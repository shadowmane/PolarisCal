// A pick-list dropdown with a "+" button to add a new entry without leaving the form.

import { Button, Tooltip } from "@heroui/react";
import { Plus } from "lucide-react";
import { useState } from "react";
import type { LookupKind } from "../bindings/LookupKind";
import { useAuth } from "../lib/auth";
import { lookupKindMeta } from "../lib/format";
import { type LookupMap, lookupOptions } from "../lib/lookups";
import type { AsyncState } from "../lib/useAsync";
import { SelectInput } from "./fields";
import { LookupDialog } from "./LookupDialog";

export function LookupSelect({
  kind,
  label,
  value,
  onChange,
  lookups,
  error,
  isRequired,
  className,
  placeholder,
  isDisabled,
  description,
}: {
  kind: LookupKind;
  label: string;
  value: number | null;
  onChange: (id: number | null) => void;
  lookups: AsyncState<LookupMap>;
  error?: string;
  isRequired?: boolean;
  className?: string;
  placeholder?: string;
  isDisabled?: boolean;
  description?: string;
}) {
  const { can } = useAuth();
  // Technicians and administrators may add entries; only administrators edit or delete them.
  const canAdd = can("edit_records");
  const [adding, setAdding] = useState(false);
  const entries = lookups.data?.[kind] ?? [];
  const nextSortOrder = Math.max(0, ...entries.map((i) => i.sort_order)) + 10;
  const singular = lookupKindMeta[kind].singular.toLowerCase();

  return (
    <div className={`flex items-start gap-2 ${className ?? ""}`}>
      <SelectInput<number>
        label={label}
        value={value}
        onChange={onChange}
        options={lookupOptions(lookups.data, kind, value)}
        error={error}
        isRequired={isRequired}
        placeholder={placeholder}
        isDisabled={isDisabled}
        description={description}
        className="min-w-0 flex-1"
      />
      {canAdd && !isDisabled && (
        <>
          <Tooltip delay={300}>
            <Button
              isIconOnly
              variant="secondary"
              className="mt-6 shrink-0"
              aria-label={`Add a ${singular}`}
              onPress={() => setAdding(true)}
            >
              <Plus className="size-4" />
            </Button>
            <Tooltip.Content>Add a {singular}</Tooltip.Content>
          </Tooltip>
          <LookupDialog
            kind={kind}
            nextSortOrder={nextSortOrder}
            isOpen={adding}
            onOpenChange={setAdding}
            onSaved={(item) => {
              lookups.reload();
              onChange(item.id);
            }}
          />
        </>
      )}
    </div>
  );
}
