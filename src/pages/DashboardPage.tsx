import { Button, Card, Table } from "@heroui/react";
import {
  CalendarCheck,
  CalendarClock,
  CircleAlert,
  CircleCheck,
  CircleDashed,
  Clock,
  X,
} from "lucide-react";
import type { ReactNode } from "react";
import { useNavigate, useSearchParams } from "react-router";
import type { Asset } from "../bindings/Asset";
import type { DueBucket } from "../bindings/DueBucket";
import type { DueSummary } from "../bindings/DueSummary";
import {
  EmptyNote,
  ErrorAlert,
  Loading,
  PageHeader,
  StatusChip,
} from "../components/common";
import { api } from "../lib/api";
import { dueStatusMeta, EMPTY, formatDate } from "../lib/format";
import { useAsync } from "../lib/useAsync";
import { WaitingForYou } from "../components/WaitingForYou";

/** One dashboard box: what it counts, how it looks, and what its filtered list says when empty. */
type Stat = {
  bucket: DueBucket;
  label: string;
  icon: ReactNode;
  /** Icon tile colors. */
  tile: string;
  /** Sets `--glow`, the active box's glow color, to match the icon. */
  glow: string;
  empty: string;
};

const STATS: Stat[] = [
  {
    bucket: "overdue",
    label: "Overdue",
    icon: <CircleAlert className="size-4" />,
    tile: "bg-danger-soft text-danger-soft-foreground",
    glow: "[--glow:var(--color-danger)]",
    empty: "Nothing is overdue.",
  },
  {
    bucket: "due_30",
    label: "Due in 30 days",
    icon: <Clock className="size-4" />,
    tile: "bg-warning-soft text-warning-soft-foreground",
    glow: "[--glow:var(--color-warning)]",
    empty: "Nothing is due in the next 30 days.",
  },
  {
    bucket: "due_31_90",
    label: "Due in 31–90 days",
    icon: <CalendarClock className="size-4" />,
    tile: "bg-accent-soft text-accent-soft-foreground",
    glow: "[--glow:var(--color-accent)]",
    empty: "Nothing is due in 31 to 90 days.",
  },
  {
    bucket: "current",
    label: "Current",
    icon: <CalendarCheck className="size-4" />,
    tile: "bg-success-soft text-success-soft-foreground",
    glow: "[--glow:var(--color-success)]",
    empty: "No items are due more than 90 days out.",
  },
  {
    bucket: "no_schedule",
    label: "No schedule",
    icon: <CircleDashed className="size-4" />,
    tile: "bg-default text-muted",
    // White doesn't show on a light background, so gray there.
    glow: "[--glow:var(--color-muted)] dark:[--glow:#fff]",
    empty: "Every tracked item has a due date.",
  },
];

const BUCKETS = new Set<string>(STATS.map((s) => s.bucket));

/** Has an interval but no calibration on record, so it needs calibrating now. */
function needsFirstCalibration(a: Asset) {
  return (
    a.due_status === "no_schedule" &&
    a.cal_interval != null &&
    a.last_cal_date == null
  );
}

function StatCard({
  stat,
  value,
  active,
  onToggle,
}: {
  stat: Stat;
  value: number;
  active: boolean;
  onToggle: () => void;
}) {
  return (
    <button
      type="button"
      aria-pressed={active}
      onClick={onToggle}
      title={
        active
          ? "Show everything that needs attention"
          : `Show only: ${stat.label.toLowerCase()}`
      }
      className={`group block w-full cursor-pointer rounded-3xl text-left outline-none ${stat.glow}`}
    >
      <Card
        className={`gap-3 p-4 transition-shadow duration-200 group-hover:bg-default/40 group-focus-visible:ring-2 group-focus-visible:ring-focus ${
          active
            ? "shadow-[0_0_0_2px_var(--glow),0_0_22px_3px_color-mix(in_oklab,var(--glow)_55%,transparent)]!"
            : ""
        }`}
      >
        <div className="flex items-center justify-between">
          <span className="text-sm text-muted">{stat.label}</span>
          <span
            className={`grid size-8 place-items-center rounded-lg ${stat.tile}`}
          >
            {stat.icon}
          </span>
        </div>
        <div className="text-3xl font-semibold tracking-tight">{value}</div>
      </Card>
    </button>
  );
}

function DueChip({ asset }: { asset: Asset }) {
  if (needsFirstCalibration(asset))
    return <StatusChip color="danger">Needs calibration</StatusChip>;
  const meta = dueStatusMeta[asset.due_status];
  return <StatusChip color={meta.color}>{meta.label}</StatusChip>;
}

