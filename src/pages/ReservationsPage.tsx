// Reservations: instruments free to reserve, your own reservations, and a month calendar of
// everyone's (past ones stay on it). Reserving books the days; the instrument is still
// handed over (checked out) when it starts.

import { Button, Card, Modal, SearchField, toast } from "@heroui/react";
import { CalendarDays, CalendarPlus, ChevronLeft, ChevronRight } from "lucide-react";
import { useDeferredValue, useMemo, useState } from "react";
import { Link } from "react-router";
import type { Asset } from "../bindings/Asset";
import type { Reservation } from "../bindings/Reservation";
import { EmptyNote, ErrorAlert, Loading, PageHeader, StatusChip } from "../components/common";
import { ConfirmDialog } from "../components/ConfirmDialog";
import { ReserveDialog } from "../components/ReserveDialog";
import { api } from "../lib/api";
import { useAuth } from "../lib/auth";
import { EMPTY, formatDate } from "../lib/format";
import { addDays, fromYmd, reservationsChanged, todayYmd, toYmd } from "../lib/reservations";
import { useAsync } from "../lib/useAsync";

/** Not reservable: retired or away for service. */
const AWAY = new Set(["retired", "out_for_calibration", "out_for_repair"]);
const WEEKDAYS = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
/** Reservations shown in a day box before "+ more". */
const PER_DAY = 3;

function span(r: Reservation): string {
  return r.start_date === r.end_date
    ? formatDate(r.start_date)
    : `${formatDate(r.start_date)} – ${formatDate(r.end_date)}`;
}

