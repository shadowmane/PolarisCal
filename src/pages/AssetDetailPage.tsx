import { Alert, Button, Card, Chip, Table, toast } from "@heroui/react";
import {
  Archive,
  ArchiveRestore,
  CalendarPlus,
  Hand,
  ArrowLeft,
  Boxes,
  ClipboardCheck,
  Copy,
  Gauge,
  History,
  Paperclip,
  Pencil,
  Plus,
  Printer,
} from "lucide-react";
import { type ReactNode, useState } from "react";
import { Link, useNavigate, useParams } from "react-router";
import type { Asset } from "../bindings/Asset";
import type { CalibrationRecord } from "../bindings/CalibrationRecord";
import { CalibrationDialog } from "../components/CalibrationDialog";
import { ConfirmDialog } from "../components/ConfirmDialog";
import { ToggleSwitch } from "../components/fields";
import { CollapsibleCard, EmptyNote, ErrorAlert, Loading, Money, StatusChip } from "../components/common";
import { AttachmentsPanel } from "../components/Attachments";
import { AuditList } from "../components/AuditList";
import { ItemPhotos, useHeight, useItemPhotos } from "../components/ItemPhotos";
import { ReserveDialog } from "../components/ReserveDialog";
import { ownerRequestsChanged } from "../lib/ownerRequests";
import { api } from "../lib/api";
import { useAuth } from "../lib/auth";
import {
  conditionReceivedMeta,
  conditionReturnedMeta,
  dueStatusMeta,
  EMPTY,
  formatDate,
  formatSchedule,
} from "../lib/format";
import { useAsync } from "../lib/useAsync";

function Detail({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex flex-col gap-0.5">
      <dt className="text-xs text-muted">{label}</dt>
      <dd className="text-sm break-words">{children ?? EMPTY}</dd>
    </div>
  );
}

