// The calibration certificate itself, laid out at its real paper size. The same element is
// shown (scaled) as the preview and printed to the PDF, so the two always match.

import { type CSSProperties, type ReactNode, useLayoutEffect, useRef, useState } from "react";
import type { CertificateDetails } from "../bindings/CertificateDetails";
import type { CertificateTemplate } from "../bindings/CertificateTemplate";
import { temperatureSymbol, withApprover } from "../lib/certificates";
import { conditionReceivedMeta, conditionReturnedMeta, formatDate } from "../lib/format";
import { usePageStyle } from "./ReportView";

const PAPER = { letter: { w: 8.5, h: 11 }, a4: { w: 8.268, h: 11.693 } } as const;
const DPI = 96;

const FONTS = {
  sans: '"Segoe UI", "Helvetica Neue", Arial, sans-serif',
  serif: 'Cambria, Georgia, "Times New Roman", serif',
} as const;

const LOGO_HEIGHT = { small: "0.5in", medium: "0.8in", large: "1.1in" } as const;

const BORDER: Record<CertificateTemplate["border"], (color: string) => CSSProperties> = {
  none: () => ({}),
  single: (c) => ({ border: `1.5px solid ${c}` }),
  double: (c) => ({ border: `5px double ${c}` }),
  thick: (c) => ({ border: `4px solid ${c}` }),
};

const dash = (s: string | null | undefined) => (s && s.trim() ? s : "—");

function Section({ title, accent, children }: { title: string; accent: string; children: ReactNode }) {
  return (
    <section style={{ marginTop: "0.16in" }}>
      <h2
        style={{
          margin: 0,
          paddingBottom: "0.03in",
          borderBottom: `1px solid ${accent}`,
          color: accent,
          fontSize: "8.5pt",
          fontWeight: 700,
          letterSpacing: "0.08em",
          textTransform: "uppercase",
        }}
      >
        {title}
      </h2>
      <div style={{ paddingTop: "0.06in" }}>{children}</div>
    </section>
  );
}

/** Label/value pairs in two columns. */
function Pairs({ items }: { items: [string, ReactNode][] }) {
  return (
    <dl
      style={{
        display: "grid",
        gridTemplateColumns: "1.25in 1fr 1.25in 1fr",
        columnGap: "0.12in",
        rowGap: "0.05in",
        margin: 0,
      }}
    >
      {items.map(([label, value]) => (
        <div key={label} style={{ display: "contents" }}>
          <dt style={{ color: "#555", fontSize: "8.5pt" }}>{label}</dt>
          <dd style={{ margin: 0, fontWeight: 600, whiteSpace: "pre-line", overflowWrap: "anywhere" }}>{value}</dd>
        </div>
      ))}
    </dl>
  );
}

function Signature({
  label,
  name,
  date,
  signature,
}: {
  label: string;
  name: string;
  date: string;
  signature: string | null;
}) {
  return (
    <div style={{ flex: 1 }}>
      {/* The signature sits on the line, like a hand-signed one. Scans often have a white
          background rather than a transparent one, so the white is blended away and the line
          is drawn on top of the image rather than underneath it. */}
      <div
        style={{
          position: "relative",
          height: "0.55in",
          display: "flex",
          alignItems: "flex-end",
          paddingLeft: "0.1in",
        }}
      >
        {signature && (
          <img
            src={signature}
            alt=""
            style={{
              maxHeight: "0.6in",
              maxWidth: "2.4in",
              objectFit: "contain",
              marginBottom: "-0.06in",
              mixBlendMode: "multiply",
            }}
          />
        )}
        <div
          aria-hidden
          style={{ position: "absolute", left: 0, right: 0, bottom: 0, borderBottom: "1px solid #222" }}
        />
      </div>
      <div style={{ display: "flex", justifyContent: "space-between", marginTop: "0.04in", fontSize: "8.5pt" }}>
        <span style={{ color: "#555" }}>{label}</span>
        <span style={{ color: "#555" }}>Date: {date}</span>
      </div>
      <div style={{ fontWeight: 600 }}>{name || " "}</div>
    </div>
  );
}

