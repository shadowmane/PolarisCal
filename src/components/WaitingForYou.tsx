// Dashboard: everything waiting on the signed-in user's answer or action. Requests
// addressed to them (an instrument they hold, or for Technicians and Administrators,
// requests for no one in particular), requests to end their reservations early, and for
// Technicians and Administrators, reservations to hand over or take back.

import { Button, Card, toast } from "@heroui/react";
import { CalendarClock, CircleCheck, Hand, LogIn, LogOut, PackageCheck } from "lucide-react";
import { type ReactNode, useState } from "react";
import { Link } from "react-router";
import type { OwnerRequest } from "../bindings/OwnerRequest";
import type { Reservation } from "../bindings/Reservation";
import { api } from "../lib/api";
import { useAuth } from "../lib/auth";
import { formatDate, formatDateTime } from "../lib/format";
import { ownerRequestsChanged } from "../lib/ownerRequests";
import { addDays, reservationsChanged, todayYmd } from "../lib/reservations";
import { useAsync } from "../lib/useAsync";
import { EmptyNote, ErrorAlert } from "./common";
import { ConfirmDialog } from "./ConfirmDialog";

type Confirm =
  | { kind: "request"; request: OwnerRequest; approve: boolean }
  | { kind: "early"; reservation: Reservation; accept: boolean }
  | { kind: "handover"; reservation: Reservation }
  | { kind: "return"; reservation: Reservation };

function span(start: string, end: string): string {
  return start === end ? formatDate(start) : `${formatDate(start)} – ${formatDate(end)}`;
}

function Row({ icon, title, detail, children }: { icon: ReactNode; title: ReactNode; detail?: ReactNode; children?: ReactNode }) {
  return (
    <li className="flex flex-wrap items-center gap-3 py-3">
      <span className="grid size-8 shrink-0 place-items-center rounded-full bg-default text-muted">{icon}</span>
      <div className="min-w-0 flex-1">
        <div className="text-sm">{title}</div>
        {detail && <div className="text-xs text-muted">{detail}</div>}
      </div>
      <div className="flex shrink-0 gap-2">{children}</div>
    </li>
  );
}

const Tag = ({ id, tag }: { id: number; tag: string }) => (
  <Link to={`/assets/${id}`} className="font-medium text-accent hover:underline">
    {tag}
  </Link>
);

