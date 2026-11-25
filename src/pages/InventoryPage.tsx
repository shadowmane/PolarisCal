// Available Inventory: which Instruments are free to use, who has the rest, and checking
// them out, back in, or over to someone else. An Instrument's Owner is who it's checked
// out to. Viewers can't make those changes themselves; they request a check-out or
// transfer, which a Technician or Administrator approves or declines.

import { Alert, Button, Checkbox, Modal, SearchField, Table, Tabs, toast } from "@heroui/react";
import { ArrowRightLeft, Check, Inbox, LogIn, LogOut, PackageCheck, Send, X } from "lucide-react";
import { useDeferredValue, useEffect, useMemo, useState } from "react";
import type { Selection } from "react-aria-components";
import { Link, useSearchParams } from "react-router";
import type { Asset } from "../bindings/Asset";
import type { OwnerChange } from "../bindings/OwnerChange";
import type { OwnerRequest } from "../bindings/OwnerRequest";
import type { OwnerRequestStatus } from "../bindings/OwnerRequestStatus";
import { ConfirmDialog } from "../components/ConfirmDialog";
import { EmptyNote, ErrorAlert, Loading, PageHeader, StatusChip } from "../components/common";
import { SelectInput, TextInput } from "../components/fields";
import { LookupSelect } from "../components/LookupSelect";
import { ApiError, api } from "../lib/api";
import { useAuth } from "../lib/auth";
import { dueStatusMeta, EMPTY, formatDate, formatDateTime } from "../lib/format";
import { useLookups } from "../lib/lookups";
import { ownerRequestsChanged } from "../lib/ownerRequests";
import { useAsync } from "../lib/useAsync";

type View = "available" | "out" | "requests";

/** Requests decided this long ago still show under Requests. */
const RECENT_DAYS = 30;
type Action = "check_out" | "check_in" | "transfer";

/** Away for service or retired: not available, even with no owner. */
const AWAY = new Set(["retired", "out_for_calibration", "out_for_repair"]);

function matches(a: Asset, query: string): boolean {
  if (!query) return true;
  const q = query.toLowerCase();
  return [a.asset_tag, a.description, a.model_number, a.serial_number, a.manufacturer_name, a.owner_name].some(
    (v) => v?.toLowerCase().includes(q),
  );
}

function SelectionBox({ label }: { label?: string }) {
  return (
    <Checkbox slot="selection" aria-label={label} variant="secondary">
      <Checkbox.Control className="size-5 shrink-0 border-2 border-muted">
        <Checkbox.Indicator />
      </Checkbox.Control>
    </Checkbox>
  );
}

