// Billing: service records by outside vendors, a section per vendor, with what's been
// invoiced. Entering an invoice logs its number and lines on the records and checks its
// prices against the recorded costs.

import { Button, Table } from "@heroui/react";
import { ReceiptText } from "lucide-react";
import { useMemo, useState } from "react";
import { Link } from "react-router";
import type { Asset } from "../bindings/Asset";
import type { CalibrationRecord } from "../bindings/CalibrationRecord";
import type { LookupItem } from "../bindings/LookupItem";
import { CollapsibleCard, EmptyNote, ErrorAlert, Loading, Money, PageHeader, StatusChip } from "../components/common";
import { SelectInput, ToggleSwitch } from "../components/fields";
import { InvoiceDialog } from "../components/InvoiceDialog";
import { api } from "../lib/api";
import { useAuth } from "../lib/auth";
import { useCurrencySymbol } from "../lib/currency";
import { EMPTY, formatCents, formatDate } from "../lib/format";
import { useAsync } from "../lib/useAsync";

function plural(n: number, word: string): string {
  return `${n} ${word}${n === 1 ? "" : "s"}`;
}

function VendorSection({
  vendor,
  records,
  shown,
  assets,
  canEdit,
  onInvoice,
}: {
  vendor: LookupItem;
  /** All of the vendor's unarchived records. */
  records: CalibrationRecord[];
  /** The ones that pass the filters. */
  shown: CalibrationRecord[];
  assets: Map<number, Asset>;
  canEdit: boolean;
  onInvoice: (invoiceNumber: string | null) => void;
}) {
  const open = records.filter((r) => r.invoice_number == null);
  const openTotal = open.reduce((s, r) => s + (r.cost_cents ?? 0), 0);
  const invoices = new Set(records.map((r) => r.invoice_number).filter(Boolean)).size;
  const summary = [
    plural(records.length, "service record"),
    open.length ? `${open.length} not invoiced (${formatCents(openTotal)} recorded)` : records.length ? "all invoiced" : "",
    invoices ? plural(invoices, "invoice") : "",
  ]
    .filter(Boolean)
    .join(" · ");

  return (
    <CollapsibleCard
      title={
        <span className="flex items-center gap-2">
          {vendor.name}
          {!vendor.is_active && <span className="text-xs font-normal text-muted">(inactive)</span>}
        </span>
      }
      description={summary}
      rememberAs={`billing-${vendor.id}`}
      defaultOpen
      actions={
        canEdit && (
          <Button size="sm" variant="secondary" isDisabled={records.length === 0} onPress={() => onInvoice(null)}>
            <ReceiptText className="size-4" /> Enter invoice
          </Button>
        )
      }
    >
      {records.length === 0 ? (
        <p className="text-sm text-muted">No service records by {vendor.name} yet.</p>
      ) : (
        <Table>
          <Table.ScrollContainer>
            <Table.Content aria-label={`Service records by ${vendor.name}`}>
              <Table.Header>
                <Table.Column isRowHeader>Date</Table.Column>
                <Table.Column>Asset tag</Table.Column>
                <Table.Column>Description</Table.Column>
                <Table.Column>Service</Table.Column>
                <Table.Column>Certificate</Table.Column>
                <Table.Column className="text-right">Cost</Table.Column>
                <Table.Column>Invoice</Table.Column>
              </Table.Header>
              <Table.Body
                items={shown}
                renderEmptyState={() => <p className="py-6 text-center text-sm text-muted">No records match the filters.</p>}
              >
                {(r) => (
                  <Table.Row id={r.id}>
                    <Table.Cell className="font-medium whitespace-nowrap">{formatDate(r.cal_date)}</Table.Cell>
                    <Table.Cell>
                      <Link to={`/assets/${r.asset_id}`} className="font-medium text-accent hover:underline">
                        {r.asset_tag}
                      </Link>
                    </Table.Cell>
                    <Table.Cell>{assets.get(r.asset_id)?.description ?? EMPTY}</Table.Cell>
                    <Table.Cell>{r.service_type_name}</Table.Cell>
                    <Table.Cell>{r.certificate_number ?? EMPTY}</Table.Cell>
                    <Table.Cell className="text-right tabular-nums">
                      <Money cents={r.cost_cents} />
                    </Table.Cell>
                    <Table.Cell>
                      {r.invoice_number ? (
                        canEdit ? (
                          <button
                            type="button"
                            className="text-left text-accent hover:underline"
                            title="Open this invoice"
                            onClick={() => onInvoice(r.invoice_number)}
                          >
                            {r.invoice_number}
                            <span className="text-muted"> · line {r.invoice_line}</span>
                          </button>
                        ) : (
                          <span>
                            {r.invoice_number}
                            <span className="text-muted"> · line {r.invoice_line}</span>
                          </span>
                        )
                      ) : (
                        <StatusChip color="warning">Not invoiced</StatusChip>
                      )}
                      {r.invoice_date && <div className="text-xs text-muted">{formatDate(r.invoice_date)}</div>}
                    </Table.Cell>
                  </Table.Row>
                )}
              </Table.Body>
            </Table.Content>
          </Table.ScrollContainer>
        </Table>
      )}
    </CollapsibleCard>
  );
}

