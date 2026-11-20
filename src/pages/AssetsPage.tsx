import { SearchField, Table, Tabs } from "@heroui/react";
import { Boxes, Gauge } from "lucide-react";
import { useDeferredValue, useState } from "react";
import type { AssetKind } from "../bindings/AssetKind";
import { EmptyNote, ErrorAlert, Loading, PageHeader, StatusChip } from "../components/common";
import { api } from "../lib/api";
import { dueStatusMeta, EMPTY, formatDate, formatInterval } from "../lib/format";
import { useAsync } from "../lib/useAsync";

type KindTab = "all" | AssetKind;

export function AssetsPage() {
  const [search, setSearch] = useState("");
  const [kind, setKind] = useState<KindTab>("all");
  const query = useDeferredValue(search);
  const { data, error, loading, reload } = useAsync(
    () => api.listAssets({ search: query || null, kind: kind === "all" ? null : kind }),
    [query, kind],
  );

  return (
    <>
      <PageHeader
        title="Instruments & Systems"
        description="Everything under calibration control. Systems group the instruments they contain."
      />

      <div className="mb-4 flex items-center justify-between gap-4">
        <Tabs selectedKey={kind} onSelectionChange={(k) => setKind(k as KindTab)}>
          <Tabs.ListContainer>
            <Tabs.List aria-label="Filter by type">
              <Tabs.Tab id="all">
                All
                <Tabs.Indicator />
              </Tabs.Tab>
              <Tabs.Tab id="instrument">
                Instruments
                <Tabs.Indicator />
              </Tabs.Tab>
              <Tabs.Tab id="system">
                Systems
                <Tabs.Indicator />
              </Tabs.Tab>
            </Tabs.List>
          </Tabs.ListContainer>
        </Tabs>

        <SearchField value={search} onChange={setSearch} aria-label="Search" className="w-80">
          <SearchField.Group>
            <SearchField.SearchIcon />
            <SearchField.Input placeholder="Search tag, description, model, serial…" />
            <SearchField.ClearButton />
          </SearchField.Group>
        </SearchField>
      </div>

      {error && <ErrorAlert error={error} onRetry={reload} />}
      {loading && !data && <Loading />}
      {data && (
        <Table>
          <Table.ScrollContainer>
            <Table.Content aria-label="Instruments and systems">
              <Table.Header>
                <Table.Column isRowHeader>Asset tag</Table.Column>
                <Table.Column>Description</Table.Column>
                <Table.Column>Manufacturer / model</Table.Column>
                <Table.Column>Serial</Table.Column>
                <Table.Column>Location</Table.Column>
                <Table.Column>Status</Table.Column>
                <Table.Column>Interval</Table.Column>
                <Table.Column>Next due</Table.Column>
              </Table.Header>
              <Table.Body
                items={data}
                renderEmptyState={() => (
                  <EmptyNote
                    icon={<Gauge className="size-5" />}
                    title={query ? "No matches" : "No instruments or systems yet"}
                  >
                    {query
                      ? `Nothing matches “${query}”.`
                      : "Adding and editing items comes in the next build phase."}
                  </EmptyNote>
                )}
              >
                {(a) => (
                  <Table.Row id={a.id}>
                    <Table.Cell>
                      <div className="flex items-center gap-2 font-medium">
                        {a.kind === "system" && <Boxes className="size-4 text-muted" aria-label="System" />}
                        {a.asset_tag}
                      </div>
                      {a.parent_tag && <div className="text-xs text-muted">in {a.parent_tag}</div>}
                    </Table.Cell>
                    <Table.Cell>{a.description}</Table.Cell>
                    <Table.Cell>
                      {[a.manufacturer_name, a.model_number].filter(Boolean).join(" · ") || EMPTY}
                    </Table.Cell>
                    <Table.Cell>{a.serial_number ?? EMPTY}</Table.Cell>
                    <Table.Cell>{a.location_name ?? EMPTY}</Table.Cell>
                    <Table.Cell>{a.status_name}</Table.Cell>
                    <Table.Cell>{formatInterval(a.cal_interval)}</Table.Cell>
                    <Table.Cell>
                      <div className="flex items-center gap-2">
                        <span>{formatDate(a.next_cal_date)}</span>
                        {(a.due_status === "overdue" || a.due_status === "due_soon") && (
                          <StatusChip color={dueStatusMeta[a.due_status].color}>
                            {dueStatusMeta[a.due_status].label}
                          </StatusChip>
                        )}
                      </div>
                    </Table.Cell>
                  </Table.Row>
                )}
              </Table.Body>
            </Table.Content>
          </Table.ScrollContainer>
        </Table>
      )}
    </>
  );
}
