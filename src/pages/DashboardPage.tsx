import { Card, Table } from "@heroui/react";
import { CalendarCheck, CalendarClock, CircleAlert, CircleCheck, CircleDashed, Clock } from "lucide-react";
import type { ReactNode } from "react";
import { useNavigate } from "react-router";
import type { Asset } from "../bindings/Asset";
import { EmptyNote, ErrorAlert, Loading, PageHeader, StatusChip } from "../components/common";
import { api } from "../lib/api";
import { dueStatusMeta, EMPTY, formatDate } from "../lib/format";
import { useAsync } from "../lib/useAsync";

function StatCard({
  label,
  value,
  icon,
  tone,
  hint,
}: {
  label: string;
  /** A small line under the number, e.g. the exact range. */
  hint?: string;
  value: number;
  icon: ReactNode;
  tone: "danger" | "warning" | "accent" | "success" | "default";
}) {
  const toneClass = {
    danger: "bg-danger-soft text-danger-soft-foreground",
    warning: "bg-warning-soft text-warning-soft-foreground",
    accent: "bg-accent-soft text-accent-soft-foreground",
    success: "bg-success-soft text-success-soft-foreground",
    default: "bg-default text-muted",
  }[tone];
  return (
    <Card className="gap-3 p-4">
      <div className="flex items-center justify-between">
        <span className="text-sm text-muted">{label}</span>
        <span className={`grid size-8 place-items-center rounded-lg ${toneClass}`}>{icon}</span>
      </div>
      <div>
        <div className="text-3xl font-semibold tracking-tight">{value}</div>
        {hint && <div className="text-xs text-muted">{hint}</div>}
      </div>
    </Card>
  );
}

function AttentionTable({ assets }: { assets: Asset[] }) {
  const navigate = useNavigate();
  return (
    <Table>
      <Table.ScrollContainer>
        <Table.Content
          aria-label="Items overdue or due within 30 days"
          onRowAction={(key) => navigate(`/assets/${key}`)}
        >
          <Table.Header>
            <Table.Column isRowHeader>Asset tag</Table.Column>
            <Table.Column>Description</Table.Column>
            <Table.Column>Location</Table.Column>
            <Table.Column>Next due</Table.Column>
            <Table.Column>Due status</Table.Column>
          </Table.Header>
          <Table.Body
            items={assets}
            renderEmptyState={() => (
              <EmptyNote icon={<CircleCheck className="size-5" />} title="All caught up">
                Nothing is overdue or due in the next 30 days.
              </EmptyNote>
            )}
          >
            {(a) => (
              <Table.Row id={a.id} className="cursor-pointer">
                <Table.Cell className="font-medium">{a.asset_tag}</Table.Cell>
                <Table.Cell>{a.description}</Table.Cell>
                <Table.Cell>{a.location_name ?? EMPTY}</Table.Cell>
                <Table.Cell>{formatDate(a.next_cal_date)}</Table.Cell>
                <Table.Cell>
                  <StatusChip color={dueStatusMeta[a.due_status].color}>
                    {dueStatusMeta[a.due_status].label}
                  </StatusChip>
                </Table.Cell>
              </Table.Row>
            )}
          </Table.Body>
        </Table.Content>
      </Table.ScrollContainer>
    </Table>
  );
}

export function DashboardPage() {
  const { data, error, loading, reload } = useAsync(() => api.dueSummary(), []);

  return (
    <>
      <PageHeader
        title="Dashboard"
        description={data ? `Calibration status as of ${formatDate(data.today)}` : undefined}
      />
      {error && <ErrorAlert error={error} onRetry={reload} />}
      {loading && !data && <Loading />}
      {data && (
        <div className="flex flex-col gap-8">
          <div className="grid grid-cols-2 gap-4 lg:grid-cols-3 xl:grid-cols-5">
            <StatCard label="Overdue" value={data.overdue} tone="danger" icon={<CircleAlert className="size-4" />} />
            <StatCard label="Due in 30 days" value={data.due_30} tone="warning" icon={<Clock className="size-4" />} />
            <StatCard
              label="Due within 90 days"
              hint="In 31 to 90 days"
              value={data.due_31_90}
              tone="accent"
              icon={<CalendarClock className="size-4" />}
            />
            <StatCard label="Current" value={data.current} tone="success" icon={<CalendarCheck className="size-4" />} />
            <StatCard label="No schedule" value={data.no_schedule} tone="default" icon={<CircleDashed className="size-4" />} />
          </div>

          <section>
            <div className="mb-3 flex items-baseline justify-between">
              <h2 className="text-lg font-semibold">Needs attention</h2>
              <span className="text-sm text-muted">
                {data.total} tracked item{data.total === 1 ? "" : "s"}
                {data.not_tracked > 0 && ` · ${data.not_tracked} not tracked`}
              </span>
            </div>
            <AttentionTable assets={data.attention} />
          </section>
        </div>
      )}
    </>
  );
}
