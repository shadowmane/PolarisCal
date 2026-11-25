import { describe, expect, it } from "vitest";
import type { Asset } from "../bindings/Asset";
import type { CalibrationRecord } from "../bindings/CalibrationRecord";
import {
  addDays,
  daysBetween,
  dueReport,
  historyReport,
  isoToday,
  labelData,
  outOfToleranceReport,
  paginateLabels,
  registerReport,
  toSheet,
} from "./reports";

function asset(id: number, tag: string, over: Partial<Asset> = {}): Asset {
  return {
    id,
    asset_tag: tag,
    kind: "instrument",
    parent_id: null,
    parent_tag: null,
    description: `${tag} description`,
    manufacturer_id: null,
    manufacturer_name: "Fluke",
    model_number: "87V",
    serial_number: null,
    location_id: 1,
    location_name: "Lab A",
    department_id: null,
    department_name: null,
    owner_id: null,
    owner_name: null,
    owner_since: null,
    status_id: 1,
    status_name: "Active",
    status_role: null,
    previous_status_id: null,
    warranty_end_date: null,
    cal_interval: { value: 12, unit: "months", end_of_month: false },
    in_service_date: null,
    first_cal_date: null,
    last_cal_date: null,
    last_cal_date_entered: null,
    next_cal_date: null,
    calibration_extended: false,
    cal_aggregate: false,
    due_status: "no_schedule",
    child_count: 0,
    notes: null,
    is_archived: false,
    row_version: 1,
    created_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-01-01T00:00:00Z",
    ...over,
  };
}

function cal(id: number, assetId: number, date: string, over: Partial<CalibrationRecord> = {}): CalibrationRecord {
  return {
    id,
    asset_id: assetId,
    asset_tag: `A${assetId}`,
    cal_date: date,
    service_type_id: 1,
    service_type_name: "Calibration",
    counts_as_calibration: true,
    is_extension: false,
    is_repair: false,
    extension_days: null,
    calibrated_by: "provider",
    calibrated_by_id: 1,
    calibrated_by_name: "Acme Labs",
    technician: null,
    cost_cents: 12_500,
    certificate_number: "C-1",
    condition_received: "pass",
    condition_returned: "pass",
    next_due_date: null,
    notes: null,
    attachment_count: 0,
    is_archived: false,
    row_version: 1,
    created_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-01-01T00:00:00Z",
    ...over,
  };
}

describe("date helpers", () => {
  it("counts days across month ends and DST changes", () => {
    expect(daysBetween("2026-03-01", "2026-03-31")).toBe(30);
    expect(daysBetween("2026-11-20", "2026-11-10")).toBe(-10);
    expect(addDays("2026-02-27", 2)).toBe("2026-03-01");
    expect(isoToday(new Date(2026, 0, 5))).toBe("2026-01-05");
  });
});

describe("dueReport", () => {
  const today = "2026-11-20";
  const assets = [
    asset(1, "LATE", { next_cal_date: "2026-11-01", due_status: "overdue", last_cal_date: "2025-11-01" }),
    asset(2, "SOON", { next_cal_date: "2026-12-10", due_status: "due_soon" }),
    asset(3, "LATER", { next_cal_date: "2027-03-01", due_status: "current" }),
    asset(4, "UNTRACKED", { next_cal_date: "2026-11-01", due_status: "not_tracked" }),
    asset(5, "GONE", { next_cal_date: "2026-11-01", due_status: "overdue", is_archived: true }),
    asset(6, "ELSEWHERE", { next_cal_date: "2026-11-02", due_status: "overdue", location_id: 2 }),
  ];

  it("lists overdue and soon-due tracked items, soonest first, with days left", () => {
    const r = dueReport(assets, { withinDays: 30, locationId: null, departmentId: null, ownerId: null }, today);
    expect(r.rows.map((row) => row[0])).toEqual(["LATE", "ELSEWHERE", "SOON"]);
    expect(r.rows[0][9]).toBe(-19);
    expect(r.rows[2][9]).toBe(20);
    expect(r.rows[0][2]).toBe("Fluke 87V");
    expect(r.rowLinks).toEqual([1, 6, 2]);
  });

  it("can show only overdue items and filter by location", () => {
    const r = dueReport(assets, { withinDays: 0, locationId: 1, departmentId: null, ownerId: null }, today, {
      location: "Lab A",
    });
    expect(r.rows.map((row) => row[0])).toEqual(["LATE"]);
    expect(r.subtitle).toBe("Overdue items · as of 2026-11-20 · Lab A");
  });
});

