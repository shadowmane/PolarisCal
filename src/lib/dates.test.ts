import { afterEach, describe, expect, it } from "vitest";
import { DATE_FORMATS, dateFormat, dateInputLocale, formatYmd, setDateFormat } from "./dates";
import { formatDate } from "./format";

afterEach(() => setDateFormat("system"));

describe("date formats", () => {
  it("writes each standard format", () => {
    const out = Object.fromEntries(
      DATE_FORMATS.filter((f) => f.id !== "system").map((f) => [f.id, formatYmd(2026, 3, 9, f.id)]),
    );
    expect(out).toEqual({
      mdy_numeric: "03/09/2026",
      mdy_medium: "Mar 9, 2026",
      mdy_long: "March 9, 2026",
      iso: "2026-03-09",
      dmy_slash: "09/03/2026",
      dmy_dot: "09.03.2026",
      dmy_dash: "09-03-2026",
      dmy_medium: "9 Mar 2026",
      dmy_long: "9 March 2026",
      dmy_mil: "09-Mar-2026",
      ymd_slash: "2026/03/09",
    });
  });

  it("formats stored dates in the chosen format", () => {
    setDateFormat("dmy_dot");
    expect(formatDate("2026-11-22")).toBe("22.11.2026");
    expect(formatDate(null)).toBe("—");
  });

  it("falls back to the Windows setting for unknown formats", () => {
    setDateFormat("dmy_dot");
    setDateFormat("not-a-format");
    expect(dateFormat()).toBe("system");
    expect(dateInputLocale()).toBeUndefined();
  });
});