export function ReservationsPage() {
  const { user } = useAuth();
  const today = todayYmd();
  const [month, setMonth] = useState(() => today.slice(0, 7)); // YYYY-MM
  const [search, setSearch] = useState("");
  const query = useDeferredValue(search);
  const [reserving, setReserving] = useState<{ open: boolean; asset: Asset | null }>({ open: false, asset: null });
  const [shown, setShown] = useState<Reservation | null>(null);
  const [dayList, setDayList] = useState<string | null>(null);
  const [editing, setEditing] = useState<Reservation | null>(null);
  const [cancelling, setCancelling] = useState<Reservation | null>(null);

  // The calendar shows whole weeks around the month.
  const first = fromYmd(`${month}-01`);
  const gridStart = addDays(`${month}-01`, -first.getDay());
  const lastOfMonth = toYmd(new Date(first.getFullYear(), first.getMonth() + 1, 0, 12));
  const gridEnd = addDays(lastOfMonth, 6 - fromYmd(lastOfMonth).getDay());

  const items = useAsync(() => api.listAssets({ kind: "instrument", include_archived: false }), []);
  const calendar = useAsync(() => api.listReservations({ from: gridStart, to: gridEnd }), [gridStart, gridEnd]);
  // Today onwards, for what's free now, each item's next booking, and yours.
  const upcoming = useAsync(() => api.listReservations({ from: today }), [today]);
  const reload = () => {
    items.reload();
    calendar.reload();
    upcoming.reload();
    reservationsChanged();
  };

  const coming = (upcoming.data ?? []).filter((r) => r.status === "active");
  const reservedToday = new Set(coming.filter((r) => r.start_date <= today).map((r) => r.asset_id));
  const nextFor = new Map<number, Reservation>();
  for (const r of coming) if (r.start_date > today && !nextFor.has(r.asset_id)) nextFor.set(r.asset_id, r);
  const reservable = (items.data ?? [])
    .filter((a) => a.owner_id == null && !AWAY.has(a.status_role ?? "") && !reservedToday.has(a.id))
    .filter((a) => {
      const q = query.toLowerCase();
      return !q || [a.asset_tag, a.description, a.model_number, a.manufacturer_name].some((v) => v?.toLowerCase().includes(q));
    })
    .sort((x, y) => x.asset_tag.localeCompare(y.asset_tag, undefined, { numeric: true }));
  const mine = (upcoming.data ?? []).filter((r) => r.user_id === user.id);
  // Everything except checked-out, retired, and away items can be reserved for later dates.
  const choices = (items.data ?? []).filter((a) => a.owner_id == null && !AWAY.has(a.status_role ?? ""));

  const byDay = useMemo(() => {
    const map = new Map<string, Reservation[]>();
    for (const r of calendar.data ?? []) {
      for (let d = r.start_date < gridStart ? gridStart : r.start_date; d <= r.end_date && d <= gridEnd; d = addDays(d, 1)) {
        map.set(d, [...(map.get(d) ?? []), r]);
      }
    }
    return map;
  }, [calendar.data, gridStart, gridEnd]);

  const days: string[] = [];
  for (let d = gridStart; d <= gridEnd; d = addDays(d, 1)) days.push(d);
  const monthName = first.toLocaleDateString(undefined, { month: "long", year: "numeric" });
  const shiftMonth = (by: number) => {
    const d = new Date(first.getFullYear(), first.getMonth() + by, 1, 12);
    setMonth(toYmd(d).slice(0, 7));
  };

  return (
    <>
      <PageHeader
        title="Reservations"
        description="Book an instrument for the days you need it. It's handed over (checked out to you) when the reservation starts."
        actions={
          <Button onPress={() => setReserving({ open: true, asset: null })} isDisabled={!items.data}>
            <CalendarPlus className="size-4" /> Reserve…
          </Button>
        }
      />
      {items.error && <ErrorAlert error={items.error} onRetry={items.reload} />}

      <div className="grid grid-cols-1 items-start gap-6 xl:grid-cols-[minmax(0,2fr)_minmax(0,3fr)]">
        <div className="flex min-w-0 flex-col gap-6">
          <Card>
            <Card.Header className="flex-row items-center justify-between gap-3">
              <div>
                <Card.Title>Free to reserve now</Card.Title>
                <Card.Description>Not checked out or reserved today. Later dates may already be booked.</Card.Description>
              </div>
            </Card.Header>
            <Card.Content className="flex flex-col gap-3">
              <SearchField value={search} onChange={setSearch} aria-label="Search">
                <SearchField.Group>
                  <SearchField.SearchIcon />
                  <SearchField.Input placeholder="Search tag, description, model…" />
                  <SearchField.ClearButton />
                </SearchField.Group>
              </SearchField>
              {items.loading && !items.data ? (
                <Loading />
              ) : reservable.length === 0 ? (
                <EmptyNote icon={<CalendarDays className="size-5" />} title={query ? "No matches" : "Nothing free right now"} />
              ) : (
                <ul className="flex max-h-[28rem] flex-col divide-y divide-separator overflow-y-auto">
                  {reservable.map((a) => {
                    const next = nextFor.get(a.id);
                    return (
                      <li key={a.id} className="flex items-center gap-3 py-2">
                        <div className="min-w-0 flex-1">
                          <Link to={`/assets/${a.id}`} className="text-sm font-medium text-accent hover:underline">
                            {a.asset_tag}
                          </Link>
                          <div className="truncate text-xs text-muted">
                            {a.description}
                            {next ? ` · next reserved ${span(next)}` : ""}
                          </div>
                        </div>
                        <Button size="sm" variant="secondary" onPress={() => setReserving({ open: true, asset: a })}>
                          Reserve
                        </Button>
                      </li>
                    );
                  })}
                </ul>
              )}
            </Card.Content>
          </Card>

          <Card>
            <Card.Header>
              <Card.Title>Your reservations</Card.Title>
            </Card.Header>
            <Card.Content>
              {mine.length === 0 ? (
                <p className="text-sm text-muted">You have no current or upcoming reservations.</p>
              ) : (
                <ul className="flex flex-col divide-y divide-separator">
                  {mine.map((r) => (
                    <li key={r.id} className="flex items-center gap-3 py-2 text-sm">
                      <button type="button" className="min-w-0 flex-1 text-left" onClick={() => setShown(r)}>
                        <span className="font-medium">{r.asset_tag}</span>
                        <span className="text-muted"> · {span(r)}</span>
                        {r.status === "pending" && (
                          <div className="text-xs text-warning">Waiting for {r.asks_user_name} to end theirs early</div>
                        )}
                      </button>
                      {r.start_date <= today && r.status === "active" && <StatusChip color="success">Now</StatusChip>}
                      {r.status === "active" && (
                        <Button size="sm" variant="ghost" onPress={() => setEditing(r)}>
                          Edit
                        </Button>
                      )}
                      <Button size="sm" variant="ghost" className="text-danger" onPress={() => setCancelling(r)}>
                        Cancel
                      </Button>
                    </li>
                  ))}
                </ul>
              )}
            </Card.Content>
          </Card>
        </div>

        <Card className="min-w-0">
          <Card.Header className="flex-row items-center justify-between gap-3">
            <Card.Title className="text-lg">{monthName}</Card.Title>
            <div className="flex items-center gap-1">
              <Button size="sm" variant="ghost" isIconOnly aria-label="Previous month" onPress={() => shiftMonth(-1)}>
                <ChevronLeft className="size-4" />
              </Button>
              <Button size="sm" variant="ghost" onPress={() => setMonth(today.slice(0, 7))}>
                Today
              </Button>
              <Button size="sm" variant="ghost" isIconOnly aria-label="Next month" onPress={() => shiftMonth(1)}>
                <ChevronRight className="size-4" />
              </Button>
            </div>
          </Card.Header>
          <Card.Content>
            {calendar.error && <ErrorAlert error={calendar.error} onRetry={calendar.reload} />}
            <div className="grid grid-cols-7 overflow-hidden rounded-xl border border-separator text-xs" role="grid" aria-label={`Reservations, ${monthName}`}>
              {WEEKDAYS.map((w) => (
                <div key={w} className="border-b border-separator bg-default/40 px-2 py-1.5 font-medium text-muted" role="columnheader">
                  {w}
                </div>
              ))}
              {days.map((d, i) => {
                const list = byDay.get(d) ?? [];
                const inMonth = d.startsWith(month);
                const past = d < today;
                return (
                  <div
                    key={d}
                    role="gridcell"
                    className={`flex min-h-24 flex-col gap-1 p-1.5 ${i % 7 ? "border-l" : ""} ${i >= 7 ? "border-t" : ""} border-separator ${inMonth ? "" : "bg-default/20"} ${past ? "opacity-60" : ""}`}
                  >
                    <span
                      className={`self-start rounded-full px-1.5 ${d === today ? "bg-accent font-semibold text-accent-foreground" : inMonth ? "" : "text-muted"}`}
                    >
                      {fromYmd(d).getDate()}
                    </span>
                    {list.slice(0, PER_DAY).map((r) => {
                      const mineR = r.user_id === user.id;
                      return (
                        <button
                          key={r.id}
                          type="button"
                          onClick={() => setShown(r)}
                          title={`${r.asset_tag} · ${r.user_name} · ${span(r)}${r.note ? ` · ${r.note}` : ""}`}
                          className={`truncate rounded px-1 py-0.5 text-left text-[11px] leading-tight ${
                            r.status === "pending"
                              ? "border border-dashed border-warning text-warning"
                              : mineR
                                ? "bg-accent-soft text-accent-soft-foreground"
                                : "bg-default text-foreground"
                          }`}
                        >
                          {r.asset_tag} · {r.user_name}
                        </button>
                      );
                    })}
                    {list.length > PER_DAY && (
                      <button type="button" className="text-left text-[11px] text-muted hover:underline" onClick={() => setDayList(d)}>
                        + {list.length - PER_DAY} more
                      </button>
                    )}
                  </div>
                );
              })}
            </div>
            <p className="mt-2 flex flex-wrap gap-x-4 gap-y-1 text-xs text-muted">
              <span className="flex items-center gap-1.5">
                <span className="size-2.5 rounded-sm bg-accent-soft" /> Yours
              </span>
              <span className="flex items-center gap-1.5">
                <span className="size-2.5 rounded-sm bg-default" /> Others
              </span>
              <span className="flex items-center gap-1.5">
                <span className="size-2.5 rounded-sm border border-dashed border-warning" /> Waiting for an answer
              </span>
            </p>
          </Card.Content>
        </Card>
      </div>

      <ReserveDialog
        isOpen={reserving.open}
        onOpenChange={(open) => setReserving((r) => ({ ...r, open }))}
        asset={reserving.asset}
        choices={choices}
        onDone={reload}
      />
      <ReserveDialog
        isOpen={!!editing}
        onOpenChange={(open) => !open && setEditing(null)}
        asset={null}
        editing={editing}
        onDone={reload}
      />
      <ConfirmDialog
        isOpen={!!cancelling}
        onOpenChange={(o) => !o && setCancelling(null)}
        title={cancelling ? `Cancel your reservation of ${cancelling.asset_tag}?` : ""}
        confirmLabel="Cancel reservation"
        cancelLabel="Keep reservation"
        onConfirm={async () => {
          if (!cancelling) return;
          await api.cancelReservation(cancelling.id);
          toast.success("Reservation cancelled");
          setCancelling(null);
          reload();
        }}
      >
        {cancelling
          ? `${span(cancelling)}. It comes off the calendar, and the instrument is free for others again.`
          : null}
      </ConfirmDialog>
      <DayDialog day={dayList} list={dayList ? (byDay.get(dayList) ?? []) : []} onClose={() => setDayList(null)} onPick={setShown} />
      <ReservationDialog
        reservation={shown}
        onClose={() => setShown(null)}
        onChanged={() => {
          setShown(null);
          reload();
        }}
      />
    </>
  );
}

