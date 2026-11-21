// Shows a report table on screen, prints it, and exports it to Excel.

import { Button, toast } from "@heroui/react";
import { save } from "@tauri-apps/plugin-dialog";
import { FileSpreadsheet, Printer } from "lucide-react";
import { useEffect, useState } from "react";
import { useNavigate } from "react-router";
import type { ExportCell } from "../bindings/ExportCell";
import { ApiError, api } from "../lib/api";
import { currencySymbol, useCurrencySymbol } from "../lib/currency";
import { EMPTY, formatCents, formatDate } from "../lib/format";
import { isoToday, type ReportColumn, type ReportTable, toSheet } from "../lib/reports";

function display(cell: ExportCell, column: ReportColumn): string {
  if (cell == null || cell === "") return EMPTY;
  if (column.kind === "date" && typeof cell === "string") return formatDate(cell);
  if (column.kind === "money" && typeof cell === "number") return formatCents(cell);
  return String(cell);
}

/** Sets the printed page orientation while mounted. */
export function usePageStyle(css: string) {
  useEffect(() => {
    const style = document.createElement("style");
    style.media = "print";
    style.textContent = css;
    document.head.appendChild(style);
    return () => style.remove();
  }, [css]);
}

export function ReportToolbar({ report, fileName }: { report: ReportTable; fileName: string }) {
  const [busy, setBusy] = useState(false);

  async function exportExcel() {
    const path = await save({
      title: "Export to Excel",
      defaultPath: `${fileName} ${isoToday()}.xlsx`,
      filters: [{ name: "Excel workbook", extensions: ["xlsx"] }],
    });
    if (!path) return;
    setBusy(true);
    try {
      await api.exportXlsx(path, [{ ...toSheet(report), currency_symbol: currencySymbol() }]);
      toast.success(`Saved ${path.split(/[\\/]/).pop()}`);
    } catch (e) {
      toast.danger(e instanceof ApiError ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="flex items-center gap-2 print:hidden">
      <Button variant="secondary" isPending={busy} isDisabled={report.rows.length === 0} onPress={() => void exportExcel()}>
        <FileSpreadsheet className="size-4" /> Export to Excel
      </Button>
      <Button variant="secondary" isDisabled={report.rows.length === 0} onPress={() => window.print()}>
        <Printer className="size-4" /> Print or save PDF
      </Button>
    </div>
  );
}

/** The printable report: title, subtitle, and table. Rows link to their item on screen. */
export function ReportBody({ report, emptyText }: { report: ReportTable; emptyText: string }) {
  const navigate = useNavigate();
  useCurrencySymbol(); // re-render money cells if the currency changes
  const wide = report.columns.length > 7;
  usePageStyle(`@page { size: ${wide ? "landscape" : "portrait"}; }`);

  return (
    <section className="print-plain">
      <div className="mb-3 hidden print:block">
        <h1 className="text-lg font-semibold">{report.title}</h1>
        {report.subtitle && <p className="text-xs">{report.subtitle}</p>}
        <p className="text-[10px]">Printed {new Date().toLocaleString()} from PolarisCal</p>
      </div>
      {report.rows.length === 0 ? (
        <p className="rounded-xl border border-dashed border-separator px-4 py-10 text-center text-sm text-muted print:hidden">
          {emptyText}
        </p>
      ) : (
        <div className="overflow-x-auto rounded-xl border border-separator print:overflow-visible print:rounded-none print:border-0">
          <table className="w-full border-collapse text-sm print:text-[9pt]">
            <thead className="bg-surface-secondary print:table-header-group">
              <tr>
                {report.columns.map((c) => (
                  <th
                    key={c.header}
                    className={`border-b border-separator px-3 py-2 text-left font-medium whitespace-nowrap text-muted print:border-black print:px-1.5 print:py-1 ${
                      c.kind === "number" || c.kind === "money" ? "text-right" : ""
                    }`}
                  >
                    {c.header}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {report.rows.map((row, i) => {
                const link = report.rowLinks?.[i];
                return (
                  <tr
                    key={i}
                    className={`border-b border-separator last:border-b-0 print:break-inside-avoid print:border-gray-400 ${
                      link != null ? "cursor-pointer hover:bg-surface-secondary" : ""
                    }`}
                    onClick={link != null ? () => navigate(`/assets/${link}`) : undefined}
                  >
                    {row.map((cell, j) => {
                      const column = report.columns[j];
                      const numeric = column.kind === "number" || column.kind === "money";
                      const negative = column.header === "Days left" && typeof cell === "number" && cell < 0;
                      return (
                        <td
                          key={j}
                          className={`px-3 py-2 align-top print:px-1.5 print:py-0.5 ${numeric ? "text-right tabular-nums" : ""} ${
                            column.kind === "date" ? "whitespace-nowrap" : ""
                          } ${negative ? "font-medium text-danger" : ""} ${j === 0 ? "font-medium" : ""}`}
                        >
                          {display(cell, column)}
                        </td>
                      );
                    })}
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
      <p className="mt-2 text-xs text-muted print:hidden">
        {report.rows.length} row{report.rows.length === 1 ? "" : "s"}
        {report.rowLinks ? " · select a row to open the item" : ""}
      </p>
    </section>
  );
}