export function InventoryPage() {
  const { can } = useAuth();
  // Technicians and Administrators make changes; everyone else asks for them.
  const canEdit = can("edit_records");
  // The view and owner filter are kept in the address, so going back returns to them.
  const [params, setParams] = useSearchParams();
  const view: View = params.get("view") === "out" ? "out" : params.get("view") === "requests" ? "requests" : "available";
  const ownerFilter = params.get("owner") ? Number(params.get("owner")) : null;
  const [search, setSearch] = useState("");
  const query = useDeferredValue(search);
  const [selected, setSelected] = useState<Selection>(new Set());
  const [action, setAction] = useState<Action | null>(null);

  const items = useAsync(() => api.listAssets({ kind: "instrument", include_archived: false }), []);
  const requests = useAsync(
    () => api.listOwnerRequests({ decided_since: new Date(Date.now() - RECENT_DAYS * 86_400_000).toISOString() }),
    [],
  );
  const pending = (requests.data ?? []).filter((r) => r.status === "pending");
  const pendingFor = new Map(pending.map((r) => [r.asset_id, r]));
  const reloadAll = () => {
    items.reload();
    requests.reload();
    ownerRequestsChanged();
  };
  const all = items.data ?? [];
  const available = all.filter((a) => a.owner_id == null && !AWAY.has(a.status_role ?? ""));
  const away = all.filter((a) => a.owner_id == null && AWAY.has(a.status_role ?? "")).length;
  const out = all.filter((a) => a.owner_id != null);
  // Owners who have something checked out, for the filter.
  const holders = useMemo(() => {
    const byId = new Map<number, { name: string; count: number }>();
    for (const a of out) {
      const h = byId.get(a.owner_id!) ?? { name: a.owner_name ?? "", count: 0 };
      h.count += 1;
      byId.set(a.owner_id!, h);
    }
    return [...byId.entries()].sort((x, y) => x[1].name.localeCompare(y[1].name));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [items.data]);

  const rows = (view === "available" ? available : out.filter((a) => ownerFilter == null || a.owner_id === ownerFilter))
    .filter((a) => matches(a, query))
    .sort((x, y) => x.asset_tag.localeCompare(y.asset_tag, undefined, { numeric: true }));

  // A change of view or filter starts a fresh selection.
  useEffect(() => setSelected(new Set()), [view, ownerFilter, query]);
  const chosen: Asset[] = selected === "all" ? rows : rows.filter((a) => selected.has(a.id));

  function setView(next: View) {
    setParams(next === "available" ? {} : { view: next }, { replace: true });
  }
  function setOwnerFilter(owner: number | null) {
    setParams(owner == null ? { view: "out" } : { view: "out", owner: String(owner) }, { replace: true });
  }

  return (
    <>
      <PageHeader
        title="Available Inventory"
        description="Instruments that aren't checked out to anyone, and who has the rest. An instrument's Owner is who it's checked out to."
        actions={
          view === "requests" ? null : view === "available" ? (
            <Button isDisabled={chosen.length === 0} onPress={() => setAction("check_out")}>
              {canEdit ? <LogOut className="size-4" /> : <Send className="size-4" />}
              {canEdit ? "Check out" : "Request check out"}
              {chosen.length ? ` (${chosen.length})` : ""}
            </Button>
          ) : (
            <>
              <Button
                variant={canEdit ? "secondary" : "primary"}
                isDisabled={chosen.length === 0}
                onPress={() => setAction("transfer")}
              >
                {canEdit ? <ArrowRightLeft className="size-4" /> : <Send className="size-4" />}
                {canEdit ? "Transfer" : "Request transfer"}
                {chosen.length ? ` (${chosen.length})` : ""}
              </Button>
              {/* Checking in means handling the item back into the store: not something to request. */}
              {canEdit && (
                <Button isDisabled={chosen.length === 0} onPress={() => setAction("check_in")}>
                  <LogIn className="size-4" /> Check in{chosen.length ? ` (${chosen.length})` : ""}
                </Button>
              )}
            </>
          )
        }
      />

      <div className="mb-4 flex flex-wrap items-end justify-between gap-4">
        <div className="flex flex-wrap items-end gap-4">
          <Tabs selectedKey={view} onSelectionChange={(k) => setView(k as View)}>
            <Tabs.ListContainer>
              <Tabs.List aria-label="Show" className="*:whitespace-nowrap">
                <Tabs.Tab id="available">
                  Available{items.data ? ` (${available.length})` : ""}
                  <Tabs.Indicator />
                </Tabs.Tab>
                <Tabs.Tab id="out">
                  Checked out{items.data ? ` (${out.length})` : ""}
                  <Tabs.Indicator />
                </Tabs.Tab>
                <Tabs.Tab id="requests">
                  Requests{requests.data ? ` (${pending.length})` : ""}
                  <Tabs.Indicator />
                </Tabs.Tab>
              </Tabs.List>
            </Tabs.ListContainer>
          </Tabs>
          {view === "out" && (
            <SelectInput<string>
              label="Checked out to"
              value={ownerFilter == null ? "all" : String(ownerFilter)}
              onChange={(v) => setOwnerFilter(!v || v === "all" ? null : Number(v))}
              options={[
                { id: "all", label: `Everyone (${out.length})` },
                ...holders.map(([id, h]) => ({ id: String(id), label: `${h.name} (${h.count})` })),
              ]}
              allowEmpty={false}
              className="w-64"
            />
          )}
        </div>
        {view !== "requests" && (
        <SearchField value={search} onChange={setSearch} aria-label="Search" className="w-80">
          <SearchField.Group>
            <SearchField.SearchIcon />
            <SearchField.Input placeholder="Search tag, description, model, serial…" />
            <SearchField.ClearButton />
          </SearchField.Group>
        </SearchField>
        )}
      </div>

      {view === "requests" ? (
        <RequestsView requests={requests} onChanged={reloadAll} />
      ) : (
        <>
      {items.error && <ErrorAlert error={items.error} onRetry={items.reload} />}
      {items.loading && !items.data && <Loading />}
      {items.data && (
        <Table>
          <Table.ScrollContainer>
            <Table.Content
              aria-label={view === "available" ? "Available instruments" : "Checked-out instruments"}
              selectionMode="multiple"
              selectedKeys={selected}
              onSelectionChange={setSelected}
            >
              <Table.Header>
                <Table.Column className="w-10">
                  <SelectionBox label="Select all" />
                </Table.Column>
                <Table.Column isRowHeader>Asset tag</Table.Column>
                <Table.Column>Description</Table.Column>
                <Table.Column>Manufacturer / model</Table.Column>
                <Table.Column>Location</Table.Column>
                <Table.Column>Status</Table.Column>
                <Table.Column>Next due</Table.Column>
                {view === "out" ? <Table.Column>Checked out to</Table.Column> : null}
                {view === "out" ? <Table.Column>Since</Table.Column> : null}
              </Table.Header>
              <Table.Body
                items={rows}
                renderEmptyState={() => (
                  <EmptyNote icon={<PackageCheck className="size-5" />} title={query ? "No matches" : view === "available" ? "Nothing available" : "Nothing checked out"}>
                    {query
                      ? `Nothing matches “${query}”.`
                      : view === "available"
                        ? away > 0
                          ? `Every instrument is checked out, retired, or away for service (${away} retired or away).`
                          : "Every instrument is checked out."
                        : "Every instrument is available."}
                  </EmptyNote>
                )}
              >
                {(a) => {
                  const due = dueStatusMeta[a.due_status];
                  return (
                    <Table.Row id={a.id} className="cursor-default">
                      <Table.Cell>
                        <SelectionBox label={`Select ${a.asset_tag}`} />
                      </Table.Cell>
                      <Table.Cell>
                        <Link to={`/assets/${a.id}`} className="font-medium text-accent hover:underline">
                          {a.asset_tag}
                        </Link>
                        {a.parent_tag && <div className="text-xs text-muted">in {a.parent_tag}</div>}
                        {pendingFor.has(a.id) && <RequestedNote request={pendingFor.get(a.id)!} />}
                      </Table.Cell>
                      <Table.Cell>{a.description}</Table.Cell>
                      <Table.Cell>{[a.manufacturer_name, a.model_number].filter(Boolean).join(" · ") || EMPTY}</Table.Cell>
                      <Table.Cell>{a.location_name ?? EMPTY}</Table.Cell>
                      <Table.Cell>
                        <span className={a.status_role === "storage" ? "text-warning" : ""}>{a.status_name}</span>
                      </Table.Cell>
                      <Table.Cell>
                        <div className="flex flex-col items-start gap-1">
                          <span>{formatDate(a.next_cal_date)}</span>
                          {(a.due_status === "overdue" || a.due_status === "due_soon") && (
                            <StatusChip color={due.color}>{due.label}</StatusChip>
                          )}
                        </div>
                      </Table.Cell>
                      {view === "out" ? <Table.Cell className="font-medium">{a.owner_name ?? EMPTY}</Table.Cell> : null}
                      {view === "out" ? (
                        <Table.Cell>{a.owner_since ? formatDateTime(a.owner_since) : EMPTY}</Table.Cell>
                      ) : null}
                    </Table.Row>
                  );
                }}
              </Table.Body>
            </Table.Content>
          </Table.ScrollContainer>
        </Table>
      )}
      {view === "available" && away > 0 && items.data && available.length > 0 && (
        <p className="mt-3 text-xs text-muted">
          {away} instrument{away === 1 ? " isn't" : "s aren't"} listed because {away === 1 ? "it's" : "they're"} retired or
          away for calibration or repair.
        </p>
      )}

        </>
      )}

      <OwnerDialog
        action={action}
        request={!canEdit}
        items={chosen}
        onClose={() => setAction(null)}
        onDone={() => {
          setAction(null);
          setSelected(new Set());
          reloadAll();
        }}
      />
    </>
  );
}

const DIALOG_TEXT: Record<Action, { title: string; button: string; done: string }> = {
  check_out: { title: "Check out", button: "Check out", done: "Checked out" },
  check_in: { title: "Check in", button: "Check in", done: "Checked in" },
  transfer: { title: "Transfer", button: "Transfer", done: "Transferred" },
};

const REQUEST_TEXT: Record<"check_out" | "transfer", { title: string; owner: string }> = {
  check_out: { title: "Request check out of", owner: "Check out to" },
  transfer: { title: "Request transfer of", owner: "Transfer to" },
};

/** Under an item's tag: someone has asked for it. */
function RequestedNote({ request }: { request: OwnerRequest }) {
  const what = request.kind === "check_out" ? "check out" : "transfer";
  return (
    <div className="mt-0.5 text-xs text-warning" title={request.note ?? undefined}>
      {request.requested_by} requested {what} to {request.owner_name}
    </div>
  );
}

function OwnerDialog({
  action,
  request,
  items,
  onClose,
  onDone,
}: {
  action: Action | null;
  /** Ask a Technician or Administrator instead of making the change. */
  request: boolean;
  items: Asset[];
  onClose: () => void;
  onDone: () => void;
}) {
  const lookups = useLookups();
  const [owner, setOwner] = useState<number | null>(null);
  const [reason, setReason] = useState("");
  const [error, setError] = useState<string>();
  const [ownerError, setOwnerError] = useState<string>();
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    if (!action) return;
    setOwner(null);
    setReason("");
    setError(undefined);
    setOwnerError(undefined);
    lookups.reload();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [action]);

  const text = action ? DIALOG_TEXT[action] : DIALOG_TEXT.check_out;
  const asking = request && action !== "check_in";
  const requestText = REQUEST_TEXT[action === "transfer" ? "transfer" : "check_out"];
  const needsOwner = action === "check_out" || action === "transfer";
  const overdue = items.filter((a) => a.due_status === "overdue");
  const stored = items.filter((a) => a.status_role === "storage");
  const count = `${items.length} instrument${items.length === 1 ? "" : "s"}`;

  async function save() {
    if (!action) return;
    if (needsOwner && owner == null) return setOwnerError("Choose who they're going to.");
    const change: OwnerChange =
      action === "check_in" ? { action: "check_in" } : { action, owner_id: owner! };
    setBusy(true);
    try {
      const to = needsOwner ? lookups.data?.owner.find((o) => o.id === owner)?.name : null;
      if (asking) {
        await api.requestOwnerChange(
          items.map((a) => a.id),
          change,
          reason,
        );
        toast.success(`Request sent for ${count}. A Technician or Administrator will approve it.`);
      } else {
        await api.changeOwner(
          items.map((a) => a.id),
          change,
          reason,
        );
        toast.success(`${text.done} ${count}${to ? ` to ${to}` : ""}`);
      }
      onDone();
    } catch (e) {
      setError(e instanceof ApiError ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Modal>
      <Modal.Backdrop isOpen={!!action} onOpenChange={(o) => !o && onClose()} isDismissable={!busy}>
        <Modal.Container size="md" scroll="inside">
          <Modal.Dialog>
            <Modal.CloseTrigger />
            <Modal.Header>
              <Modal.Heading>
                {asking ? requestText.title : text.title} {count}
              </Modal.Heading>
            </Modal.Header>
            <Modal.Body>
              <form
                id="owner-change"
                noValidate
                className="flex flex-col gap-4 p-1"
                onSubmit={(e) => {
                  e.preventDefault();
                  void save();
                }}
              >
                {asking && (
                  <p className="text-sm text-muted">
                    A Technician or Administrator will approve or decline your request. You'll see the outcome under
                    Requests.
                  </p>
                )}
                {error && (
                  <Alert status="danger">
                    <Alert.Indicator />
                    <Alert.Content>
                      <Alert.Description>{error}</Alert.Description>
                    </Alert.Content>
                  </Alert>
                )}
                <ul className="flex max-h-40 flex-col divide-y divide-separator overflow-y-auto rounded-xl border border-separator text-sm">
                  {items.map((a) => (
                    <li key={a.id} className="flex justify-between gap-3 px-3 py-1.5">
                      <span>
                        <span className="font-medium">{a.asset_tag}</span>
                        <span className="text-muted"> · {a.description}</span>
                      </span>
                      {action !== "check_out" && a.owner_name && (
                        <span className="shrink-0 text-xs text-muted">with {a.owner_name}</span>
                      )}
                    </li>
                  ))}
                </ul>
                {action === "check_out" && overdue.length > 0 && (
                  <Alert status="warning">
                    <Alert.Indicator />
                    <Alert.Content>
                      <Alert.Description>
                        {overdue.map((a) => a.asset_tag).join(", ")} {overdue.length === 1 ? "is" : "are"} overdue for
                        calibration.
                      </Alert.Description>
                    </Alert.Content>
                  </Alert>
                )}
                {action === "check_out" && stored.length > 0 && (
                  <Alert status="warning">
                    <Alert.Indicator />
                    <Alert.Content>
                      <Alert.Description>
                        {stored.map((a) => a.asset_tag).join(", ")} {stored.length === 1 ? "is" : "are"} in Storage, so
                        not tracked for calibration. Change the status when {stored.length === 1 ? "it goes" : "they go"}{" "}
                        back into use.
                      </Alert.Description>
                    </Alert.Content>
                  </Alert>
                )}
                {needsOwner && (
                  <LookupSelect
                    kind="owner"
                    label={asking ? requestText.owner : action === "transfer" ? "Transfer to" : "Check out to"}
                    value={owner}
                    onChange={(v) => {
                      setOwner(v);
                      setOwnerError(undefined);
                    }}
                    lookups={lookups}
                    error={ownerError}
                    isRequired
                  />
                )}
                <TextInput
                  label="Note"
                  value={reason}
                  onChange={setReason}
                  placeholder={action === "check_in" ? "e.g. Returned after the job" : "e.g. Field job at plant 2"}
                  description={
                    asking
                      ? "Optional. Why you need it, for whoever approves the request."
                      : "Optional. Saved in each instrument's history."
                  }
                />
              </form>
            </Modal.Body>
            <Modal.Footer>
              <Button slot="close" variant="tertiary" isDisabled={busy}>
                Cancel
              </Button>
              <Button type="submit" form="owner-change" isPending={busy}>
                {asking ? "Send request" : text.button}
              </Button>
            </Modal.Footer>
          </Modal.Dialog>
        </Modal.Container>
      </Modal.Backdrop>
    </Modal>
  );
}

function waitingAll(list: OwnerRequest[] | undefined): OwnerRequest[] {
  return (list ?? []).filter((r) => r.status === "pending");
}

const OUTCOME: Record<OwnerRequestStatus, { label: string; color: "default" | "success" | "danger" | "warning" }> = {
  pending: { label: "Waiting", color: "warning" },
  approved: { label: "Approved", color: "success" },
  declined: { label: "Declined", color: "danger" },
  cancelled: { label: "Cancelled", color: "default" },
};

function requestText(r: OwnerRequest): string {
  return `${r.kind === "check_out" ? "Check out" : "Transfer"} to ${r.owner_name}`;
}

/** Waiting requests to approve, decline, or withdraw, then the recently decided ones. */
function RequestsView({
  requests,
  onChanged,
}: {
  requests: { data?: OwnerRequest[]; error?: ApiError; loading: boolean; reload: () => void };
  onChanged: () => void;
}) {
  const { can, user } = useAuth();
  const editor = can("edit_records");
  // Addressed to you (the holder), or for a Technician or Administrator.
  const decides = (r: OwnerRequest) => editor || r.approver_user_id === user.id;
  const canDecide = editor || waitingAll(requests.data).some((r) => r.approver_user_id === user.id);
  const [deciding, setDeciding] = useState<{ request: OwnerRequest; approve: boolean } | null>(null);
  const [cancelling, setCancelling] = useState<OwnerRequest | null>(null);
  const all = requests.data ?? [];
  const waiting = all.filter((r) => r.status === "pending");
  const decided = all.filter((r) => r.status !== "pending");

  if (requests.error) return <ErrorAlert error={requests.error} onRetry={requests.reload} />;
  if (requests.loading && !requests.data) return <Loading />;

  return (
    <div className="flex flex-col gap-8">
      <section className="flex flex-col gap-3">
        <h2 className="text-sm font-semibold">Waiting{canDecide ? " for you" : ""}</h2>
        <Table>
          <Table.ScrollContainer>
            <Table.Content aria-label="Waiting requests">
              <Table.Header>
                <Table.Column isRowHeader>Instrument</Table.Column>
                <Table.Column>Request</Table.Column>
                <Table.Column>With now</Table.Column>
                <Table.Column>Requested by</Table.Column>
                <Table.Column>Note</Table.Column>
                <Table.Column>Decided by</Table.Column>
                <Table.Column>
                  <span className="sr-only">Actions</span>
                </Table.Column>
              </Table.Header>
              <Table.Body
                items={waiting}
                renderEmptyState={() => (
                  <EmptyNote icon={<Inbox className="size-5" />} title="No requests waiting">
                    {canDecide
                      ? "Requests from Viewers to check out or transfer instruments appear here."
                      : "Select instruments under Available or Checked out to request them."}
                  </EmptyNote>
                )}
              >
                {(r) => {
                  const mine = r.requested_by_id === user.id;
                  return (
                    <Table.Row id={r.id}>
                      <Table.Cell>
                        <Link to={`/assets/${r.asset_id}`} className="font-medium text-accent hover:underline">
                          {r.asset_tag}
                        </Link>
                        <div className="text-xs text-muted">{r.asset_description}</div>
                      </Table.Cell>
                      <Table.Cell className="font-medium">{requestText(r)}</Table.Cell>
                      <Table.Cell>{r.current_owner_name ?? <span className="text-muted">Available</span>}</Table.Cell>
                      <Table.Cell>
                        {r.requested_by}
                        <div className="text-xs text-muted">{formatDateTime(r.created_at)}</div>
                      </Table.Cell>
                      <Table.Cell className="max-w-64 whitespace-normal">{r.note ?? EMPTY}</Table.Cell>
                      <Table.Cell>
                        {r.approver_user_id === user.id ? (
                          <span className="font-medium">You</span>
                        ) : (
                          (r.approver_name ?? <span className="text-muted">A Technician or Administrator</span>)
                        )}
                      </Table.Cell>
                      <Table.Cell>
                        <div className="flex justify-end gap-2">
                          {decides(r) && (
                            <>
                              <Button size="sm" onPress={() => setDeciding({ request: r, approve: true })}>
                                <Check className="size-4" /> Approve
                              </Button>
                              <Button
                                size="sm"
                                variant="secondary"
                                onPress={() => setDeciding({ request: r, approve: false })}
                              >
                                <X className="size-4" /> Decline
                              </Button>
                            </>
                          )}
                          {mine && !decides(r) && (
                            <Button size="sm" variant="secondary" onPress={() => setCancelling(r)}>
                              Cancel request
                            </Button>
                          )}
                        </div>
                      </Table.Cell>
                    </Table.Row>
                  );
                }}
              </Table.Body>
            </Table.Content>
          </Table.ScrollContainer>
        </Table>
      </section>

      <section className="flex flex-col gap-3">
        <h2 className="text-sm font-semibold">Decided in the last {RECENT_DAYS} days</h2>
        <Table>
          <Table.ScrollContainer>
            <Table.Content aria-label="Decided requests">
              <Table.Header>
                <Table.Column isRowHeader>Instrument</Table.Column>
                <Table.Column>Request</Table.Column>
                <Table.Column>Requested by</Table.Column>
                <Table.Column>Outcome</Table.Column>
                <Table.Column>By</Table.Column>
                <Table.Column>Note</Table.Column>
              </Table.Header>
              <Table.Body
                items={decided}
                renderEmptyState={() => (
                  <EmptyNote icon={<Inbox className="size-5" />} title="Nothing decided recently" />
                )}
              >
                {(r) => (
                  <Table.Row id={r.id}>
                    <Table.Cell>
                      <Link to={`/assets/${r.asset_id}`} className="font-medium text-accent hover:underline">
                        {r.asset_tag}
                      </Link>
                    </Table.Cell>
                    <Table.Cell>{requestText(r)}</Table.Cell>
                    <Table.Cell>
                      {r.requested_by}
                      <div className="text-xs text-muted">{formatDateTime(r.created_at)}</div>
                    </Table.Cell>
                    <Table.Cell>
                      <StatusChip color={OUTCOME[r.status].color}>{OUTCOME[r.status].label}</StatusChip>
                    </Table.Cell>
                    <Table.Cell>
                      {r.decided_by ?? EMPTY}
                      {r.decided_at && <div className="text-xs text-muted">{formatDateTime(r.decided_at)}</div>}
                    </Table.Cell>
                    <Table.Cell className="max-w-64 whitespace-normal">{r.decision_note ?? r.note ?? EMPTY}</Table.Cell>
                  </Table.Row>
                )}
              </Table.Body>
            </Table.Content>
          </Table.ScrollContainer>
        </Table>
      </section>

      <ConfirmDialog
        isOpen={!!deciding}
        onOpenChange={(o) => !o && setDeciding(null)}
        title={
          deciding
            ? `${deciding.approve ? "Approve" : "Decline"}: ${requestText(deciding.request)}?`
            : ""
        }
        confirmLabel={deciding?.approve ? "Approve" : "Decline"}
        tone={deciding?.approve ? "accent" : "danger"}
        askReason
        reasonLabel="Note (optional)"
        onConfirm={async (note) => {
          if (!deciding) return;
          await api.decideOwnerRequest(deciding.request.id, deciding.approve, note);
          toast.success(
            deciding.approve
              ? `${deciding.request.asset_tag}: ${requestText(deciding.request).toLowerCase()} done`
              : `Request for ${deciding.request.asset_tag} declined`,
          );
          onChanged();
        }}
      >
        {deciding && (
          <>
            {deciding.request.requested_by} asked for {deciding.request.asset_tag} ({deciding.request.asset_description})
            {deciding.request.note ? `: “${deciding.request.note}”` : "."}
            {deciding.approve
              ? deciding.request.kind === "check_out"
                ? " Approving checks it out now."
                : ` Approving transfers it from ${deciding.request.current_owner_name ?? "its owner"} now.`
              : " Nothing changes."}
          </>
        )}
      </ConfirmDialog>
      <ConfirmDialog
        isOpen={!!cancelling}
        onOpenChange={(o) => !o && setCancelling(null)}
        title={cancelling ? `Cancel your request for ${cancelling.asset_tag}?` : ""}
        confirmLabel="Cancel request"
        onConfirm={async () => {
          if (!cancelling) return;
          await api.cancelOwnerRequest(cancelling.id);
          toast.success("Request cancelled");
          onChanged();
        }}
      >
        {cancelling ? `${requestText(cancelling)}. You can ask again later.` : null}
      </ConfirmDialog>
    </div>
  );
}