describe("dueReport with extensions", () => {
  it("lists extended items with the overdue ones, even when only overdue is asked for", () => {
    const assets = [
      asset(1, "EXT", { next_cal_date: "2026-12-10", due_status: "extended", calibration_extended: true }),
      asset(2, "LATER", { next_cal_date: "2026-12-11", due_status: "current" }),
    ];
    const r = dueReport(assets, { withinDays: 0, locationId: null, departmentId: null, ownerId: null }, "2026-11-20");
    expect(r.rows.map((row) => row[0])).toEqual(["EXT"]);
  });
});

describe("outOfToleranceReport", () => {
  it("keeps non-passing received conditions in the date range, newest first", () => {
    const cals = [
      cal(1, 1, "2026-02-01", { condition_received: "out_of_tolerance" }),
      cal(2, 1, "2026-05-01", { condition_received: "inoperative", condition_returned: "limited" }),
      cal(3, 1, "2026-06-01"),
      cal(4, 1, "2025-01-01", { condition_received: "out_of_tolerance" }),
      cal(5, 1, "2026-07-01", { condition_received: "out_of_tolerance", is_archived: true }),
    ];
    const r = outOfToleranceReport(cals, [asset(1, "A1")], "2026-01-01", "2026-12-31");
    expect(r.rows.map((row) => row[0])).toEqual(["2026-05-01", "2026-02-01"]);
    expect(r.rows[0].slice(8, 10)).toEqual(["Inoperative", "Limited calibration"]);
    expect(r.rows[0][4]).toBe("Lab A");
    expect(r.rows[0][1]).toBe("Calibration");
  });
});

describe("registerReport and historyReport", () => {
  it("lists items by tag and leaves archived ones out unless asked", () => {
    const assets = [asset(2, "B"), asset(1, "A", { is_archived: true, status_name: "Retired" })];
    expect(registerReport(assets, false).rows.map((r) => r[0])).toEqual(["B"]);
    const all = registerReport(assets, true);
    expect(all.rows.map((r) => r[0])).toEqual(["A", "B"]);
    expect(all.rows[0][10]).toBe("Retired (archived)");
    expect(all.rows[0][11]).toBe("12 months");
  });

  it("shows one item's active calibrations newest first, with cost in cents", () => {
    const a = asset(1, "DMM-1", { serial_number: "123" });
    const r = historyReport(a, [cal(1, 1, "2025-01-01"), cal(2, 1, "2026-01-01"), cal(3, 2, "2026-02-01")]);
    expect(r.title).toBe("Service history: DMM-1");
    expect(r.subtitle).toBe("DMM-1 description · Fluke 87V · S/N 123");
    expect(r.rows.map((row) => row[0])).toEqual(["2026-01-01", "2025-01-01"]);
    expect(r.rows[0][7]).toBe(12_500);
    expect(toSheet(r).columns[7]).toEqual({ header: "Cost", kind: "money", width: null });
  });
});

describe("labels", () => {
  it("takes who calibrated from the latest active record", () => {
    const a = asset(1, "DMM-1", { last_cal_date: "2026-01-01", next_cal_date: "2027-01-01" });
    const [label] = labelData(
      [a],
      [
        cal(1, 1, "2025-01-01", { calibrated_by_name: "Old Lab" }),
        cal(2, 1, "2026-01-01", { technician: "J. Smith" }),
        cal(3, 1, "2026-06-01", { is_archived: true, calibrated_by_name: "Archived Lab" }),
        cal(4, 1, "2026-07-01", { counts_as_calibration: false, calibrated_by_name: "Repair Shop" }),
      ],
    );
    expect(label).toMatchObject({ tag: "DMM-1", calibrated: "2026-01-01", due: "2027-01-01", by: "Acme Labs, J. Smith" });
  });

  it("paginates with empty positions to skip on a part-used first sheet", () => {
    const pages = paginateLabels([1, 2, 3, 4, 5], 4, 2);
    expect(pages).toEqual([
      [null, null, 1, 2],
      [3, 4, 5],
    ]);
    expect(paginateLabels([1], 4, 9)[0]).toEqual([null, null, null, 1]);
  });
});
