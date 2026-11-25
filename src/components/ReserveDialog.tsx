// Reserving an instrument: choose the days, see straight away whether they're free, and if
// not, what to do instead: other dates, asking the holder of an earlier reservation to end
// it early, or (if it's checked out) requesting the instrument from whoever has it.

import { Alert, Button, Modal, toast } from "@heroui/react";
import { CalendarCheck, CalendarClock, Hand } from "lucide-react";
import { useEffect, useState } from "react";
import type { Asset } from "../bindings/Asset";
import type { Reservation } from "../bindings/Reservation";
import type { ReservationCheck } from "../bindings/ReservationCheck";
import { ApiError, api } from "../lib/api";
import { useAuth } from "../lib/auth";
import { formatDate } from "../lib/format";
import { ownerRequestsChanged } from "../lib/ownerRequests";
import { addDays, reservationsChanged, spanDays, todayYmd } from "../lib/reservations";
import { DateInput, SelectInput, TextInput } from "./fields";

function span(start: string, end: string): string {
  return start === end ? formatDate(start) : `${formatDate(start)} – ${formatDate(end)}`;
}

export function ReserveDialog({
  isOpen,
  onOpenChange,
  asset,
  choices,
  editing = null,
  onDone,
}: {
  isOpen: boolean;
  onOpenChange: (open: boolean) => void;
  /** The instrument to reserve, or null to choose one from `choices`. */
  asset: Asset | null;
  choices?: Asset[];
  /** A reservation to change instead of making a new one. */
  editing?: Reservation | null;
  onDone?: () => void;
}) {
  const { user } = useAuth();
  const [assetId, setAssetId] = useState<number | null>(null);
  const [start, setStart] = useState<string | null>(null);
  const [end, setEnd] = useState<string | null>(null);
  const [note, setNote] = useState("");
  const [check, setCheck] = useState<ReservationCheck>();
  const [checkError, setCheckError] = useState<string>();
  const [error, setError] = useState<string>();
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!isOpen) return;
    setAssetId(editing?.asset_id ?? asset?.id ?? null);
    setStart(editing?.start_date ?? todayYmd());
    setEnd(editing?.end_date ?? todayYmd());
    setNote(editing?.note ?? "");
    setCheck(undefined);
    setCheckError(undefined);
    setError(undefined);
  }, [isOpen, asset, editing]);
  // Once a reservation has started, only its end can move.
  const started = !!editing && editing.start_date <= todayYmd();

  // Check the dates as they change (a moment after the last change).
  useEffect(() => {
    setCheck(undefined);
    setCheckError(undefined);
    if (!isOpen || assetId == null || !start || !end) return;
    if (end < start) return setCheckError("The reservation must end on or after the day it starts.");
    if (start < todayYmd() && !(editing && start === editing.start_date)) {
      return setCheckError("A reservation can't start in the past.");
    }
    if (end < todayYmd()) return setCheckError("A reservation can't end in the past.");
    let live = true;
    const timer = window.setTimeout(() => {
      api
        .checkReservation(assetId, start, end, editing?.id ?? null)
        .then((c) => live && setCheck(c))
        .catch((e) => live && setCheckError(e instanceof ApiError ? e.message : String(e)));
    }, 250);
    return () => {
      live = false;
      window.clearTimeout(timer);
    };
  }, [isOpen, assetId, start, end, editing]);

  const chosen = asset ?? choices?.find((a) => a.id === assetId) ?? null;
  const free = !!check && !check.checked_out_to && check.conflicts.length === 0;
  const days = start && end && end >= start ? spanDays(start, end) : 1;

  async function reserve(asks: number | null = null) {
    if (assetId == null || !start || !end) return;
    setBusy(true);
    setError(undefined);
    try {
      if (editing) {
        const changed = await api.updateReservation(editing.id, start, end, note);
        toast.success(`Reservation of ${changed.asset_tag} now ${span(changed.start_date, changed.end_date)}`);
        reservationsChanged();
        onOpenChange(false);
        onDone?.();
        return;
      }
      const r = await api.createReservation(assetId, start, end, note, asks);
      toast.success(
        r.status === "pending"
          ? `Asked ${r.asks_user_name} to end their reservation early. You'll see their answer on the Dashboard.`
          : `Reserved ${r.asset_tag}, ${span(r.start_date, r.end_date)}`,
      );
      reservationsChanged();
      onOpenChange(false);
      onDone?.();
    } catch (e) {
      setError(e instanceof ApiError ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  async function requestInstrument() {
    if (assetId == null) return;
    setBusy(true);
    setError(undefined);
    try {
      const r = await api.requestInstrument(assetId, note);
      toast.success(
        r.approver_name
          ? `Asked ${r.approver_name} to hand over ${r.asset_tag}.`
          : `Requested ${r.asset_tag}. A Technician or Administrator will approve it.`,
      );
      ownerRequestsChanged();
      onOpenChange(false);
      onDone?.();
    } catch (e) {
      setError(e instanceof ApiError ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Modal>
      <Modal.Backdrop isOpen={isOpen} onOpenChange={onOpenChange} isDismissable={!busy}>
        <Modal.Container size="md" scroll="inside">
          <Modal.Dialog>
            <Modal.CloseTrigger />
            <Modal.Header>
              <Modal.Heading>
                {editing
                  ? `Change reservation of ${editing.asset_tag}`
                  : chosen
                    ? `Reserve ${chosen.asset_tag}`
                    : "Reserve an instrument"}
              </Modal.Heading>
            </Modal.Header>
            <Modal.Body>
              <form
                id="reserve"
                noValidate
                className="flex flex-col gap-4 p-1"
                onSubmit={(e) => {
                  e.preventDefault();
                  if (free) void reserve();
                }}
              >
                {(editing || chosen) && (
                  <p className="text-sm text-muted">{editing ? editing.asset_description : chosen?.description}</p>
                )}
                {!asset && !editing && choices && (
                  <SelectInput<number>
                    label="Instrument"
                    value={assetId}
                    onChange={setAssetId}
                    options={choices.map((a) => ({ id: a.id, label: a.asset_tag, description: a.description }))}
                    isRequired
                  />
                )}
                <div className="grid grid-cols-2 gap-3">
                  <DateInput
                    label="From"
                    value={start}
                    onChange={setStart}
                    isRequired
                    isDisabled={started}
                    description={started ? "It has started, so only the end can change." : undefined}
                  />
                  <DateInput label="To" value={end} onChange={setEnd} isRequired />
                </div>
                <TextInput
                  label="Note"
                  value={note}
                  onChange={setNote}
                  placeholder="e.g. Audit at plant 2"
                  description="Optional. Shown on the calendar."
                />

                {checkError && <p className="text-sm text-danger">{checkError}</p>}
                {free && (
                  <p className="flex items-center gap-2 text-sm text-success">
                    <CalendarCheck className="size-4" /> {editing ? "These dates are free." : "Available for these dates."}
                  </p>
                )}
                {check?.checked_out_to && (
                  <Alert status="warning">
                    <Alert.Indicator />
                    <Alert.Content>
                      <Alert.Title>Checked out to {check.checked_out_to}</Alert.Title>
                      <Alert.Description>
                        It has to be returned before it can be reserved. You can ask for it instead.
                        {!user.owner_id && " (Your account needs linking to an Owner entry first: ask an administrator.)"}
                      </Alert.Description>
                      <div className="mt-2">
                        <Button size="sm" variant="secondary" isDisabled={busy || !user.owner_id} onPress={() => void requestInstrument()}>
                          <Hand className="size-4" /> Request instrument
                        </Button>
                      </div>
                    </Alert.Content>
                  </Alert>
                )}
                {check && !check.checked_out_to && check.conflicts.length > 0 && (
                  <Alert status="warning">
                    <Alert.Indicator />
                    <Alert.Content>
                      <Alert.Title>Already reserved</Alert.Title>
                      <Alert.Description>
                        <ul className="mt-1 list-disc pl-4">
                          {check.conflicts.map((c) => (
                            <li key={c.id}>
                              {span(c.start_date, c.end_date)} by {c.user_name}
                              {c.status === "pending" ? " (waiting for an answer)" : ""}
                            </li>
                          ))}
                        </ul>
                      </Alert.Description>
                      <div className="mt-2 flex flex-wrap gap-2">
                        {check.next_free_start && check.next_free_start !== start && !started && (
                          <Button
                            size="sm"
                            variant="secondary"
                            isDisabled={busy}
                            onPress={() => {
                              setStart(check.next_free_start!);
                              setEnd(addDays(check.next_free_start!, days - 1));
                            }}
                          >
                            <CalendarClock className="size-4" /> Use {span(check.next_free_start, addDays(check.next_free_start, days - 1))}
                          </Button>
                        )}
                        {!editing && check.can_ask && check.can_ask.user_id !== user.id && start && (
                          <Button size="sm" variant="secondary" isDisabled={busy} onPress={() => void reserve(check.can_ask!.id)}>
                            <Hand className="size-4" /> Ask {check.can_ask.user_name} to end theirs on{" "}
                            {formatDate(addDays(start, -1))}
                          </Button>
                        )}
                      </div>
                    </Alert.Content>
                  </Alert>
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
            <Modal.Footer>
              <Button slot="close" variant="tertiary" isDisabled={busy}>
                Cancel
              </Button>
              <Button type="submit" form="reserve" isPending={busy} isDisabled={!free}>
                {editing ? "Save changes" : "Reserve"}
              </Button>
            </Modal.Footer>
          </Modal.Dialog>
        </Modal.Container>
      </Modal.Backdrop>
    </Modal>
  );
}