/** The due status, plus the item's status when it's away for calibration or repair. */
function DueChips({ asset }: { asset: Asset }) {
  const away =
    asset.status_role === "out_for_calibration" ||
    asset.status_role === "out_for_repair";
  return (
    <div className="flex flex-wrap items-center gap-1.5">
      <DueChip asset={asset} />
      {away && <StatusChip color="accent">{asset.status_name}</StatusChip>}
    </div>
  );
}

function AttentionTable({
  assets,
  label,
  empty,
}: {
  assets: Asset[];
  label: string;
  empty: ReactNode;
}) {
  const navigate = useNavigate();
  return (
    <Table>
      <Table.ScrollContainer>
        <Table.Content
          aria-label={label}
          onRowAction={(key) => navigate(`/assets/${key}`)}
        >
          <Table.Header>
            <Table.Column isRowHeader>Asset tag</Table.Column>
            <Table.Column>Description</Table.Column>
            <Table.Column>Location</Table.Column>
            <Table.Column>Next due</Table.Column>
            <Table.Column>Due status</Table.Column>
          </Table.Header>
          <Table.Body items={assets} renderEmptyState={() => empty}>
            {(a) => (
              <Table.Row id={a.id} className="cursor-pointer">
                <Table.Cell className="font-medium">{a.asset_tag}</Table.Cell>
                <Table.Cell>{a.description}</Table.Cell>
                <Table.Cell>
                  {a.location_name ?? EMPTY}
                  {a.parent_tag && (
                    <div className="text-xs text-muted">Part of System {a.parent_tag}</div>
                  )}
                </Table.Cell>
                <Table.Cell>{formatDate(a.next_cal_date)}</Table.Cell>
                <Table.Cell>
                  <DueChips asset={a} />
                </Table.Cell>
              </Table.Row>
            )}
          </Table.Body>
        </Table.Content>
      </Table.ScrollContainer>
    </Table>
  );
}

function count(data: DueSummary, bucket: DueBucket): number {
  return data[bucket];
}

export function DashboardPage() {
  const { data, error, loading, reload } = useAsync(() => api.dueSummary(), []);
  // The filter lives in the URL so it's still set when coming back from an item.
  const [params, setParams] = useSearchParams();
  const shown = params.get("show");
  const filter = shown && BUCKETS.has(shown) ? (shown as DueBucket) : null;
  const active = STATS.find((s) => s.bucket === filter) ?? null;

  function toggle(bucket: DueBucket) {
    setParams(bucket === filter ? {} : { show: bucket }, { replace: true });
  }

  const assets = data
    ? filter
      ? data.items.filter((i) => i.bucket === filter).map((i) => i.asset)
      : data.attention
    : [];

  return (
    <>
      <PageHeader
        title="Dashboard"
        description={
          data
            ? `Calibration status as of ${formatDate(data.today)}`
            : undefined
        }
      />
      {error && <ErrorAlert error={error} onRetry={reload} />}
      {loading && !data && <Loading />}
      {data && (
        <div className="flex flex-col gap-8">
          <div className="grid grid-cols-2 gap-4 lg:grid-cols-3 xl:grid-cols-5">
            {STATS.map((s) => (
              <StatCard
                key={s.bucket}
                stat={s}
                value={count(data, s.bucket)}
                active={filter === s.bucket}
                onToggle={() => toggle(s.bucket)}
              />
            ))}
          </div>

          <section>
            <div className="mb-3 flex items-center justify-between gap-4">
              <div className="flex items-center gap-2">
                <h2 className="text-lg font-semibold">
                  {active ? active.label : "Needs attention"}
                </h2>
                {active && (
                  <Button
                    size="sm"
                    variant="ghost"
                    onPress={() => setParams({}, { replace: true })}
                  >
                    <X className="size-3.5" />
                    Clear filter
                  </Button>
                )}
              </div>
              <span className="text-sm text-muted">
                {active
                  ? `${assets.length} of ${data.total} item${data.total === 1 ? "" : "s"}`
                  : `${data.total} tracked item${data.total === 1 ? "" : "s"}`}
                {!active &&
                  data.not_tracked > 0 &&
                  ` · ${data.not_tracked} not tracked`}
              </span>
            </div>
            <AttentionTable
              assets={assets}
              label={
                active ? `Items: ${active.label}` : "Items needing attention"
              }
              empty={
                <EmptyNote
                  icon={<CircleCheck className="size-5" />}
                  title={active ? "No items" : "All caught up"}
                >
                  {active
                    ? active.empty
                    : "Nothing is overdue, due in the next 30 days, or waiting for its first calibration."}
                </EmptyNote>
              }
            />
          </section>

          <WaitingForYou />
        </div>
      )}
    </>
  );
}