export function WaitingForYou() {
  const { user, can } = useAuth();
  const editor = can("edit_records");
  const today = todayYmd();
  const requests = useAsync(() => api.listOwnerRequests(), []);
  const reservations = useAsync(() => api.listReservations({ from: addDays(today, -90) }), [today]);
  const [confirm, setConfirm] = useState<Confirm | null>(null);

  const reload = () => {
    requests.reload();
    reservations.reload();
    ownerRequestsChanged();
    reservationsChanged();
  };

  const forMe = (requests.data ?? []).filter(
    (r) => r.status === "pending" && (r.approver_user_id === user.id || (r.approver_user_id == null && editor)),
  );
  const all = reservations.data ?? [];
  const asksMe = all.filter((r) => r.status === "pending" && r.asks_user_id === user.id);
  const handovers = editor
    ? all.filter(
        (r) =>
          r.status === "active" &&
          r.start_date <= today &&
          today <= r.end_date &&
          (r.asset_owner_id == null || r.asset_owner_id !== r.user_owner_id),
      )
    : [];
  const returns = editor
    ? all.filter(
        (r) => r.status === "active" && r.end_date < today && r.asset_owner_id != null && r.asset_owner_id === r.user_owner_id,
      )
    : [];
  const count = forMe.length + asksMe.length + handovers.length + returns.length;
  const error = requests.error ?? reservations.error;

  return (
    <section>
      <div className="mb-3 flex items-center justify-between gap-4">
        <h2 className="text-lg font-semibold">Waiting for you</h2>
        {count > 0 && (
          <span className="text-sm text-muted">
            {count} item{count === 1 ? "" : "s"}
          </span>
        )}
      </div>
      {error && <ErrorAlert error={error} onRetry={reload} />}
      <Card>
        <Card.Content>
          {count === 0 ? (
            <EmptyNote icon={<CircleCheck className="size-5" />} title="Nothing waiting on you">
              Requests that need your answer, and reservations to hand over or take back, appear here.
            </EmptyNote>
          ) : (
            <ul className="flex flex-col divide-y divide-separator">
              {forMe.map((r) => (
                <Row
                  key={`req-${r.id}`}
                  icon={r.kind === "transfer" ? <Hand className="size-4" /> : <PackageCheck className="size-4" />}
                  title={
                    <>
                      <span className="font-medium">{r.requested_by}</span> asks for <Tag id={r.asset_id} tag={r.asset_tag} />:{" "}
                      {r.kind === "check_out" ? "check out" : "transfer"} to {r.owner_name}
                    </>
                  }
                  detail={
                    <>
                      {r.asset_description}
                      {r.current_owner_name ? ` · with ${r.current_owner_name} now` : ""} · {formatDateTime(r.created_at)}
                      {r.note ? ` · “${r.note}”` : ""}
                    </>
                  }
                >
                  <Button size="sm" variant="secondary" onPress={() => setConfirm({ kind: "request", request: r, approve: false })}>
                    Decline
                  </Button>
                  <Button size="sm" onPress={() => setConfirm({ kind: "request", request: r, approve: true })}>
                    Approve
                  </Button>
                </Row>
              ))}
              {asksMe.map((r) => (
                <Row
                  key={`early-${r.id}`}
                  icon={<CalendarClock className="size-4" />}
                  title={
                    <>
                      <span className="font-medium">{r.user_name}</span> asks you to end your reservation of{" "}
                      <Tag id={r.asset_id} tag={r.asset_tag} /> on {formatDate(addDays(r.start_date, -1))}
                    </>
                  }
                  detail={
                    <>
                      They'd like it {span(r.start_date, r.end_date)}
                      {r.note ? ` · “${r.note}”` : ""}
                    </>
                  }
                >
                  <Button size="sm" variant="secondary" onPress={() => setConfirm({ kind: "early", reservation: r, accept: false })}>
                    Decline
                  </Button>
                  <Button size="sm" onPress={() => setConfirm({ kind: "early", reservation: r, accept: true })}>
                    End mine early
                  </Button>
                </Row>
              ))}
              {handovers.map((r) => (
                <Row
                  key={`hand-${r.id}`}
                  icon={<LogOut className="size-4" />}
                  title={
                    <>
                      Hand over <Tag id={r.asset_id} tag={r.asset_tag} /> to <span className="font-medium">{r.user_name}</span>
                    </>
                  }
                  detail={
                    <>
                      Reserved {span(r.start_date, r.end_date)}
                      {r.asset_owner_name
                        ? ` · still checked out to ${r.asset_owner_name}`
                        : r.user_owner_id == null
                          ? ` · link ${r.user_name}'s account to an Owner entry (Users) to check it out to them`
                          : ""}
                    </>
                  }
                >
                  {r.user_owner_id != null && r.asset_owner_id == null && (
                    <Button size="sm" onPress={() => setConfirm({ kind: "handover", reservation: r })}>
                      Check out
                    </Button>
                  )}
                  {(r.asset_owner_id != null || r.user_owner_id == null) && (
                    <Link to="/inventory" className="text-sm text-accent hover:underline">
                      Available Inventory
                    </Link>
                  )}
                </Row>
              ))}
              {returns.map((r) => (
                <Row
                  key={`ret-${r.id}`}
                  icon={<LogIn className="size-4" />}
                  title={
                    <>
                      <Tag id={r.asset_id} tag={r.asset_tag} />: <span className="font-medium">{r.user_name}</span>'s reservation
                      has ended
                    </>
                  }
                  detail={`Reserved ${span(r.start_date, r.end_date)} · still checked out to ${r.asset_owner_name}`}
                >
                  <Button size="sm" onPress={() => setConfirm({ kind: "return", reservation: r })}>
                    Check in
                  </Button>
                </Row>
              ))}
            </ul>
          )}
        </Card.Content>
      </Card>

      <ConfirmDialog
        isOpen={!!confirm}
        onOpenChange={(o) => !o && setConfirm(null)}
        title={
          !confirm
            ? ""
            : confirm.kind === "request"
              ? `${confirm.approve ? "Approve" : "Decline"} ${confirm.request.requested_by}'s request for ${confirm.request.asset_tag}?`
              : confirm.kind === "early"
                ? confirm.accept
                  ? `End your reservation of ${confirm.reservation.asset_tag} early?`
                  : `Decline ${confirm.reservation.user_name}'s request?`
                : confirm.kind === "handover"
                  ? `Check ${confirm.reservation.asset_tag} out to ${confirm.reservation.user_name}?`
                  : `Check ${confirm.reservation.asset_tag} in?`
        }
        confirmLabel={
          !confirm
            ? ""
            : confirm.kind === "request"
              ? confirm.approve
                ? "Approve"
                : "Decline"
              : confirm.kind === "early"
                ? confirm.accept
                  ? "End mine early"
                  : "Decline"
                : confirm.kind === "handover"
                  ? "Check out"
                  : "Check in"
        }
        tone={
          confirm && ((confirm.kind === "request" && !confirm.approve) || (confirm.kind === "early" && !confirm.accept))
            ? "danger"
            : "accent"
        }
        askReason
        reasonLabel="Note (optional)"
        onConfirm={async (note) => {
          if (!confirm) return;
          if (confirm.kind === "request") {
            await api.decideOwnerRequest(confirm.request.id, confirm.approve, note);
            toast.success(confirm.approve ? `Approved: ${confirm.request.asset_tag}` : "Request declined");
          } else if (confirm.kind === "early") {
            await api.answerReservationRequest(confirm.reservation.id, confirm.accept, note);
            toast.success(confirm.accept ? "Your reservation now ends early" : "Request declined");
          } else if (confirm.kind === "handover") {
            const r = confirm.reservation;
            await api.changeOwner([r.asset_id], { action: "check_out", owner_id: r.user_owner_id! }, note || `Reservation ${span(r.start_date, r.end_date)}`);
            toast.success(`${r.asset_tag} checked out to ${r.user_name}`);
          } else {
            const r = confirm.reservation;
            await api.changeOwner([r.asset_id], { action: "check_in" }, note || "Reservation ended");
            toast.success(`${r.asset_tag} checked in`);
          }
          setConfirm(null);
          reload();
        }}
      >
        {!confirm
          ? null
          : confirm.kind === "request"
            ? confirm.approve
              ? `${confirm.request.asset_tag} goes to ${confirm.request.owner_name} now.`
              : "Nothing changes."
            : confirm.kind === "early"
              ? confirm.accept
                ? `Yours will end on ${formatDate(addDays(confirm.reservation.start_date, -1))}, and ${confirm.reservation.user_name}'s is booked.`
                : "Your reservation stays as it is."
              : confirm.kind === "handover"
                ? `For their reservation, ${span(confirm.reservation.start_date, confirm.reservation.end_date)}.`
                : `It's back in the store, no longer with ${confirm.reservation.user_name}.`}
      </ConfirmDialog>
    </section>
  );
}