export function BillingPage() {
  useCurrencySymbol();
  const { can } = useAuth();
  const canEdit = can("edit_records");
  const data = useAsync(async () => {
    const [records, providers, assets] = await Promise.all([
      api.listCalibrations(),
      api.listLookups("provider", true),
      api.listAssets({ include_archived: true }),
    ]);
    return { records, providers, assets: new Map(assets.map((a) => [a.id, a])) };
  }, []);
  const [year, setYear] = useState<number | null>(null);
  const [onlyOpen, setOnlyOpen] = useState(false);
  const [dialog, setDialog] = useState<{ vendor: LookupItem; invoice: string | null; open: boolean } | null>(null);

  const byVendor = useMemo(() => {
    const d = data.data;
    if (!d) return [];
    return d.providers
      .filter((p) => !p.is_internal)
      .map((vendor) => ({
        vendor,
        records: d.records.filter((r) => r.calibrated_by === "provider" && r.calibrated_by_id === vendor.id),
      }))
      .filter((v) => v.vendor.is_active || v.records.length > 0)
      .sort((a, b) => a.vendor.name.localeCompare(b.vendor.name));
  }, [data.data]);

  const years = useMemo(
    () =>
      [...new Set(byVendor.flatMap((v) => v.records.map((r) => Number(r.cal_date.slice(0, 4)))))].sort((a, b) => b - a),
    [byVendor],
  );

  const filter = (r: CalibrationRecord) =>
    (year == null || r.cal_date.startsWith(`${year}-`)) && (!onlyOpen || r.invoice_number == null);

  const dialogRecords = dialog ? (byVendor.find((v) => v.vendor.id === dialog.vendor.id)?.records ?? []) : [];

  return (
    <>
      <PageHeader
        title="Billing"
        description="Service records by outside vendors, by vendor. Enter an invoice to check its prices against the recorded costs and log its number and line on each record."
      />

      <div className="mb-5 flex flex-wrap items-end justify-between gap-4">
        <div className="w-48">
          <SelectInput<number>
            label="Service year"
            value={year}
            onChange={setYear}
            options={years.map((y) => ({ id: y, label: String(y) }))}
            placeholder="All years"
          />
        </div>
        <ToggleSwitch label="Only records not invoiced" isSelected={onlyOpen} onChange={setOnlyOpen} />
      </div>

      {data.error && <ErrorAlert error={data.error} onRetry={data.reload} />}
      {data.loading && !data.data && <Loading />}
      {data.data && byVendor.length === 0 && (
        <EmptyNote icon={<ReceiptText className="size-5" />} title="No outside vendors">
          Calibration providers that aren't marked Internal on the Lists page appear here with their service records.
        </EmptyNote>
      )}
      <div className="flex flex-col gap-4">
        {data.data &&
          byVendor.map(({ vendor, records }) => (
            <VendorSection
              key={vendor.id}
              vendor={vendor}
              records={records}
              shown={records.filter(filter)}
              assets={data.data!.assets}
              canEdit={canEdit}
              onInvoice={(invoice) => setDialog({ vendor, invoice, open: true })}
            />
          ))}
      </div>

      {dialog && data.data && (
        <InvoiceDialog
          isOpen={dialog.open}
          onOpenChange={(open) => setDialog((d) => (d ? { ...d, open } : d))}
          provider={dialog.vendor}
          records={dialogRecords}
          assets={data.data.assets}
          invoiceNumber={dialog.invoice}
          onSaved={data.reload}
        />
      )}
    </>
  );
}
