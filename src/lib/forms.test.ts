import { describe, expect, it } from "vitest";
import type { Asset } from "../bindings/Asset";
import type { LookupItem } from "../bindings/LookupItem";
import { ApiError } from "./api";
import {
  assetToForm,
  centsToDollars,
  dollarsToCents,
  emptyAssetForm,
  emptyCalibrationForm,
  emptyLookupInput,
  fieldErrors,
  formToAssetInput,
  formToCalibrationInput,
  fromDateValue,
  optionalText,
  pickable,
  toDateValue,
} from "./forms";

describe("dates", () => {
  it("round-trips ISO dates through the picker value", () => {
    const v = toDateValue("2027-02-28");
    expect(v?.year).toBe(2027);
    expect(v?.month).toBe(2);
    expect(fromDateValue(v)).toBe("2027-02-28");
  });

  it("treats empty and malformed dates as no value", () => {
    expect(toDateValue(null)).toBeNull();
    expect(toDateValue("")).toBeNull();
    expect(toDateValue("not-a-date")).toBeNull();
    expect(fromDateValue(null)).toBeNull();
  });
});

describe("money", () => {
  it("converts dollars to whole cents without float drift", () => {
    expect(dollarsToCents(125)).toBe(12500);
    expect(dollarsToCents(0.1 + 0.2)).toBe(30);
    expect(dollarsToCents(19.995)).toBe(2000);
    expect(dollarsToCents(null)).toBeNull();
    expect(dollarsToCents(Number.NaN)).toBeNull();
    expect(centsToDollars(12550)).toBe(125.5);
    expect(centsToDollars(null)).toBeNull();
  });
});

describe("optionalText", () => {
  it("trims and nulls blanks", () => {
    expect(optionalText("  x ")).toBe("x");
    expect(optionalText("   ")).toBeNull();
  });
});

describe("fieldErrors", () => {
  it("maps validation errors by field, keeping the first message", () => {
    const err = new ApiError("validation", "bad", [
      { field: "asset_tag", message: "Tag is required." },
      { field: "asset_tag", message: "second" },
      { field: "", message: "general" },
    ]);
    expect(fieldErrors(err)).toEqual({ asset_tag: "Tag is required." });
    expect(fieldErrors(new Error("x"))).toEqual({});
  });
});

describe("pickable", () => {
  const item = (id: number, is_active: boolean): LookupItem => ({
    id,
    kind: "location",
    name: `L${id}`,
    description: null,
    is_active,
    sort_order: 0,
    is_internal: null,
    tracks_due: null,
  });

  it("offers active entries plus the current value even if inactive", () => {
    const items = [item(1, true), item(2, false), item(3, false)];
    expect(pickable(items, null).map((i) => i.id)).toEqual([1]);
    expect(pickable(items, 2).map((i) => i.id)).toEqual([1, 2]);
    expect(pickable(undefined, 2)).toEqual([]);
  });
});

describe("asset form", () => {
  const valid = () =>
    emptyAssetForm({ asset_tag: " DMM-1 ", description: " Multimeter ", status_id: 1, serial_number: "  " });

  it("converts to API input, trimming text and blanking empty optionals", () => {
    const r = formToAssetInput(valid());
    expect(r.ok).toBe(true);
    if (!r.ok) return;
    expect(r.value.asset_tag).toBe("DMM-1");
    expect(r.value.description).toBe("Multimeter");
    expect(r.value.serial_number).toBeNull();
    expect(r.value.cal_interval).toEqual({ value: 12, unit: "months", end_of_month: false });
  });

  it("reports missing required fields", () => {
    const r = formToAssetInput(emptyAssetForm());
    expect(r.ok).toBe(false);
    if (r.ok) return;
    expect(Object.keys(r.errors).sort()).toEqual(["asset_tag", "description", "status_id"]);
  });

  it("drops the interval when empty and rejects non-whole numbers", () => {
    const none = formToAssetInput({ ...valid(), interval_value: null });
    expect(none.ok && none.value.cal_interval).toBeNull();
    const fractional = formToAssetInput({ ...valid(), interval_value: 1.5 });
    expect(fractional.ok).toBe(false);
  });

  it("never sends a parent for a System", () => {
    const r = formToAssetInput({ ...valid(), kind: "system", parent_id: 7 });
    expect(r.ok && r.value.parent_id).toBeNull();
  });

  it("loads an existing asset back into form state", () => {
    const asset = {
      ...formToAssetInputValue(),
      id: 1,
      parent_tag: null,
      manufacturer_name: null,
      location_name: null,
      department_name: null,
      owner_name: null,
      status_name: "Active",
      last_cal_date: "2026-11-18",
      last_cal_date_entered: "2026-03-15",
      next_cal_date: null,
      due_status: "no_schedule",
      child_count: 0,
      is_archived: false,
      row_version: 1,
      created_at: "",
      updated_at: "",
    } as Asset;
    const form = assetToForm(asset);
    expect(form.serial_number).toBe("");
    expect(form.interval_value).toBe(6);
    expect(form.interval_unit).toBe("days");
    expect(form.last_cal_date).toBe("2026-03-15"); // the entered date, not the derived one
  });
});

function formToAssetInputValue() {
  const r = formToAssetInput(
    emptyAssetForm({
      asset_tag: "A",
      description: "B",
      status_id: 1,
      interval_value: 6,
      interval_unit: "days",
    }),
  );
  if (!r.ok) throw new Error("fixture invalid");
  return r.value;
}

describe("calibration form", () => {
  it("requires item, date, provider, and both conditions", () => {
    const r = formToCalibrationInput(emptyCalibrationForm());
    expect(r.ok).toBe(false);
    if (r.ok) return;
    expect(Object.keys(r.errors).sort()).toEqual([
      "asset_id",
      "cal_date",
      "condition_received",
      "condition_returned",
      "provider_id",
    ]);
  });

  it("converts cost to cents and blanks optional text", () => {
    const r = formToCalibrationInput(
      emptyCalibrationForm({
        asset_id: 1,
        cal_date: "2026-06-01",
        provider_id: 2,
        condition_received: "out_of_tolerance",
        condition_returned: "pass",
        cost: 149.99,
        technician: " ",
      }),
    );
    expect(r.ok).toBe(true);
    if (!r.ok) return;
    expect(r.value.cost_cents).toBe(14999);
    expect(r.value.technician).toBeNull();
  });
});

describe("lookup input", () => {
  it("defaults kind-specific flags", () => {
    expect(emptyLookupInput("provider").is_internal).toBe(false);
    expect(emptyLookupInput("status").tracks_due).toBe(true);
    expect(emptyLookupInput("location").is_internal).toBeNull();
  });
});
