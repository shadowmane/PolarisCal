import { Chip } from "@heroui/react";
import { ArrowRight } from "lucide-react";
import { useState } from "react";
import type { AuditEntry } from "../bindings/AuditEntry";
import { auditActionMeta, auditEntityLabel, EMPTY, formatDateTime } from "../lib/format";

function ChangeTable({ entry }: { entry: AuditEntry }) {
  const [expanded, setExpanded] = useState(entry.changes.length <= 6);
  const shown = expanded ? entry.changes : entry.changes.slice(0, 5);
  if (entry.changes.length === 0) return null;
  return (
    <div className="mt-2 overflow-hidden rounded-lg border border-separator">
      <table className="w-full text-xs">
        <tbody>
          {shown.map((c) => (
            <tr key={c.field} className="border-b border-separator last:border-b-0">
              <td className="w-48 bg-surface-secondary px-3 py-1.5 align-top font-medium text-muted">{c.field}</td>
              <td className="px-3 py-1.5 align-top">
                {entry.action === "create" ? (
                  <span className="break-words">{c.new ?? EMPTY}</span>
                ) : (
                  <span className="flex flex-wrap items-center gap-1.5 break-words">
                    <span className="text-muted line-through decoration-muted/60">{c.old ?? EMPTY}</span>
                    <ArrowRight className="size-3 shrink-0 text-muted" />
                    <span>{c.new ?? EMPTY}</span>
                  </span>
                )}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      {!expanded && (
        <button
          type="button"
          className="w-full bg-surface-secondary px-3 py-1.5 text-left text-xs text-link hover:underline"
          onClick={() => setExpanded(true)}
        >
          Show all {entry.changes.length} fields
        </button>
      )}
    </div>
  );
}

/** A newest-first list of audit entries with their reasons and field changes. */
export function AuditList({ entries, showSubject = true }: { entries: AuditEntry[]; showSubject?: boolean }) {
  return (
    <ol className="flex flex-col">
      {entries.map((e) => {
        const meta = auditActionMeta[e.action];
        return (
          <li key={e.id} className="border-b border-separator py-3 first:pt-0 last:border-b-0 last:pb-0">
            <div className="flex flex-wrap items-center gap-x-2 gap-y-1 text-sm">
              <Chip size="sm" variant="soft" color={meta.color}>
                {meta.label}
              </Chip>
              {showSubject && (
                <span className="font-medium">
                  {auditEntityLabel[e.entity]}
                  {e.entity_label ? `: ${e.entity_label}` : ""}
                </span>
              )}
              {!showSubject && e.entity === "calibration" && (
                <span className="font-medium">Calibration record {e.entity_label ? `(${e.entity_label})` : ""}</span>
              )}
              <span className="text-muted">
                by <span className="text-foreground">{e.username}</span> · {formatDateTime(e.occurred_at)}
              </span>
            </div>
            {e.reason && (
              <p className="mt-1.5 text-sm">
                <span className="text-muted">Reason: </span>
                {e.reason}
              </p>
            )}
            <ChangeTable entry={e} />
          </li>
        );
      })}
    </ol>
  );
}
