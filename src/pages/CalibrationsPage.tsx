import { Table } from "@heroui/react";
import { ClipboardCheck } from "lucide-react";
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

export function CalibrationRecordsTable() {
  const { data, error, loading, reload } = useAsync(() => api.listCalibrations(), []);

  if (error) return <ErrorAlert error={error} onRetry={reload} />;
  if (loading && !data) return <Loading />;

  return (
    <Table>
      <Table.ScrollContainer>
        <Table.Content aria-label="Calibration records">
          <Table.Header>
            <Table.Column isRowHeader>Date</Table.Column>
            <Table.Column>Asset tag</Table.Column>
            <Table.Column>Calibrated by</Table.Column>
            <Table.Column>Certificate</Table.Column>
            <Table.Column>As received</Table.Column>
            <Table.Column>As returned</Table.Column>
            <Table.Column>Cost</Table.Column>
            <Table.Column>Next due</Table.Column>
          </Table.Header>
          <Table.Body
            items={data ?? []}
            renderEmptyState={() => (
              <EmptyNote icon={<ClipboardCheck className="size-5" />} title="No calibration records yet">
                Records will appear here once calibrations are entered.
              </EmptyNote>
            )}
          >
            {(r) => (
              <Table.Row id={r.id}>
                <Table.Cell className="font-medium">{formatDate(r.cal_date)}</Table.Cell>
                <Table.Cell>{r.asset_tag}</Table.Cell>
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
  );
}

export function CalibrationsPage() {
  return (
    <>
      <PageHeader
        title="Calibration Records"
        description="Every calibration performed, newest first. Saving a record updates the item's last and next calibration dates."
      />
      <CalibrationRecordsTable />
    </>
  );
}