function ChildrenCard({ system }: { system: Asset }) {
  const navigate = useNavigate();
  const { can } = useAuth();
  const children = useAsync(() => api.listAssets({ parent_id: system.id }), [system.id, system.row_version]);
  return (
    <Card>
      <Card.Header className="flex-row items-center justify-between">
        <div>
          <Card.Title>Contained instruments</Card.Title>
          <Card.Description>
            {system.cal_aggregate
              ? "Each instrument keeps its own calibration schedule; this System is due when the soonest of them is."
              : "Each instrument keeps its own calibration schedule."}
          </Card.Description>
        </div>
        {!system.is_archived && can("edit_records") && (
          <Button
            size="sm"
            variant="secondary"
            onPress={() => navigate(`/assets/new?kind=instrument&parent=${system.id}`)}
          >
            <Plus className="size-4" /> Add instrument
          </Button>
        )}
      </Card.Header>
      <Card.Content>
        {children.error && <ErrorAlert error={children.error} onRetry={children.reload} />}
        <Table variant="secondary">
          <Table.ScrollContainer>
            <Table.Content aria-label="Contained instruments" onRowAction={(key) => navigate(`/assets/${key}`)}>
              <Table.Header>
                <Table.Column isRowHeader>Asset tag</Table.Column>
                <Table.Column>Description</Table.Column>
                <Table.Column>Status</Table.Column>
                <Table.Column>Next due</Table.Column>
              </Table.Header>
              <Table.Body
                items={children.data ?? []}
                renderEmptyState={() => (
                  <EmptyNote icon={<Gauge className="size-5" />} title="No instruments in this System yet" />
                )}
              >
                {(a) => (
                  <Table.Row id={a.id} className="cursor-pointer">
                    <Table.Cell className="font-medium">{a.asset_tag}</Table.Cell>
                    <Table.Cell>{a.description}</Table.Cell>
                    <Table.Cell>{a.status_name}</Table.Cell>
                    <Table.Cell>
                      <div className="flex items-center gap-2">
                        {formatDate(a.next_cal_date)}
                        {(a.due_status === "overdue" || a.due_status === "extended" || a.due_status === "due_soon") && (
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
      </Card.Content>
    </Card>
  );
}

function HistoryCard({
  asset,
  version,
  onEdit,
  onAdd,
}: {
  asset: Asset;
  version: number;
  onEdit: (r: CalibrationRecord) => void;
  onAdd: () => void;
}) {
  const [showArchived, setShowArchived] = useState(false);
  const { can } = useAuth();
  const canEdit = can("edit_records");
  const navigate = useNavigate();
  const records = useAsync(
    () => api.listCalibrations({ asset_id: asset.id, include_archived: showArchived }),
    [asset.id, showArchived, version],
  );
  const byId = new Map((records.data ?? []).map((r) => [r.id, r]));

  return (
    <Card>
      <Card.Header className="flex-row items-center justify-between">
        <div>
          <Card.Title>Service history</Card.Title>
          <Card.Description>
            {canEdit ? "Newest first. Select a record to view or edit it." : "Newest first."}
          </Card.Description>
        </div>
        <div className="flex items-center gap-4">
          <ToggleSwitch label="Show archived" isSelected={showArchived} onChange={setShowArchived} />
          <Button size="sm" variant="ghost" onPress={() => navigate(`/reports/history/${asset.id}`)}>
            <Printer className="size-4" /> Print or export
          </Button>
          {!asset.is_archived && canEdit && (
            <Button size="sm" variant="secondary" onPress={onAdd}>
              <Plus className="size-4" /> Record service
            </Button>
          )}
        </div>
      </Card.Header>
      <Card.Content>
        {records.error && <ErrorAlert error={records.error} onRetry={records.reload} />}
        <Table variant="secondary">
          <Table.ScrollContainer>
            <Table.Content
              aria-label="Service history"
              onRowAction={
                canEdit
                  ? (key) => {
                      const r = byId.get(Number(key));
                      if (r) onEdit(r);
                    }
                  : undefined
              }
            >
              <Table.Header>
                <Table.Column isRowHeader>Date</Table.Column>
                <Table.Column>Service</Table.Column>
                <Table.Column>Performed by</Table.Column>
                <Table.Column>Certificate</Table.Column>
                <Table.Column>As received</Table.Column>
                <Table.Column>As returned</Table.Column>
                <Table.Column>Cost</Table.Column>
                <Table.Column>Due date set</Table.Column>
              </Table.Header>
              <Table.Body
                items={records.data ?? []}
                renderEmptyState={() => (
                  <EmptyNote icon={<ClipboardCheck className="size-5" />} title="No service recorded" />
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
                    <Table.Cell>{r.service_type_name}</Table.Cell>
                    <Table.Cell>
                      {r.calibrated_by_name ?? EMPTY}
                      {r.technician && <div className="text-xs text-muted">{r.technician}</div>}
                    </Table.Cell>
                    <Table.Cell>
                      <span className="inline-flex items-center gap-1.5">
                        {r.certificate_number ?? EMPTY}
                        {r.attachment_count > 0 && (
                          <span
                            className="inline-flex items-center gap-0.5 text-xs text-muted"
                            title={`${r.attachment_count} file(s) attached`}
                          >
                            <Paperclip className="size-3.5" />
                            {r.attachment_count}
                          </span>
                        )}
                      </span>
                    </Table.Cell>
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
                    <Table.Cell>
                      <Money cents={r.cost_cents} />
                    </Table.Cell>
                    <Table.Cell>{formatDate(r.next_due_date)}</Table.Cell>
                  </Table.Row>
                )}
              </Table.Body>
            </Table.Content>
          </Table.ScrollContainer>
        </Table>
      </Card.Content>
    </Card>
  );
}

function ChangeHistoryCard({ asset, version }: { asset: Asset; version: number }) {
  return (
    <CollapsibleCard
      title="Change history"
      description="Every change to this item and its service records: who, when, and why."
      rememberAs="asset-change-history"
    >
      <ChangeHistory asset={asset} version={version} />
    </CollapsibleCard>
  );
}

/** Loads only when shown, i.e. while the card is open. */
function ChangeHistory({ asset, version }: { asset: Asset; version: number }) {
  const [limit, setLimit] = useState(20);
  const entries = useAsync(
    () => api.listAudit({ asset_id: asset.id, limit: limit + 1 }),
    [asset.id, asset.row_version, version, limit],
  );
  const shown = (entries.data ?? []).slice(0, limit);
  const more = (entries.data?.length ?? 0) > limit;
  return (
    <>
        {entries.error && <ErrorAlert error={entries.error} onRetry={entries.reload} />}
        {entries.data && shown.length === 0 && (
          <EmptyNote icon={<History className="size-5" />} title="No recorded changes">
            Items created before change tracking was added have no history yet.
          </EmptyNote>
        )}
        <AuditList entries={shown} showSubject={false} />
        {more && (
          <Button size="sm" variant="ghost" className="mt-3" onPress={() => setLimit((l) => l + 30)}>
            Show older changes
          </Button>
        )}
    </>
  );
}

/** Retired or away for service: not reservable. */
const AWAY_ROLES = new Set(["retired", "out_for_calibration", "out_for_repair"]);

export function AssetDetailPage() {
  const { id } = useParams();
  const assetId = Number(id);
  const navigate = useNavigate();
  const { data: asset, error, loading, reload } = useAsync(() => api.getAsset(assetId), [assetId]);

  const [dialog, setDialog] = useState<{ open: boolean; record?: CalibrationRecord }>({ open: false });
  const [confirmArchive, setConfirmArchive] = useState(false);
  const [reserving, setReserving] = useState(false);
  const [requesting, setRequesting] = useState(false);
  const { can, user } = useAuth();
  const canEdit = can("edit_records");
  // Bumped after any save so the history and children re-load.
  const [version, setVersion] = useState(0);
  // Photos of the item go beside Details, as tall as it is.
  const photos = useItemPhotos(assetId, version);
  const [detailsCard, setDetailsCard] = useState<HTMLDivElement | null>(null);
  const detailsHeight = useHeight(detailsCard);

  if (error) return <ErrorAlert error={error} onRetry={reload} />;
  if (loading && !asset) return <Loading />;
  if (!asset) return null;

  const due = dueStatusMeta[asset.due_status];
  const isSystem = asset.kind === "system";

  /** Errors propagate to the confirm dialog, which shows them. */
  async function toggleArchived(reason: string) {
    const saved = await api.setAssetArchived(asset!.id, asset!.row_version, !asset!.is_archived, reason);
    toast.success(saved.is_archived ? `${saved.asset_tag} archived` : `${saved.asset_tag} restored`);
    setVersion((v) => v + 1);
    reload();
  }

  return (
    <div className="flex flex-col gap-6">
      <div>
        <Link
          to="/assets"
          className="mb-3 inline-flex items-center gap-1.5 text-sm text-muted hover:text-foreground"
        >
          <ArrowLeft className="size-4" /> Instruments & Systems
        </Link>
        <header className="flex items-start justify-between gap-4">
          <div className="min-w-0">
            <div className="flex flex-wrap items-center gap-2.5">
              {isSystem ? <Boxes className="size-6 text-muted" /> : <Gauge className="size-6 text-muted" />}
              <h1 className="text-2xl font-semibold tracking-tight">{asset.asset_tag}</h1>
              <Chip size="sm" variant="secondary">
                {isSystem ? "System" : "Instrument"}
              </Chip>
              <Chip size="sm" variant="soft" color={due.color}>
                {due.label}
              </Chip>
            </div>
            <p className="mt-1 text-sm text-muted">{asset.description}</p>
          </div>
          <div className="flex shrink-0 items-center gap-2">
            {/* Anyone can reserve a free instrument, or ask for one someone else has. */}
            {!isSystem && !asset.is_archived && asset.owner_id == null && !AWAY_ROLES.has(asset.status_role ?? "") && (
              <Button variant="secondary" onPress={() => setReserving(true)}>
                <CalendarPlus className="size-4" /> Reserve
              </Button>
            )}
            {!isSystem && !asset.is_archived && asset.owner_id != null && asset.owner_id !== user.owner_id && (
              <span title={user.owner_id ? undefined : "Ask an administrator to link your account to an Owner entry first."}>
                <Button variant="secondary" isDisabled={!user.owner_id} onPress={() => setRequesting(true)}>
                  <Hand className="size-4" /> Request instrument
                </Button>
              </span>
            )}
          </div>
          <div className={`flex shrink-0 items-center gap-2 ${canEdit ? "" : "hidden"}`}>
            <Button variant="secondary" onPress={() => navigate(`/assets/${asset.id}/edit`)}>
              <Pencil className="size-4" /> Edit
            </Button>
            <Button variant="secondary" onPress={() => navigate(`/assets/new?clone=${asset.id}`)}>
              <Copy className="size-4" /> Clone
            </Button>
            <Button
              variant={asset.is_archived ? "secondary" : "danger-soft"}
              onPress={() => setConfirmArchive(true)}
            >
              {asset.is_archived ? (
                <>
                  <ArchiveRestore className="size-4" /> Restore
                </>
              ) : (
                <>
                  <Archive className="size-4" /> Archive
                </>
              )}
            </Button>
          </div>
        </header>
      </div>

      {asset.is_archived && (
        <Alert status="warning">
          <Alert.Indicator />
          <Alert.Content>
            <Alert.Title>This item is archived</Alert.Title>
            <Alert.Description>
              It's hidden from lists and due tracking. Restore it to record new service.
            </Alert.Description>
          </Alert.Content>
        </Alert>
      )}

      <div
        className={`grid grid-cols-1 gap-6 ${photos.length ? "xl:grid-cols-[auto_minmax(0,3fr)_minmax(0,2fr)]" : "xl:grid-cols-[3fr_2fr]"}`}
      >
        {photos.length > 0 && <ItemPhotos photos={photos} detailsHeight={detailsHeight} />}
        <div ref={setDetailsCard} className="min-w-0">
        <Card className="h-full">
          <Card.Header>
            <Card.Title>Details</Card.Title>
          </Card.Header>
          <Card.Content>
            <dl className="grid grid-cols-2 gap-x-6 gap-y-4 md:grid-cols-3">
              <Detail label="Manufacturer">{asset.manufacturer_name}</Detail>
              <Detail label="Model number">{asset.model_number}</Detail>
              <Detail label="Serial number">{asset.serial_number}</Detail>
              <Detail label="Status">{asset.status_name}</Detail>
              <Detail label="Location">{asset.location_name}</Detail>
              <Detail label="Department">{asset.department_name}</Detail>
              <Detail label="Owner">{asset.owner_name}</Detail>
              {!isSystem && (
                <Detail label="Part of System">
                  {asset.parent_id ? (
                    <Link to={`/assets/${asset.parent_id}`} className="font-medium text-accent hover:underline">
                      {asset.parent_tag}
                    </Link>
                  ) : (
                    "Standalone"
                  )}
                </Detail>
              )}
              <Detail label="Warranty ends">{formatDate(asset.warranty_end_date)}</Detail>
            </dl>
            {asset.notes && (
              <div className="mt-5 border-t border-separator pt-4">
                <div className="mb-1 text-xs text-muted">Notes</div>
                <p className="text-sm whitespace-pre-wrap">{asset.notes}</p>
              </div>
            )}
          </Card.Content>
        </Card>
        </div>

        <Card>
          <Card.Header>
            <Card.Title>Calibration</Card.Title>
          </Card.Header>
          <Card.Content>
            <dl className="grid grid-cols-2 gap-x-6 gap-y-4">
              <Detail label="Next calibration">
                <span className="text-base font-semibold">{formatDate(asset.next_cal_date)}</span>
              </Detail>
              <Detail label="Interval">{formatSchedule(asset)}</Detail>
              <Detail label="Last calibration">{formatDate(asset.last_cal_date)}</Detail>
              <Detail label="First calibration">{formatDate(asset.first_cal_date)}</Detail>
              <Detail label="Put into service">{formatDate(asset.in_service_date)}</Detail>
            </dl>
          </Card.Content>
        </Card>
      </div>

      {isSystem && <ChildrenCard system={asset} />}

      <HistoryCard
        asset={asset}
        version={version}
        onAdd={() => setDialog({ open: true })}
        onEdit={(record) => setDialog({ open: true, record })}
      />

      <CollapsibleCard
        title="Files"
        description="Certificates, manuals, and photos for this item and its service records. Selecting a file opens it."
        rememberAs="asset-files"
      >
        <AttachmentsPanel
          key={version}
          assetId={asset.id}
          readOnly={asset.is_archived}
          onChanged={() => setVersion((v) => v + 1)}
        />
      </CollapsibleCard>

      <ChangeHistoryCard asset={asset} version={version} />

      <CalibrationDialog
        isOpen={dialog.open}
        onOpenChange={(open) => setDialog((d) => ({ ...d, open }))}
        record={dialog.record}
        asset={asset}
        onSaved={() => {
          setVersion((v) => v + 1);
          reload();
        }}
      />

      <ReserveDialog isOpen={reserving} onOpenChange={setReserving} asset={asset} />
      <ConfirmDialog
        isOpen={requesting}
        onOpenChange={setRequesting}
        title={`Request ${asset.asset_tag}?`}
        confirmLabel="Send request"
        tone="accent"
        askReason
        reasonLabel="Note (optional)"
        onConfirm={async (note) => {
          const r = await api.requestInstrument(asset.id, note);
          toast.success(
            r.approver_name
              ? `Asked ${r.approver_name} to hand over ${r.asset_tag}.`
              : `Requested ${r.asset_tag}. A Technician or Administrator will approve it.`,
          );
          ownerRequestsChanged();
          setVersion((v) => v + 1);
        }}
      >
        It's checked out to {asset.owner_name}. Asks them to hand it over to you
        {user.owner_name ? ` (${user.owner_name})` : ""}; they can approve or decline.
      </ConfirmDialog>
      <ConfirmDialog
        isOpen={confirmArchive}
        onOpenChange={setConfirmArchive}
        title={asset.is_archived ? `Restore ${asset.asset_tag}?` : `Archive ${asset.asset_tag}?`}
        confirmLabel={asset.is_archived ? "Restore" : "Archive"}
        tone={asset.is_archived ? "accent" : "danger"}
        askReason
        onConfirm={toggleArchived}
      >
        {asset.is_archived
          ? "The item returns to lists and due tracking."
          : "Archived items are hidden from lists and due tracking. Their service history is kept, and you can restore them at any time."}
      </ConfirmDialog>
    </div>
  );
}
