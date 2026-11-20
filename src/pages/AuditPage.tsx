import { Button, Card } from "@heroui/react";
import { History } from "lucide-react";
import { useEffect, useState } from "react";
import type { AuditEntity } from "../bindings/AuditEntity";
import type { AuditEntry } from "../bindings/AuditEntry";
import { AuditList } from "../components/AuditList";
import { EmptyNote, ErrorAlert, Loading, PageHeader } from "../components/common";
import { SelectInput } from "../components/fields";
import { ApiError, api } from "../lib/api";
import { useAuth } from "../lib/auth";
import { auditEntityLabel } from "../lib/format";
import { useAsync } from "../lib/useAsync";

const PAGE = 100;
const ENTITIES: AuditEntity[] = ["asset", "calibration", "lookup", "user", "session", "settings"];

export function AuditPage() {
  const { can } = useAuth();
  const [entity, setEntity] = useState<AuditEntity | null>(null);
  const [userId, setUserId] = useState<number | null>(null);
  const [entries, setEntries] = useState<AuditEntry[]>([]);
  const [hasMore, setHasMore] = useState(false);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<ApiError>();
  const users = useAsync(() => api.listUsers(), []);

  async function load(beforeId: number | null) {
    setLoading(true);
    try {
      const page = await api.listAudit({ entity, user_id: userId, before_id: beforeId, limit: PAGE + 1 });
      setHasMore(page.length > PAGE);
      const rows = page.slice(0, PAGE);
      setEntries((prev) => (beforeId == null ? rows : [...prev, ...rows]));
      setError(undefined);
    } catch (e) {
      setError(e instanceof ApiError ? e : new ApiError("database", String(e)));
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    if (can("admin")) void load(null);
    // eslint-disable-next-line react-hooks/exhaustive-deps -- reload when the filters change
  }, [entity, userId]);

  if (!can("admin")) {
    return <ErrorAlert error={{ kind: "forbidden", message: "Only administrators can view the full audit log." }} />;
  }

  return (
    <>
      <PageHeader
        title="Audit Log"
        description="Every change, sign-in, and settings update, newest first. Entries can't be edited or deleted."
      />
      <div className="mb-4 grid max-w-2xl grid-cols-2 gap-4">
        <SelectInput<AuditEntity>
          label="Subject"
          value={entity}
          onChange={setEntity}
          options={ENTITIES.map((e) => ({ id: e, label: auditEntityLabel[e] }))}
          placeholder="Everything"
        />
        <SelectInput<number>
          label="User"
          value={userId}
          onChange={setUserId}
          options={(users.data ?? []).map((u) => ({ id: u.id, label: `${u.display_name} (${u.username})` }))}
          placeholder="Anyone"
        />
      </div>

      {error && <ErrorAlert error={error} onRetry={() => void load(null)} />}
      <Card>
        <Card.Content>
          {loading && entries.length === 0 ? (
            <Loading />
          ) : entries.length === 0 ? (
            <EmptyNote icon={<History className="size-5" />} title="No matching entries" />
          ) : (
            <AuditList entries={entries} />
          )}
          {hasMore && (
            <Button
              variant="ghost"
              size="sm"
              className="mt-4"
              isPending={loading}
              onPress={() => void load(entries[entries.length - 1].id)}
            >
              Load older entries
            </Button>
          )}
        </Card.Content>
      </Card>
    </>
  );
}
