import { Button, Card, Checkbox } from "@heroui/react";
import { ArrowLeft, Printer } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router";
import { ErrorAlert, Loading, PageHeader } from "../components/common";
import { DateInput, NumberInput, SelectInput, TextInput } from "../components/fields";
import { usePageStyle } from "../components/ReportView";
import { api } from "../lib/api";
import { formatDate } from "../lib/format";
import { lookupOptions, useLookups } from "../lib/lookups";
import { LABEL_SHEETS, type LabelData, type LabelSheet, labelData, paginateLabels } from "../lib/reports";
import { useAsync } from "../lib/useAsync";

function Label({ label, sheet }: { label: LabelData | null; sheet: LabelSheet }) {
  const box = { width: sheet.width, height: sheet.height };
  if (!label) return <div style={box} />;
  return (
    <div
      style={box}
      className="flex flex-col justify-center overflow-hidden px-[0.08in] py-[0.04in] leading-tight text-black"
    >
      <div className="flex items-baseline justify-between gap-2">
        <span className={`truncate font-bold ${sheet.roomy ? "text-[16pt]" : "text-[11pt]"}`}>{label.tag}</span>
        <span className="shrink-0 text-[6pt] font-semibold tracking-wider uppercase">Calibrated</span>
      </div>
      {sheet.roomy && <div className="truncate text-[8pt]">{label.description}</div>}
      <div className={`flex gap-3 ${sheet.roomy ? "mt-1 text-[9pt]" : "text-[7pt]"}`}>
        <span>
          Cal: <b>{label.calibrated ? formatDate(label.calibrated) : "—"}</b>
        </span>
        <span>
          Due: <b>{label.due ? formatDate(label.due) : "—"}</b>
        </span>
      </div>
      <div className={`truncate ${sheet.roomy ? "text-[8pt]" : "text-[6.5pt]"}`}>By: {label.by ?? "—"}</div>
    </div>
  );
}

function Sheet({ labels, sheet }: { labels: (LabelData | null)[]; sheet: LabelSheet }) {
  return (
    <div
      className="labels-sheet mx-auto mb-6 bg-white shadow-md ring-1 ring-black/10 print:m-0 print:shadow-none print:ring-0"
      style={{
        width: sheet.page === "A4" ? "210mm" : sheet.page === "letter" ? "8.5in" : sheet.width,
        minHeight: sheet.page === "A4" ? "297mm" : sheet.page === "letter" ? "11in" : sheet.height,
        paddingTop: sheet.marginTop,
        paddingLeft: sheet.marginLeft,
      }}
    >
      <div
        className="grid"
        style={{
          gridTemplateColumns: `repeat(${sheet.columns}, ${sheet.width})`,
          gridAutoRows: sheet.height,
          columnGap: sheet.columnGap,
          rowGap: sheet.rowGap,
        }}
      >
        {labels.map((l, i) => (
          <div key={i} className="outline-1 outline-dashed outline-gray-300 print:outline-0">
            <Label label={l} sheet={sheet} />
          </div>
        ))}
      </div>
    </div>
  );
}