function DayDialog({
  day,
  list,
  onClose,
  onPick,
}: {
  day: string | null;
  list: Reservation[];
  onClose: () => void;
  onPick: (r: Reservation) => void;
}) {
  return (
    <Modal>
      <Modal.Backdrop isOpen={!!day} onOpenChange={(o) => !o && onClose()}>
        <Modal.Container size="sm">
          <Modal.Dialog>
            <Modal.CloseTrigger />
            <Modal.Header>
              <Modal.Heading>{day ? formatDate(day) : ""}</Modal.Heading>
            </Modal.Header>
            <Modal.Body>
              <ul className="flex flex-col divide-y divide-separator text-sm">
                {list.map((r) => (
                  <li key={r.id}>
                    <button
                      type="button"
                      className="w-full py-2 text-left hover:underline"
                      onClick={() => {
                        onClose();
                        onPick(r);
                      }}
                    >
                      <span className="font-medium">{r.asset_tag}</span> · {r.user_name} · {span(r)}
                    </button>
                  </li>
                ))}
              </ul>
            </Modal.Body>
          </Modal.Dialog>
        </Modal.Container>
      </Modal.Backdrop>
    </Modal>
  );
}

/** One reservation's details, with what you can do about it. */
export function ReservationDialog({
  reservation: r,
  onClose,
  onChanged,
}: {
  reservation: Reservation | null;
  onClose: () => void;
  onChanged: () => void;
}) {
  const { user, can } = useAuth();
  const editor = can("edit_records");
  const [confirm, setConfirm] = useState<"cancel" | "accept" | "decline" | null>(null);
  if (!r && !confirm) return null;
  const today = todayYmd();
  const canCancel = !!r && (r.user_id === user.id || editor) && r.end_date >= today && (r.status === "active" || r.status === "pending");
  const canAnswer = !!r && r.status === "pending" && (r.asks_user_id === user.id || editor);

  return (
    <>
      <Modal>
        <Modal.Backdrop isOpen={!!r && !confirm} onOpenChange={(o) => !o && onClose()}>
          <Modal.Container size="sm">
            <Modal.Dialog>
              <Modal.CloseTrigger />
              <Modal.Header>
                <Modal.Heading>{r ? `${r.asset_tag} reservation` : ""}</Modal.Heading>
              </Modal.Header>
              <Modal.Body>
                {r && (
                  <dl className="grid grid-cols-[110px_1fr] gap-x-3 gap-y-2 text-sm">
                    <dt className="text-muted">Instrument</dt>
                    <dd>
                      <Link to={`/assets/${r.asset_id}`} className="text-accent hover:underline">
                        {r.asset_tag}
                      </Link>{" "}
                      · {r.asset_description}
                    </dd>
                    <dt className="text-muted">For</dt>
                    <dd>{r.user_name}</dd>
                    <dt className="text-muted">Dates</dt>
                    <dd>{span(r)}</dd>
                    <dt className="text-muted">Status</dt>
                    <dd>
                      {r.status === "pending"
                        ? `Waiting for ${r.asks_user_name} to end their reservation on ${formatDate(addDays(r.start_date, -1))}`
                        : r.status === "active"
                          ? r.end_date < today
                            ? "Finished"
                            : r.start_date <= today
                              ? "Now"
                              : "Booked"
                          : r.status === "declined"
                            ? "Declined"
                            : "Cancelled"}
                    </dd>
                    <dt className="text-muted">With now</dt>
                    <dd>{r.asset_owner_name ?? "In the store (not checked out)"}</dd>
                    <dt className="text-muted">Note</dt>
                    <dd>{r.note ?? EMPTY}</dd>
                    {r.decision_note && (
                      <>
                        <dt className="text-muted">Answer</dt>
                        <dd>{r.decision_note}</dd>
                      </>
                    )}
                  </dl>
                )}
              </Modal.Body>
              <Modal.Footer>
                {canAnswer && (
                  <>
                    <Button variant="secondary" onPress={() => setConfirm("decline")}>
                      Decline
                    </Button>
                    <Button onPress={() => setConfirm("accept")}>End mine early</Button>
                  </>
                )}
                {canCancel && (
                  <Button variant="danger-soft" onPress={() => setConfirm("cancel")}>
                    Cancel reservation
                  </Button>
                )}
              </Modal.Footer>
            </Modal.Dialog>
          </Modal.Container>
        </Modal.Backdrop>
      </Modal>
      <ConfirmDialog
        isOpen={!!confirm}
        onOpenChange={(o) => !o && setConfirm(null)}
        title={
          confirm === "cancel"
            ? `Cancel this reservation of ${r?.asset_tag}?`
            : confirm === "accept"
              ? `End your reservation of ${r?.asset_tag} early?`
              : `Decline ${r?.user_name}'s request?`
        }
        confirmLabel={confirm === "cancel" ? "Cancel reservation" : confirm === "accept" ? "End mine early" : "Decline"}
        cancelLabel={confirm === "cancel" ? "Keep reservation" : "Cancel"}
        tone={confirm === "accept" ? "accent" : "danger"}
        askReason={confirm !== "cancel"}
        reasonLabel="Note (optional)"
        onConfirm={async (note) => {
          if (!r) return;
          if (confirm === "cancel") {
            await api.cancelReservation(r.id);
            toast.success("Reservation cancelled");
          } else {
            await api.answerReservationRequest(r.id, confirm === "accept", note);
            toast.success(confirm === "accept" ? `Yours now ends ${formatDate(addDays(r.start_date, -1))}` : "Request declined");
          }
          setConfirm(null);
          onChanged();
        }}
      >
        {r && confirm === "accept"
          ? `${r.user_name} would like it from ${formatDate(r.start_date)}. Yours will end on ${formatDate(addDays(r.start_date, -1))}.`
          : r && confirm === "decline"
            ? `${r.user_name}'s reservation (${span(r)}) won't go ahead. Yours stays as it is.`
            : r
              ? `${span(r)}, for ${r.user_name}.`
              : null}
      </ConfirmDialog>
    </>
  );
}
