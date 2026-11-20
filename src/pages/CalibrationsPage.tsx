import { Button, Table } from "@heroui/react";
import { ClipboardCheck, Plus } from "lucide-react";
import { useState } from "react";
import { Link } from "react-router";
import type { CalibrationRecord } from "../bindings/CalibrationRecord";
import { CalibrationDialog } from "../components/CalibrationDialog";
import { ToggleSwitch } from "../components/fields";
import { EmptyNote, ErrorAlert, Loading, PageHeader, StatusChip } from "../components/common";
import { api } from "../lib/api";
import {
  conditionReceivedMeta,
  conditionReturnedMeta,
  EMPTY,
  formatCents,
  formatDate,
} from "../lib/format";
import { useAsync } from "../lib/useAsync";
import { useAuth } from "../lib/auth";

export function CalibrationsPage() {
  const [showArchived, setShowArchived] = useState(false);
  const { data, error, loading, reload } = useAsync(
    () => api.listCalibrations({ include_archived: showArchived }),
    [showArchived],
  );
  const [dialog, setDialog] = useState<{ open: boolean; record?: CalibrationRecord }>({ open: false });
  const byId = new Map((data ?? []).map((r) => [r.id, r]));
  const { can } = useAuth();
  const canEdit = can("edit_records");

  return (
    <>
      <PageHeader
        title="Calibration Records"
        description="Every calibration performed, newest first. Saving a record updates the item's last and next calibration dates."
        actions={
          canEdit && (
            <Button onPress={() => setDialog({ open: true })}>
              <Plus className="size-4" /> Record calibration
            </Button>
          )
        }
      />

      <div className="mb-4 flex justify-end">
        <ToggleSwitch label="Show archived" isSelected={showArchived} onChange={setShowArchived} />
      </div>

      {error && <ErrorAlert error={error} onRetry={reload} />}
      {loading && !data && <Loading />}
      {data && (
        <Table>
          <Table.ScrollContainer>
            <Table.Content
              aria-label="Calibration records"
              onRowAction={
                canEdit
                  ? (key) => {
                      const record = byId.get(Number(key));
                      if (record) setDialog({ open: true, record });
                    }
                  : undefined
              }
            >
              <Table.Header>
                <Table.Column isRowHeader>Date</Table.Column>
                <Table.Column>Asset tag</Table.Column>
                <Table.Column>Calibrated by</Table.Column>
                <Table.Column>Certificate</Table.Column>
                <Table.Column>As received</Table.Column>
                <Table.Column>As returned</Table.Column>
                <Table.Column>Cost</Table.Column>
                <Table.Column>Due date set</Table.Column>
              </Table.Header>
              <Table.Body
                items={data}
                renderEmptyState={() => (
                  <EmptyNote icon={<ClipboardCheck className="size-5" />} title="No calibration records yet">
                    {canEdit ? "Use Record calibration to enter the first one." : "Nothing has been recorded yet."}
                  </EmptyNote>
                )}
              >
                {(r) => (
                  <Table.Row
                    id={r.id}
                    className={`${canEdit ? "cursor-pointer" : ""} ${r.is_archived ? "opacity-55" : ""}`}
                  >
                    <Table.Cell className="font-medium">
                      {formatDate(r.cal_date)}
                      {r.is_archived && <span className="ml-2 text-xs text-muted">(archived)</span>}
                    </Table.Cell>
                    <Table.Cell>
                      <Link
                        to={`/assets/${r.asset_id}`}
                        className="font-medium text-accent hover:underline"
                        onClick={(e) => e.stopPropagation()}
                      >
                        {r.asset_tag}
                      </Link>
                    </Table.Cell>
                    <Table.Cell>
                      {r.provider_name}
                      {r.technician && <div className="text-xs text-muted">{r.technician}</div>}
                    </Table.Cell>
                    <Table.Cell>{r.certificate_number ?? EMPTY}</Table.Cell>
                    <Table.Cell>
                      <StatusChip color={conditionReceivedMeta[r.condition_received].color}>
                        {conditionReceivedMeta[r.condition_received].label}
                      </StatusChip>
                    </Table.Cell>
                    <Table.Cell>
                      <StatusChip color={conditionReturnedMeta[r.condition_returned].color}>
                        {conditionReturnedMeta[r.condition_returned].label}
                      </StatusChip>
                    </Table.Cell>
                    <Table.Cell>{formatCents(r.cost_cents)}</Table.Cell>
                    <Table.Cell>{formatDate(r.next_due_date)}</Table.Cell>
                  </Table.Row>
                )}
              </Table.Body>
            </Table.Content>
          </Table.ScrollContainer>
        </Table>
      )}

      <CalibrationDialog
        isOpen={dialog.open}
        onOpenChange={(open) => setDialog((d) => ({ ...d, open }))}
        record={dialog.record}
        onSaved={reload}
      />
    </>
  );
}
