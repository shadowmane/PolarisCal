import { Button, ToggleButton, ToggleButtonGroup } from "@heroui/react";
import { ArrowLeft, Tags } from "lucide-react";
import { useMemo, useState } from "react";
import { useNavigate, useParams, useSearchParams } from "react-router";
import { ErrorAlert, Loading, PageHeader } from "../components/common";
import { DateInput, SelectInput, ToggleSwitch } from "../components/fields";
import { ReportBody, ReportToolbar } from "../components/ReportView";
import { api } from "../lib/api";
import { lookupOptions, useLookups } from "../lib/lookups";
import {
  addDays,
  dueReport,
  historyReport,
  isoToday,
  outOfToleranceReport,
  registerReport,
  type ReportTable,
} from "../lib/reports";
import { useAsync } from "../lib/useAsync";

type ReportId = "due" | "oot" | "register";

const REPORTS: { id: ReportId; label: string; description: string }[] = [
  { id: "due", label: "Due and overdue", description: "What needs calibrating soon, soonest first." },
  {
    id: "oot",
    label: "Out of tolerance",
    description: "Service records where the item arrived out of tolerance or inoperative, for impact assessments.",
  },
  { id: "register", label: "Instrument register", description: "Every Instrument and System with its schedule." },
];

const WINDOWS = [
  { id: 0, label: "Overdue only" },
  { id: 30, label: "Due within 30 days" },
  { id: 60, label: "Due within 60 days" },
  { id: 90, label: "Due within 90 days" },
  { id: 365, label: "Due within a year" },
];

function nameOf(list: { id: number; name: string }[] | undefined, id: number | null): string | undefined {
  return id == null ? undefined : list?.find((i) => i.id === id)?.name;
}