export function LabelsPage() {
  const navigate = useNavigate();
  const data = useAsync(async () => {
    const [assets, calibrations] = await Promise.all([api.listAssets({}), api.listCalibrations({})]);
    return { assets, labels: labelData(assets, calibrations) };
  }, []);
  const lookups = useLookups();

  const [sheetId, setSheetId] = useState(LABEL_SHEETS[0].id);
  const [skip, setSkip] = useState<number | null>(0);
  const [locationId, setLocationId] = useState<number | null>(null);
  const [calibratedSince, setCalibratedSince] = useState<string | null>(null);
  const [search, setSearch] = useState("");
  const [excluded, setExcluded] = useState<Set<number>>(new Set());
  const sheet = LABEL_SHEETS.find((s) => s.id === sheetId)!;
  const perPage = sheet.columns * sheet.rows;

  const matching = useMemo(() => {
    if (!data.data) return [];
    const byId = new Map(data.data.assets.map((a) => [a.id, a]));
    const q = search.trim().toLowerCase();
    return data.data.labels.filter((l) => {
      const a = byId.get(l.assetId)!;
      return (
        a.next_cal_date != null &&
        (locationId == null || a.location_id === locationId) &&
        (calibratedSince == null || (l.calibrated != null && l.calibrated >= calibratedSince)) &&
        (!q || l.tag.toLowerCase().includes(q) || l.description.toLowerCase().includes(q))
      );
    });
  }, [data.data, locationId, calibratedSince, search]);
  const chosen = matching.filter((l) => !excluded.has(l.assetId));
  const pages = paginateLabels(chosen, perPage, skip ?? 0);

  useEffect(() => setSkip(0), [sheetId]);
  usePageStyle(`@page { size: ${sheet.page}; margin: 0; } .labels-sheet { break-after: page; }`);

  function toggle(id: number, on: boolean) {
    setExcluded((prev) => {
      const next = new Set(prev);
      if (on) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  return (
    <>
      <div className="print:hidden">
        <Button variant="ghost" size="sm" className="mb-3" onPress={() => navigate("/reports")}>
          <ArrowLeft className="size-4" /> Reports
        </Button>
        <PageHeader
          title="Calibration labels"
          description="Stickers showing each item's tag, when it was calibrated, by whom, and when it's due."
          actions={
            <Button isDisabled={chosen.length === 0} onPress={() => window.print()}>
              <Printer className="size-4" /> Print {chosen.length} label{chosen.length === 1 ? "" : "s"}
            </Button>
          }
        />
        {data.error && <ErrorAlert error={data.error} onRetry={data.reload} />}
        {data.loading && !data.data && <Loading />}

        {data.data && (
          <div className="mb-6 grid grid-cols-1 gap-6 lg:grid-cols-[340px_1fr]">
            <Card>
              <Card.Content className="flex flex-col gap-4">
                <SelectInput<string>
                  label="Label sheet"
                  value={sheetId}
                  onChange={(v) => v && setSheetId(v)}
                  options={LABEL_SHEETS.map((s) => ({ id: s.id, label: s.label, description: s.description }))}
                  isRequired
                />
                {perPage > 1 && (
                  <NumberInput
                    label="Labels already used on the first sheet"
                    value={skip}
                    onChange={(v) => setSkip(v == null ? null : Math.max(0, Math.min(perPage - 1, Math.trunc(v))))}
                    minValue={0}
                    step={1}
                    description="Printing starts after them, so a part-used sheet isn't wasted."
                  />
                )}
                <SelectInput<number>
                  label="Location"
                  value={locationId}
                  onChange={setLocationId}
                  options={lookupOptions(lookups.data, "location", locationId)}
                  placeholder="All locations"
                />
                <DateInput
                  label="Calibrated on or after"
                  value={calibratedSince}
                  onChange={setCalibratedSince}
                  description="For example, just the items back from today's calibrations."
                />
                <p className="text-xs text-muted">
                  In the print dialog, set the scale to 100% (actual size) and margins to none so labels line up.
                </p>
              </Card.Content>
            </Card>

            <Card>
              <Card.Header className="flex-row items-end justify-between gap-3">
                <div>
                  <Card.Title>Items</Card.Title>
                  <Card.Description>
                    {chosen.length} of {matching.length} selected. Only items with a due date are listed.
                  </Card.Description>
                </div>
                <div className="flex items-end gap-2">
                  <TextInput label="Search" value={search} onChange={setSearch} placeholder="Tag or description" />
                  <Button size="sm" variant="ghost" onPress={() => setExcluded(new Set())}>
                    All
                  </Button>
                  <Button size="sm" variant="ghost" onPress={() => setExcluded(new Set(matching.map((l) => l.assetId)))}>
                    None
                  </Button>
                </div>
              </Card.Header>
              <Card.Content>
                <div className="max-h-72 overflow-y-auto rounded-lg border border-separator">
                  {matching.length === 0 ? (
                    <p className="px-3 py-6 text-center text-sm text-muted">No items match.</p>
                  ) : (
                    matching.map((l) => (
                      <div key={l.assetId} className="flex items-center gap-3 border-b border-separator px-3 py-1.5 text-sm last:border-b-0">
                        <Checkbox
                          isSelected={!excluded.has(l.assetId)}
                          onChange={(on) => toggle(l.assetId, on)}
                          aria-label={`Print a label for ${l.tag}`}
                        >
                          <Checkbox.Content className="cursor-pointer">
                            <Checkbox.Control>
                              <Checkbox.Indicator />
                            </Checkbox.Control>
                          </Checkbox.Content>
                        </Checkbox>
                        <span className="w-28 shrink-0 font-medium">{l.tag}</span>
                        <span className="min-w-0 flex-1 truncate text-muted">{l.description}</span>
                        <span className="shrink-0 text-xs text-muted">Due {formatDate(l.due)}</span>
                      </div>
                    ))
                  )}
                </div>
              </Card.Content>
            </Card>
          </div>
        )}
        {pages.length > 0 && (
          <h2 className="mb-3 text-sm font-semibold">
            Preview · {pages.length} {sheet.columns * sheet.rows > 1 ? "sheet" : "label"}
            {pages.length === 1 ? "" : "s"}
          </h2>
        )}
      </div>

      <div className="overflow-x-auto print:overflow-visible">
        {pages.map((labels, i) => (
          <Sheet key={i} labels={labels} sheet={sheet} />
        ))}
      </div>
    </>
  );
}