/** One certificate page at real size (inches); `number` is shown as given (or a placeholder). */
export function CertificateView({
  details,
  template: t,
  number,
}: {
  details: CertificateDetails;
  template: CertificateTemplate;
  number: string;
}) {
  const d = withApprover(details);
  const paper = PAPER[t.paper];
  const accent = t.accent_color;
  const internal = d.kind === "internal";
  const unit = temperatureSymbol[t.temperature_unit];
  const centered = t.header_layout === "logo_center";

  const org = (
    <div style={{ textAlign: centered ? "center" : "right", fontSize: "8.5pt", lineHeight: 1.35 }}>
      {t.organization_name && <div style={{ fontSize: "13pt", fontWeight: 700 }}>{t.organization_name}</div>}
      {t.organization_address && <div style={{ whiteSpace: "pre-line" }}>{t.organization_address}</div>}
      {t.organization_contact && <div style={{ color: "#555" }}>{t.organization_contact}</div>}
    </div>
  );
  const logo = t.logo ? (
    <img src={t.logo} alt="" style={{ height: LOGO_HEIGHT[t.logo_size], maxWidth: "3in", objectFit: "contain" }} />
  ) : null;

  return (
    <div
      className="certificate-page"
      style={{
        position: "relative",
        width: `${paper.w}in`,
        minHeight: `${paper.h}in`,
        boxSizing: "border-box",
        padding: "0.6in 0.65in 0.9in",
        background: "#fff",
        color: "#111",
        fontFamily: FONTS[t.font],
        fontSize: "9.5pt",
        lineHeight: 1.3,
      }}
    >
      <div style={{ position: "absolute", inset: "0.32in", pointerEvents: "none", ...BORDER[t.border](accent) }} />

      <header
        style={
          centered
            ? { display: "flex", flexDirection: "column", alignItems: "center", gap: "0.08in" }
            : { display: "flex", alignItems: "center", justifyContent: "space-between", gap: "0.3in" }
        }
      >
        {logo ?? (centered ? null : <div />)}
        {org}
      </header>

      <h1
        style={{
          margin: "0.22in 0 0",
          paddingBottom: "0.08in",
          borderBottom: `2px solid ${accent}`,
          color: accent,
          fontSize: "21pt",
          fontWeight: 700,
          textAlign: "center",
          letterSpacing: "0.02em",
        }}
      >
        {t.title}
      </h1>

      <div
        style={{
          display: "grid",
          gridTemplateColumns: "repeat(3, 1fr)",
          marginTop: "0.14in",
          border: `1px solid ${accent}`,
          textAlign: "center",
        }}
      >
        {(
          [
            ["Certificate No.", number],
            ["Date of calibration", formatDate(d.cal_date)],
            ["Next calibration due", formatDate(d.next_cal_date)],
          ] as const
        ).map(([label, value], i) => (
          <div key={label} style={{ padding: "0.06in", borderLeft: i ? `1px solid ${accent}` : undefined }}>
            <div style={{ color: "#555", fontSize: "8pt" }}>{label}</div>
            <div style={{ fontWeight: 700, fontSize: "11pt" }}>{value}</div>
          </div>
        ))}
      </div>

      {!internal && (
        <Section title="Customer" accent={accent}>
          <div style={{ fontWeight: 600 }}>{dash(d.customer_name)}</div>
          {d.customer_address && <div style={{ whiteSpace: "pre-line" }}>{d.customer_address}</div>}
        </Section>
      )}

      <Section title="Instrument" accent={accent}>
        <Pairs
          items={[
            ...(internal ? ([["Asset number", dash(d.asset_tag)]] as [string, ReactNode][]) : []),
            ["Description", dash(d.description)],
            ["Manufacturer", dash(d.manufacturer)],
            ["Model number", dash(d.model_number)],
            ["Serial number", dash(d.serial_number)],
            ["Procedure", dash(d.procedure)],
          ]}
        />
      </Section>

      <Section title="Calibration" accent={accent}>
        <Pairs
          items={[
            ["Temperature", d.temperature ? `${d.temperature} ${unit}` : "—"],
            ["Humidity", d.humidity ? `${d.humidity} % RH` : "—"],
            ["As received", d.condition_received ? conditionReceivedMeta[d.condition_received].label : "—"],
            ["As returned", d.condition_returned ? conditionReturnedMeta[d.condition_returned].label : "—"],
          ]}
        />
      </Section>

      <Section title="Equipment used" accent={accent}>
        {d.equipment.length === 0 ? (
          <div style={{ color: "#555" }}>—</div>
        ) : (
          <table style={{ width: "100%", borderCollapse: "collapse", fontSize: "8.5pt" }}>
            <thead>
              <tr style={{ color: accent, textAlign: "left" }}>
                {["Asset no.", "Description", "Model", "Serial", "Calibration Valid Until"].map((h) => (
                  <th key={h} style={{ padding: "0.03in 0.05in", borderBottom: "1px solid #bbb", fontWeight: 700 }}>
                    {h}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {d.equipment.map((e, i) => (
                <tr key={i}>
                  {[e.asset_tag, e.description, e.model_number, e.serial_number, formatDate(e.cal_due)].map((v, j) => (
                    <td key={j} style={{ padding: "0.03in 0.05in", borderBottom: "1px solid #e3e3e3" }}>
                      {dash(v)}
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </Section>

      {t.statement && (
        <p style={{ margin: "0.2in 0 0", fontSize: "8.5pt", lineHeight: 1.4, whiteSpace: "pre-line" }}>
          {t.statement}
        </p>
      )}

      <div style={{ display: "flex", gap: "0.5in", marginTop: "0.25in" }}>
        <Signature
          label="Calibrated by"
          name={d.calibrated_by}
          date={formatDate(d.cal_date)}
          signature={d.calibrated_by_signature}
        />
        <Signature
          label="Approved by"
          name={d.approved_by}
          date={d.approved_by.trim() === d.calibrated_by.trim() ? formatDate(d.cal_date) : ""}
          signature={d.approved_by_signature}
        />
      </div>

      <footer
        style={{
          position: "absolute",
          left: "0.65in",
          right: "0.65in",
          bottom: "0.5in",
          display: "flex",
          justifyContent: "space-between",
          gap: "0.3in",
          color: "#555",
          fontSize: "7.5pt",
        }}
      >
        <span style={{ whiteSpace: "pre-line" }}>{t.footer}</span>
        <span style={{ whiteSpace: "nowrap" }}>{number}</span>
      </footer>
    </div>
  );
}

/**
 * The certificate scaled to fit its column on screen, and printed at full size alone on
 * the page (everything else on the page should be `print:hidden`).
 */
export function CertificatePreview(props: Parameters<typeof CertificateView>[0]) {
  const paper = PAPER[props.template.paper];
  usePageStyle(`@page { size: ${paper.w}in ${paper.h}in; margin: 0; }`);
  const box = useRef<HTMLDivElement>(null);
  const [zoom, setZoom] = useState(0.6);
  useLayoutEffect(() => {
    const el = box.current;
    if (!el) return;
    const fit = () => setZoom(Math.min(1, el.clientWidth / (paper.w * DPI)));
    fit();
    const observer = new ResizeObserver(fit);
    observer.observe(el);
    return () => observer.disconnect();
  }, [paper.w]);

  return (
    <div ref={box} className="w-full">
      <div
        className="origin-top-left shadow-lg ring-1 ring-black/10 [zoom:var(--certificate-zoom)] print:shadow-none print:ring-0 print:[zoom:1]"
        style={{ width: `${paper.w}in`, ["--certificate-zoom" as string]: zoom }}
      >
        <CertificateView {...props} />
      </div>
    </div>
  );
}
