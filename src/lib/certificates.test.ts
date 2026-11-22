import { describe, expect, it } from "vitest";
import { addInterval, formatCertificateNumber } from "./certificates";

describe("certificate numbers", () => {
  it("fill in the same tokens as the Rust side", () => {
    expect(formatCertificateNumber("CAL-{YYYY}-{SEQ}", 4, 7, "2026-03-09")).toBe("CAL-2026-0007");
    expect(formatCertificateNumber("{YY}{MM}{DD}/{SEQ}", 3, 1234, "2026-03-09")).toBe("260309/1234");
    expect(formatCertificateNumber("{SEQ}-{SEQ}", 2, 5, "2026-03-09")).toBe("05-05");
  });
});

describe("next calibration", () => {
  it("adds months, clamping to the month's end, like the schedule", () => {
    expect(addInterval("2026-01-31", { value: 1, unit: "months", end_of_month: false })).toBe("2026-02-28");
    expect(addInterval("2026-11-20", { value: 12, unit: "months", end_of_month: true })).toBe("2027-11-30");
    expect(addInterval("2026-12-25", { value: 10, unit: "days", end_of_month: false })).toBe("2027-01-04");
  });
});
