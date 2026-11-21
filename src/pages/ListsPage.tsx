import { Button, Table, Tabs } from "@heroui/react";
import { ListChecks, Plus } from "lucide-react";
import { useState } from "react";
import type { LookupItem } from "../bindings/LookupItem";
import type { LookupKind } from "../bindings/LookupKind";
import { EmptyNote, ErrorAlert, Loading, PageHeader, StatusChip } from "../components/common";
import { LookupDialog } from "../components/LookupDialog";
import { api } from "../lib/api";
import { EMPTY, LOOKUP_KINDS, lookupKindMeta } from "../lib/format";
import { useAsync } from "../lib/useAsync";
import { useAuth } from "../lib/auth";

function LookupTable({ kind }: { kind: LookupKind }) {
  const { data, error, loading, reload } = useAsync(() => api.listLookups(kind, true), [kind]);
  const [dialog, setDialog] = useState<{ open: boolean; item?: LookupItem }>({ open: false });
  const meta = lookupKindMeta[kind];
  const { can } = useAuth();
  const isAdmin = can("admin");
  const canAdd = can("edit_records");
  // The store returns every list alphabetically.
  const items = data ?? [];
  const byId = new Map(items.map((i) => [i.id, i]));
  const nextSortOrder = Math.max(0, ...(data ?? []).map((i) => i.sort_order)) + 10;

  return (
    <>
      <div className="mb-3 flex items-center justify-between">
        <p className="text-sm text-muted">
          {isAdmin
            ? "Select an entry to edit or delete it."
            : canAdd
              ? "You can add entries; only administrators can edit or delete them."
              : "Only administrators can change these lists."}
        </p>
        {canAdd && (
          <Button size="sm" onPress={() => setDialog({ open: true })}>
            <Plus className="size-4" /> Add {meta.singular.toLowerCase()}
          </Button>
        )}
      </div>

      {error && <ErrorAlert error={error} onRetry={reload} />}
      {loading && !data ? (
        <Loading />
      ) : (
        <Table>
          <Table.ScrollContainer>
            <Table.Content
              aria-label={meta.label}
              onRowAction={
                isAdmin
                  ? (key) => {
                      const item = byId.get(Number(key));
                      if (item) setDialog({ open: true, item });
                    }
                  : undefined
              }
            >
              <Table.Header>
                <Table.Column isRowHeader>Name</Table.Column>
                <Table.Column>Description</Table.Column>
                {kind === "provider" && <Table.Column>Type</Table.Column>}
                {kind === "status" && <Table.Column>Due tracking</Table.Column>}
                {kind === "service_type" && <Table.Column>Calibration</Table.Column>}
                <Table.Column>State</Table.Column>
              </Table.Header>
              <Table.Body
                items={items}
                renderEmptyState={() => (
                  <EmptyNote icon={<ListChecks className="size-5" />} title={`No ${meta.label.toLowerCase()} yet`}>
                    {canAdd ? "Add the first one with the button above." : "An administrator or technician can add entries."}
                  </EmptyNote>
                )}
              >
                {(item) => (
                  <Table.Row
                    id={item.id}
                    className={`${isAdmin ? "cursor-pointer" : ""} ${item.is_active ? "" : "opacity-55"}`}
                  >
                    <Table.Cell className="font-medium">{item.name}</Table.Cell>
                    <Table.Cell>{item.description ?? EMPTY}</Table.Cell>
                    {kind === "provider" && (
                      <Table.Cell>{item.is_internal ? "Internal" : "Outside vendor"}</Table.Cell>
                    )}
                    {kind === "status" && <Table.Cell>{item.tracks_due ? "Tracked" : "Excluded"}</Table.Cell>}
                    {kind === "service_type" && (
                      <Table.Cell>{item.counts_as_calibration ? "Sets due dates" : "Doesn't affect due dates"}</Table.Cell>
                    )}
                    <Table.Cell>
                      <StatusChip color={item.is_active ? "success" : "default"}>
                        {item.is_active ? "Active" : "Inactive"}
                      </StatusChip>
                    </Table.Cell>
                  </Table.Row>
                )}
              </Table.Body>
            </Table.Content>
          </Table.ScrollContainer>
        </Table>
      )}

      <LookupDialog
        kind={kind}
        item={dialog.item}
        nextSortOrder={nextSortOrder}
        isOpen={dialog.open}
        onOpenChange={(open) => setDialog((d) => ({ ...d, open }))}
        onSaved={reload}
        onDeleted={reload}
      />
    </>
  );
}

export function ListsPage() {
  const [kind, setKind] = useState<LookupKind>("location");

  return (
    <>
      <PageHeader
        title="Lists"
        description="The pick lists used across PolarisCal. Entries nothing uses yet can be deleted; once in use, make them inactive instead so existing records keep their values."
      />
      <Tabs selectedKey={kind} onSelectionChange={(k) => setKind(k as LookupKind)} className="mb-4">
        <Tabs.ListContainer>
          <Tabs.List aria-label="List">
            {LOOKUP_KINDS.map((k) => (
              <Tabs.Tab key={k} id={k}>
                {lookupKindMeta[k].label}
                <Tabs.Indicator />
              </Tabs.Tab>
            ))}
          </Tabs.List>
        </Tabs.ListContainer>
      </Tabs>
      <LookupTable key={kind} kind={kind} />
    </>
  );
}
