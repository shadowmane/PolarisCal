// Entering an outside vendor's invoice: its number and date, the service records it bills
// (each with its line number), and the invoiced prices, checked against the recorded costs.
// Saving logs the invoice on those records and makes the invoiced prices their costs.
// Typing the number of an invoice that's already entered opens it for changes.

import { Alert, Button, Checkbox, Modal, toast } from "@heroui/react";
import { useEffect, useMemo, useState } from "react";
import type { Asset } from "../bindings/Asset";
import type { CalibrationRecord } from "../bindings/CalibrationRecord";
import type { LookupItem } from "../bindings/LookupItem";
import { ApiError, api } from "../lib/api";
import { useCurrencySymbol } from "../lib/currency";
import { EMPTY, formatCents, formatDate } from "../lib/format";
import { isoToday } from "../lib/reports";
import { DateInput, MoneyInput, NumberInput, TextInput } from "./fields";

interface Pick {
  line: number | null;
  /** The invoiced price in currency units (as typed), or null if not entered yet. */
  price: number | null;
}

const toCents = (price: number) => Math.round(price * 100);

function signedCents(cents: number): string {
  return `${cents > 0 ? "+" : "−"}${formatCents(Math.abs(cents))}`;
}

export function InvoiceDialog({
  isOpen,
  onOpenChange,
  provider,
  records,
  assets,
  invoiceNumber = null,
  onSaved,
}: {
  isOpen: boolean;
  onOpenChange: (open: boolean) => void;
  provider: LookupItem;
  /** The vendor's unarchived service records. */
  records: CalibrationRecord[];
  assets: Map<number, Asset>;
  /** An invoice to open, or null for a new one. */
  invoiceNumber?: string | null;
  onSaved?: () => void;
}) {
  useCurrencySymbol();
  const [number, setNumber] = useState("");
  const [date, setDate] = useState<string | null>(null);
  const [picked, setPicked] = useState<Map<number, Pick>>(new Map());
  const [search, setSearch] = useState("");
  const [tried, setTried] = useState(false);
  const [error, setError] = useState<string>();
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    // Cleared on close, so the next opening doesn't start from the last invoice.
    if (!isOpen) return setNumber("");
    setNumber(invoiceNumber ?? "");
    setDate(isoToday());
    setPicked(new Map());
    setSearch("");
    setTried(false);
    setError(undefined);
  }, [isOpen, invoiceNumber]);

  const trimmed = number.trim();
  const existing = useMemo(
    () => (trimmed ? records.filter((r) => r.invoice_number === trimmed) : []),
    [records, trimmed],
  );

  // An invoice that's already entered opens with its lines; otherwise records that are on
  // other invoices can't be picked.
  useEffect(() => {
    if (!isOpen) return;
    if (existing.length) {
      setPicked(
        new Map(
          existing.map((r) => [r.id, { line: r.invoice_line, price: r.cost_cents == null ? null : r.cost_cents / 100 }]),
        ),
      );
      setDate(existing[0].invoice_date ?? isoToday());
    } else {
      setPicked((was) => new Map([...was].filter(([id]) => records.find((r) => r.id === id)?.invoice_number == null)));
    }
  }, [isOpen, existing, records]);

  // Records this invoice can bill: those not on any invoice, and those already on it.
  const candidates = useMemo(() => {
    const q = search.trim().toLowerCase();
    return records
      .filter((r) => r.invoice_number == null || r.invoice_number === trimmed)
      .filter((r) => {
        if (!q || picked.has(r.id)) return true;
        const a = assets.get(r.asset_id);
        return [r.asset_tag, a?.description, r.certificate_number, r.service_type_name, a?.model_number]
          .filter(Boolean)
          .some((s) => s!.toLowerCase().includes(q));
      })
      .sort((x, y) => x.cal_date.localeCompare(y.cal_date) || x.asset_tag.localeCompare(y.asset_tag));
  }, [records, trimmed, search, picked, assets]);

  const lines = [...picked.entries()]
    .map(([id, p]) => ({ record: records.find((r) => r.id === id)!, ...p }))
    .filter((l) => l.record)
    .sort((x, y) => (x.line ?? Infinity) - (y.line ?? Infinity));
  const invoiceTotal = lines.reduce((s, l) => s + (l.price == null ? 0 : toCents(l.price)), 0);
  const recordedTotal = lines.reduce((s, l) => s + (l.record.cost_cents ?? 0), 0);
  const changing = lines.filter((l) => l.price != null && toCents(l.price) !== l.record.cost_cents).length;

  const problems: string[] = [];
  if (!trimmed) problems.push("Enter the invoice number.");
  if (!date) problems.push("Enter the invoice date.");
  if (lines.length === 0 && existing.length === 0) problems.push("Select the service records on this invoice.");
  if (lines.some((l) => l.line == null || l.line < 1)) problems.push("Give every selected record a line number.");
  const used = lines.map((l) => l.line).filter((n) => n != null);
  const dupes = [...new Set(used.filter((n, i) => used.indexOf(n) !== i))];
  if (dupes.length) problems.push(`Line ${dupes.join(", ")} is used more than once.`);
  if (lines.some((l) => l.price == null)) problems.push("Enter the invoiced price of every selected record.");

  function toggle(r: CalibrationRecord, on: boolean) {
    setPicked((was) => {
      const next = new Map(was);
      if (!on) next.delete(r.id);
      else {
        const taken = [...was.values()].map((p) => p.line ?? 0);
        next.set(r.id, {
          line: Math.max(0, ...taken) + 1,
          price: r.cost_cents == null ? null : r.cost_cents / 100,
        });
      }
      return next;
    });
  }

  function change(id: number, patch: Partial<Pick>) {
    setPicked((was) => {
      const next = new Map(was);
      const p = next.get(id);
      if (p) next.set(id, { ...p, ...patch });
      return next;
    });
  }

  async function save() {
    setTried(true);
    if (problems.length || !date) return;
    setBusy(true);
    setError(undefined);
    try {
      const saved = await api.saveInvoice({
        provider_id: provider.id,
        invoice_number: trimmed,
        invoice_date: date,
        lines: lines.map((l) => ({
          calibration_id: l.record.id,
          expected_version: l.record.row_version,
          line: l.line!,
          cost_cents: toCents(l.price!),
        })),
      });
      toast.success(
        saved.length
          ? `Saved invoice ${trimmed}: ${saved.length} line${saved.length === 1 ? "" : "s"}, ${formatCents(
              saved.reduce((s, r) => s + (r.cost_cents ?? 0), 0),
            )}`
          : `Took every record off invoice ${trimmed}`,
      );
      onOpenChange(false);
      onSaved?.();
    } catch (e) {
      setError(e instanceof ApiError ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  const removing = lines.length === 0 && existing.length > 0;

  return (
    <Modal>
      <Modal.Backdrop isOpen={isOpen} onOpenChange={onOpenChange} isDismissable={!busy}>
        <Modal.Container size="lg" scroll="inside">
          <Modal.Dialog className="max-w-5xl">
            <Modal.CloseTrigger />
            <Modal.Header>
              <Modal.Heading>
                {existing.length ? `Invoice ${trimmed}` : "Enter invoice"} · {provider.name}
              </Modal.Heading>
              <p className="text-sm text-muted">
                {existing.length
                  ? `Already entered with ${existing.length} line${existing.length === 1 ? "" : "s"}. Changes replace it.`
                  : "Select the service records the invoice bills, give each its line number, and check the prices."}
              </p>
            </Modal.Header>
            <Modal.Body>
              <form
                id="invoice"
                noValidate
                className="flex flex-col gap-4 p-1"
                onSubmit={(e) => {
                  e.preventDefault();
                  void save();
                }}
              >
                <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
                  <TextInput
                    label="Invoice number"
                    value={number}
                    onChange={setNumber}
                    isRequired
                    autoFocus={!invoiceNumber}
                    maxLength={100}
                  />
                  <DateInput label="Invoice date" value={date} onChange={setDate} isRequired />
                  <TextInput label="Find" value={search} onChange={setSearch} placeholder="Asset tag, description, certificate" />
                </div>

                {candidates.length === 0 ? (
                  <p className="rounded-xl border border-dashed border-separator px-4 py-8 text-center text-sm text-muted">
                    {search
                      ? "No records match."
                      : `Every service record by ${provider.name} is already on an invoice.`}
                  </p>
                ) : (
                  <div className="overflow-x-auto rounded-xl border border-separator">
                    <table className="w-full border-collapse text-sm">
                      <thead className="bg-surface-secondary text-left text-muted">
                        <tr>
                          <th className="w-10 px-3 py-2">
                            <span className="sr-only">On this invoice</span>
                          </th>
                          <th className="px-2 py-2 font-medium">Line</th>
                          <th className="px-2 py-2 font-medium">Service date</th>
                          <th className="px-2 py-2 font-medium">Instrument</th>
                          <th className="px-2 py-2 font-medium">Service</th>
                          <th className="px-2 py-2 text-right font-medium">Recorded</th>
                          <th className="px-2 py-2 font-medium">Invoiced price</th>
                        </tr>
                      </thead>
                      <tbody>
                        {candidates.map((r) => {
                          const p = picked.get(r.id);
                          const a = assets.get(r.asset_id);
                          const diff = p && p.price != null && r.cost_cents != null ? toCents(p.price) - r.cost_cents : 0;
                          return (
                            <tr
                              key={r.id}
                              className={`border-t border-separator align-middle ${p ? "bg-accent-soft/40" : ""}`}
                            >
                              <td className="px-3 py-2">
                                <Checkbox
                                  variant="secondary"
                                  isSelected={!!p}
                                  onChange={(on) => toggle(r, on)}
                                  aria-label={`Include ${r.asset_tag} on ${formatDate(r.cal_date)}`}
                                >
                                  <Checkbox.Content>
                                    <Checkbox.Control className="size-5 shrink-0 border-2 border-muted">
                                      <Checkbox.Indicator />
                                    </Checkbox.Control>
                                  </Checkbox.Content>
                                </Checkbox>
                              </td>
                              <td className="w-24 px-2 py-1.5">
                                {p && (
                                  <NumberInput
                                    aria-label={`Line number for ${r.asset_tag}`}
                                    value={p.line}
                                    onChange={(v) => change(r.id, { line: v })}
                                    minValue={1}
                                    step={1}
                                    formatOptions={{ maximumFractionDigits: 0, useGrouping: false }}
                                    isInvalid={tried && (p.line == null || dupes.includes(p.line))}
                                  />
                                )}
                              </td>
                              <td className="px-2 py-2 whitespace-nowrap">{formatDate(r.cal_date)}</td>
                              <td className="px-2 py-2">
                                <span className="font-medium">{r.asset_tag}</span>
                                {a?.description && <div className="text-xs text-muted">{a.description}</div>}
                              </td>
                              <td className="px-2 py-2">
                                {r.service_type_name}
                                {r.certificate_number && (
                                  <div className="text-xs text-muted">Cert. {r.certificate_number}</div>
                                )}
                              </td>
                              <td className="px-2 py-2 text-right whitespace-nowrap tabular-nums">
                                {r.cost_cents == null ? <span className="text-muted">{EMPTY}</span> : formatCents(r.cost_cents)}
                              </td>
                              <td className="w-44 px-2 py-1.5">
                                {p && (
                                  <>
                                    <MoneyInput
                                      aria-label={`Invoiced price for ${r.asset_tag}`}
                                      value={p.price}
                                      onChange={(v) => change(r.id, { price: v })}
                                      isInvalid={tried && p.price == null}
                                    />
                                    {diff !== 0 && (
                                      <div className="mt-0.5 text-xs font-medium text-warning">
                                        {signedCents(diff)} vs recorded
                                      </div>
                                    )}
                                    {r.cost_cents == null && p.price != null && (
                                      <div className="mt-0.5 text-xs text-muted">No cost recorded</div>
                                    )}
                                  </>
                                )}
                              </td>
                            </tr>
                          );
                        })}
                      </tbody>
                    </table>
                  </div>
                )}

                {tried && problems.length > 0 && (
                  <ul className="list-disc pl-5 text-sm text-danger">
                    {problems.map((p) => (
                      <li key={p}>{p}</li>
                    ))}
                  </ul>
                )}
                {error && (
                  <Alert status="danger">
                    <Alert.Indicator />
                    <Alert.Content>
                      <Alert.Description>{error}</Alert.Description>
                    </Alert.Content>
                  </Alert>
                )}
              </form>
            </Modal.Body>
            <Modal.Footer className="flex-wrap items-center">
              <div className="mr-auto text-sm">
                {lines.length > 0 ? (
                  <>
                    <span className="font-medium">
                      {lines.length} line{lines.length === 1 ? "" : "s"} · invoice total {formatCents(invoiceTotal)}
                    </span>
                    <span className="text-muted"> · recorded {formatCents(recordedTotal)}</span>
                    {changing > 0 && (
                      <div className="text-xs text-warning">
                        {changing} recorded price{changing === 1 ? "" : "s"} will change to match the invoice.
                      </div>
                    )}
                  </>
                ) : removing ? (
                  <span className="text-warning">Saving takes every record off this invoice.</span>
                ) : (
                  <span className="text-muted">No records selected.</span>
                )}
              </div>
              <Button slot="close" variant="tertiary" isDisabled={busy}>
                Cancel
              </Button>
              <Button type="submit" form="invoice" isPending={busy} variant={removing ? "danger" : "primary"}>
                {removing ? "Remove invoice" : existing.length ? "Save changes" : "Save invoice"}
              </Button>
            </Modal.Footer>
          </Modal.Dialog>
        </Modal.Container>
      </Modal.Backdrop>
    </Modal>
  );
}
