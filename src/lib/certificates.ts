// Calibration certificates: numbering, defaults, and the sample shown while editing the template.

import type { CalInterval } from "../bindings/CalInterval";
import type { CertificateDetails } from "../bindings/CertificateDetails";
import type { CertificateTemplate } from "../bindings/CertificateTemplate";
import type { TemperatureUnit } from "../bindings/TemperatureUnit";
import { isoToday } from "./reports";

/** Mirrors `format_certificate_number` in Rust: {YYYY} {YY} {MM} {DD} and {SEQ}. */
export function formatCertificateNumber(pattern: string, digits: number, seq: number, isoDate: string): string {
  const [y, m, d] = isoDate.split("-");
  const tokens: [string, string][] = [
    ["{YYYY}", y],
    ["{YY}", y.slice(-2)],
    ["{MM}", m],
    ["{DD}", d],
    ["{SEQ}", String(seq).padStart(digits, "0")],
  ];
  return tokens.reduce((out, [token, value]) => out.split(token).join(value), pattern);
}

/** The number the next certificate issued today would get. */
export function nextCertificateNumber(template: CertificateTemplate, today = isoToday()): string {
  return formatCertificateNumber(template.number_pattern, template.number_digits, template.next_number, today);
}

export const temperatureSymbol: Record<TemperatureUnit, string> = { celsius: "°C", fahrenheit: "°F" };

/** Mirrors `add_interval` in Rust (months clamp to the month's last day). */
export function addInterval(isoDate: string, interval: CalInterval): string {
  const [y, m, d] = isoDate.split("-").map(Number);
  let due: Date;
  if (interval.unit === "days") {
    due = new Date(y, m - 1, d + interval.value);
  } else {
    const first = new Date(y, m - 1 + interval.value, 1);
    const last = new Date(first.getFullYear(), first.getMonth() + 1, 0).getDate();
    due = new Date(first.getFullYear(), first.getMonth(), Math.min(d, last));
  }
  if (interval.end_of_month) due = new Date(due.getFullYear(), due.getMonth() + 1, 0);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${due.getFullYear()}-${pad(due.getMonth() + 1)}-${pad(due.getDate())}`;
}

export function emptyCertificateDetails(defaults: Partial<CertificateDetails> = {}): CertificateDetails {
  return {
    kind: "internal",
    asset_id: null,
    customer_name: "",
    customer_address: "",
    asset_tag: "",
    manufacturer: "",
    description: "",
    model_number: "",
    serial_number: "",
    procedure: "",
    temperature: "",
    humidity: "",
    cal_date: isoToday(),
    next_cal_date: null,
    condition_received: "pass",
    condition_returned: "pass",
    equipment: [],
    calibrated_by: "",
    calibrated_by_id: null,
    approved_by: "",
    approved_by_id: null,
    calibrated_by_signature: null,
    approved_by_signature: null,
    test_data_file: "",
    ...defaults,
  };
}

/** What a certificate prints: a blank approver is the person who calibrated it. */
export function withApprover(d: CertificateDetails): CertificateDetails {
  return d.approved_by.trim()
    ? d
    : {
        ...d,
        approved_by: d.calibrated_by,
        approved_by_id: d.calibrated_by_id,
        approved_by_signature: d.calibrated_by_signature,
      };
}

/** Example details for previewing the template in Settings. */
export function sampleCertificateDetails(): CertificateDetails {
  const today = isoToday();
  return emptyCertificateDetails({
    kind: "external",
    customer_name: "Example Customer Ltd.",
    customer_address: "100 Industrial Way\nSpringfield, ST 12345",
    manufacturer: "Fluke",
    description: "Digital multimeter",
    model_number: "87V",
    serial_number: "12345678",
    procedure: "CP-DMM-01 Rev. B",
    temperature: "21.5",
    humidity: "45",
    cal_date: today,
    next_cal_date: addInterval(today, { value: 12, unit: "months", end_of_month: false }),
    equipment: [
      {
        asset_id: null,
        asset_tag: "STD-0012",
        description: "Multifunction calibrator",
        model_number: "5522A",
        serial_number: "9876543",
        cal_due: addInterval(today, { value: 6, unit: "months", end_of_month: false }),
      },
    ],
    calibrated_by: "A. Technician",
    approved_by: "Q. Manager",
  });
}
