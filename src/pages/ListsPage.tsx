import { Table, Tabs } from "@heroui/react";
import { ListChecks } from "lucide-react";
import { useState } from "react";
import type { LookupKind } from "../bindings/LookupKind";
import { EmptyNote, ErrorAlert, Loading, PageHeader, StatusChip } from "../components/common";
import { api } from "../lib/api";
import { EMPTY, LOOKUP_KINDS, lookupKindMeta } from "../lib/format";
import { useAsync } from "../lib/useAsync";

function LookupTable({ kind }: { kind: LookupKind }) {
  const { data, error, loading, reload } = useAsync(() => api.listLookups(kind, true), [kind]);
  const meta = lookupKindMeta[kind];

  if (error) return <ErrorAlert error={error} onRetry={reload} />;
  if (loading && !data) return <Loading />;

  return (
    <Table>
      <Table.ScrollContainer>
        <Table.Content aria-label={meta.label}>
          <Table.Header>
            <Table.Column isRowHeader>Name</Table.Column>
            <Table.Column>Description</Table.Column>
            {kind === "provider" && <Table.Column>Type</Table.Column>}
            {kind === "status" && <Table.Column>Due tracking</Table.Column>}
            <Table.Column>State</Table.Column>
          </Table.Header>
          <Table.Body
            items={data ?? []}
            renderEmptyState={() => (
              <EmptyNote icon={<ListChecks className="size-5" />} title={`No ${meta.label.toLowerCase()} yet`}>
                Editing lists comes in the next build phase.
              </EmptyNote>
            )}
          >
            {(item) => (
              <Table.Row id={item.id}>
                <Table.Cell className="font-medium">{item.name}</Table.Cell>
                <Table.Cell>{item.description ?? EMPTY}</Table.Cell>
                {kind === "provider" && (
                  <Table.Cell>{item.is_internal ? "Internal" : "Outside vendor"}</Table.Cell>
                )}
                {kind === "status" && <Table.Cell>{item.tracks_due ? "Tracked" : "Excluded"}</Table.Cell>}
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
  );
}

export function ListsPage() {
  const [kind, setKind] = useState<LookupKind>("status");

  return (
    <>
      <PageHeader
        title="Lists"
        description="The pick lists used across PolarisCal. Inactive entries stay on existing records but can't be chosen for new ones."
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