export function ReportsPage() {
  const navigate = useNavigate();
  const [params, setParams] = useSearchParams();
  const report = (REPORTS.find((r) => r.id === params.get("report"))?.id ?? "due") as ReportId;
  const today = isoToday();

  const assets = useAsync(() => api.listAssets({ include_archived: true }), []);
  const lookups = useLookups();

  // Due list filters
  const [withinDays, setWithinDays] = useState(30);
  const [locationId, setLocationId] = useState<number | null>(null);
  const [departmentId, setDepartmentId] = useState<number | null>(null);
  const [ownerId, setOwnerId] = useState<number | null>(null);
  // Out-of-tolerance range
  const [from, setFrom] = useState<string | null>(addDays(today, -365));
  const [to, setTo] = useState<string | null>(today);
  // Register
  const [includeArchived, setIncludeArchived] = useState(false);

  const calibrations = useAsync(
    () =>
      report === "oot" && from && to
        ? api.listCalibrations({ from_date: from, to_date: to })
        : Promise.resolve(null),
    [report, from, to],
  );

  const table: ReportTable | null = useMemo(() => {
    const items = assets.data;
    if (!items) return null;
    const l = lookups.data;
    if (report === "due") {
      return dueReport(items, { withinDays, locationId, departmentId, ownerId }, today, {
        location: nameOf(l?.location, locationId),
        department: nameOf(l?.department, departmentId),
        owner: nameOf(l?.owner, ownerId),
      });
    }
    if (report === "oot") {
      if (!from || !to || !calibrations.data) return null;
      return outOfToleranceReport(calibrations.data, items, from, to);
    }
    const filtered = items.filter(
      (a) =>
        (locationId == null || a.location_id === locationId) &&
        (departmentId == null || a.department_id === departmentId) &&
        (ownerId == null || a.owner_id === ownerId),
    );
    const note = [nameOf(l?.location, locationId), nameOf(l?.department, departmentId), nameOf(l?.owner, ownerId)]
      .filter(Boolean)
      .join(" · ");
    return registerReport(filtered, includeArchived, note);
  }, [assets.data, lookups.data, report, withinDays, locationId, departmentId, ownerId, today, from, to, calibrations.data, includeArchived]);

  const meta = REPORTS.find((r) => r.id === report)!;
  const error = assets.error ?? calibrations.error;

  return (
    <>
      <div className="print:hidden">
        <PageHeader
          title="Reports"
          description={meta.description}
          actions={
            <Button variant="tertiary" onPress={() => navigate("/reports/labels")}>
              <Tags className="size-4" /> Calibration labels
            </Button>
          }
        />
        <div className="mb-5 flex flex-wrap items-end justify-between gap-4">
          <ToggleButtonGroup
            selectionMode="single"
            disallowEmptySelection
            selectedKeys={[report]}
            onSelectionChange={(keys) => {
              const [next] = [...keys];
              if (next) setParams({ report: String(next) }, { replace: true });
            }}
            aria-label="Report"
          >
            {REPORTS.map((r) => (
              <ToggleButton key={r.id} id={r.id}>
                {r.label}
              </ToggleButton>
            ))}
          </ToggleButtonGroup>
          {table && <ReportToolbar report={table} fileName={meta.label} />}
        </div>

        <div className="mb-5 grid grid-cols-2 gap-3 lg:grid-cols-4">
          {report === "due" && (
            <SelectInput<number>
              label="Show"
              value={withinDays}
              onChange={(v) => setWithinDays(v ?? 30)}
              options={WINDOWS}
              isRequired
            />
          )}
          {report === "oot" ? (
            <>
              <DateInput label="From" value={from} onChange={setFrom} isRequired />
              <DateInput label="To" value={to} onChange={setTo} isRequired />
            </>
          ) : (
            <>
              <SelectInput<number>
                label="Location"
                value={locationId}
                onChange={setLocationId}
                options={lookupOptions(lookups.data, "location", locationId)}
                placeholder="All locations"
              />
              <SelectInput<number>
                label="Department"
                value={departmentId}
                onChange={setDepartmentId}
                options={lookupOptions(lookups.data, "department", departmentId)}
                placeholder="All departments"
              />
              <SelectInput<number>
                label="Owner"
                value={ownerId}
                onChange={setOwnerId}
                options={lookupOptions(lookups.data, "owner", ownerId)}
                placeholder="All owners"
              />
            </>
          )}
          {report === "register" && (
            <div className="flex items-end pb-2">
              <ToggleSwitch label="Include archived" isSelected={includeArchived} onChange={setIncludeArchived} />
            </div>
          )}
        </div>
        {table?.subtitle && <p className="mb-3 text-sm text-muted">{table.subtitle}</p>}
      </div>

      {error && <ErrorAlert error={error} />}
      {!table && !error && (report !== "oot" || (from && to)) && <Loading />}
      {report === "oot" && (!from || !to) && <p className="text-sm text-muted">Choose a date range.</p>}
      {table && (
        <ReportBody
          report={table}
          emptyText={
            report === "due"
              ? "Nothing is overdue or due in this period."
              : report === "oot"
                ? "No items arrived out of tolerance or inoperative in this period."
                : "No items match."
          }
        />
      )}
    </>
  );
}

/** One item's service history, for printing or Excel. */
export function HistoryReportPage() {
  const { id } = useParams();
  const assetId = Number(id);
  const navigate = useNavigate();
  const data = useAsync(async () => {
    const [asset, calibrations] = await Promise.all([
      api.getAsset(assetId),
      api.listCalibrations({ asset_id: assetId }),
    ]);
    return historyReport(asset, calibrations);
  }, [assetId]);

  return (
    <>
      <div className="print:hidden">
        <Button variant="ghost" size="sm" className="mb-3" onPress={() => navigate(`/assets/${assetId}`)}>
          <ArrowLeft className="size-4" /> Back to the item
        </Button>
        <PageHeader
          title={data.data?.title ?? "Service history"}
          description={data.data?.subtitle}
          actions={data.data && <ReportToolbar report={data.data} fileName={data.data.title.replace(":", "")} />}
        />
      </div>
      {data.error && <ErrorAlert error={data.error} onRetry={data.reload} />}
      {data.loading && !data.data && <Loading />}
      {data.data && <ReportBody report={data.data} emptyText="No service recorded for this item yet." />}
    </>
  );
}
