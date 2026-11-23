import { Button, SearchField, Table, Tabs } from "@heroui/react";
import { Boxes, Gauge, Plus } from "lucide-react";
import { useNavigate } from "react-router";
import { useAuth } from "../lib/auth";
import { useDeferredValue, useState } from "react";
import type { AssetKind } from "../bindings/AssetKind";
import { ToggleSwitch } from "../components/fields";
import { EmptyNote, ErrorAlert, Loading, PageHeader, StatusChip } from "../components/common";
import { api } from "../lib/api";
import { dueStatusMeta, EMPTY, formatDate, formatSchedule } from "../lib/format";
import { useAsync } from "../lib/useAsync";

type KindTab = "all" | AssetKind;

export function AssetsPage() {
  const [search, setSearch] = useState("");
  const [kind, setKind] = useState<KindTab>("all");
  const [showArchived, setShowArchived] = useState(false);
  // Retired items are no longer calibrated; they're listed unless this is turned off.
  const [showRetired, setShowRetired] = useState(true);
  const navigate = useNavigate();
  const { can } = useAuth();
  const query = useDeferredValue(search);
  const { data, error, loading, reload } = useAsync(
    () =>
      api.listAssets({
        search: query || null,
        kind: kind === "all" ? null : kind,
        include_archived: showArchived,
      }),
    [query, kind, showArchived],
  );
  const rows = data?.filter((a) => showRetired || a.status_role !== "retired");
  const hiddenRetired = (data?.length ?? 0) - (rows?.length ?? 0);

  return (
    <>
      <PageHeader
        title="Instruments & Systems"
        description="Everything under calibration control. Systems group the instruments they contain."
        actions={
          can("edit_records") && (
          <>
            <Button variant="secondary" onPress={() => navigate("/assets/new?kind=system")}>
              <Boxes className="size-4" /> New System
            </Button>
            <Button onPress={() => navigate("/assets/new")}>
              <Plus className="size-4" /> New Instrument
            </Button>
          </>
          )
        }
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

        <div className="flex items-center gap-4">
        <ToggleSwitch label="Show retired" isSelected={showRetired} onChange={setShowRetired} />
        <ToggleSwitch label="Show archived" isSelected={showArchived} onChange={setShowArchived} />
        <SearchField value={search} onChange={setSearch} aria-label="Search" className="w-80">
          <SearchField.Group>
            <SearchField.SearchIcon />
            <SearchField.Input placeholder="Search tag, description, model, serial…" />
            <SearchField.ClearButton />
          </SearchField.Group>
        </SearchField>
        </div>
      </div>

      {error && <ErrorAlert error={error} onRetry={reload} />}
      {loading && !data && <Loading />}
      {rows && (
        <Table>
          <Table.ScrollContainer>
            <Table.Content aria-label="Instruments and systems" onRowAction={(key) => navigate(`/assets/${key}`)}>
              <Table.Header>
                <Table.Column isRowHeader>Asset tag</Table.Column>
                <Table.Column>Description</Table.Column>
                <Table.Column>Manufacturer / model</Table.Column>
                <Table.Column>Serial</Table.Column>
                <Table.Column>Location</Table.Column>
                <Table.Column>Department</Table.Column>
                <Table.Column>Status</Table.Column>
                <Table.Column>Interval</Table.Column>
                <Table.Column>Last calibration</Table.Column>
                <Table.Column>Next due</Table.Column>
              </Table.Header>
              <Table.Body
                items={rows}
                renderEmptyState={() => (
                  <EmptyNote
                    icon={<Gauge className="size-5" />}
                    title={query ? "No matches" : "No instruments or systems yet"}
                  >
                    {hiddenRetired > 0
                      ? `${hiddenRetired} retired ${hiddenRetired === 1 ? "item is" : "items are"} hidden. Turn on Show retired to see ${hiddenRetired === 1 ? "it" : "them"}.`
                      : query
                      ? `Nothing matches “${query}”.`
                      : can("edit_records")
                        ? "Use New Instrument or New System to add the first one."
                        : "Nothing has been added yet."}
                  </EmptyNote>
                )}
              >
                {(a) => (
                  <Table.Row id={a.id} className={`cursor-pointer ${a.is_archived ? "opacity-55" : ""}`}>
                    <Table.Cell>
                      <div className="flex items-center gap-2 font-medium">
                        {a.kind === "system" && <Boxes className="size-4 text-muted" aria-label="System" />}
                        {a.asset_tag}
                        {a.is_archived && <span className="text-xs font-normal text-muted">(archived)</span>}
                      </div>
                      {a.parent_tag && <div className="text-xs text-muted">in {a.parent_tag}</div>}
                    </Table.Cell>
                    <Table.Cell>{a.description}</Table.Cell>
                    <Table.Cell>
                      {[a.manufacturer_name, a.model_number].filter(Boolean).join(" · ") || EMPTY}
                    </Table.Cell>
                    <Table.Cell>{a.serial_number ?? EMPTY}</Table.Cell>
                    <Table.Cell>{a.location_name ?? EMPTY}</Table.Cell>
                    <Table.Cell>{a.department_name ?? EMPTY}</Table.Cell>
                    <Table.Cell>
                      <span
                        className={
                          a.status_role === "retired"
                            ? "font-medium text-danger"
                            : a.status_role === "storage"
                              ? "font-medium text-warning"
                              : undefined
                        }
                      >
                        {a.status_name}
                      </span>
                    </Table.Cell>
                    <Table.Cell>{formatSchedule(a)}</Table.Cell>
                    <Table.Cell>{formatDate(a.last_cal_date)}</Table.Cell>
                    <Table.Cell>
                      {/* Retired items aren't calibrated any more, so they have nothing due. */}
                      {a.status_role === "retired" ? (
                        EMPTY
                      ) : (
                      <div className="flex items-center gap-2">
                        <span>{formatDate(a.next_cal_date)}</span>
                        {(a.due_status === "overdue" || a.due_status === "extended" || a.due_status === "due_soon") && (
                          <StatusChip color={dueStatusMeta[a.due_status].color}>
                            {dueStatusMeta[a.due_status].label}
                          </StatusChip>
                        )}
                      </div>
                      )}
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
