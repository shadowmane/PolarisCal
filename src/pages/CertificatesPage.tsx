// Issued certificates: the log, and one certificate as it was issued (to save it again).

import { Button, SearchField, Table, toast } from "@heroui/react";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { ArrowLeft, Copy, FileBadge, FileDown, FilePlus2, Paperclip, X } from "lucide-react";
import { useEffect, useState } from "react";
import { Link, useNavigate, useParams } from "react-router";
import { CertificatePreview } from "../components/CertificateView";
import { EmptyNote, ErrorAlert, Loading, PageHeader } from "../components/common";
import { ApiError, api } from "../lib/api";
import { useAuth } from "../lib/auth";
import { formatDate, formatDateTime } from "../lib/format";
import { useAsync } from "../lib/useAsync";

export function CertificatesPage() {
  const navigate = useNavigate();
  const { can } = useAuth();
  const [search, setSearch] = useState("");
  const [query, setQuery] = useState("");
  useEffect(() => {
    const t = setTimeout(() => setQuery(search.trim()), 250);
    return () => clearTimeout(t);
  }, [search]);
  const { data, error, loading, reload } = useAsync(() => api.listCertificates({ search: query || null }), [query]);

  return (
    <>
      <PageHeader
        title="Issued Certificates"
        description="Every certificate generated in PolarisCal. Open one to save it as a PDF again."
        actions={
          can("edit_records") && (
            <Button onPress={() => navigate("/certificates/new")}>
              <FilePlus2 className="size-4" /> Generate Certificate
            </Button>
          )
        }
      />
      <div className="mb-4 flex justify-end">
        <SearchField value={search} onChange={setSearch} aria-label="Search" className="w-80">
          <SearchField.Group>
            <SearchField.SearchIcon />
            <SearchField.Input placeholder="Search number, asset, customer, description…" />
            <SearchField.ClearButton />
          </SearchField.Group>
        </SearchField>
      </div>
      {error && <ErrorAlert error={error} onRetry={reload} />}
      {loading && !data && <Loading />}
      {data && (
        <Table>
          <Table.ScrollContainer>
            <Table.Content aria-label="Issued certificates" onRowAction={(key) => navigate(`/certificates/${key}`)}>
              <Table.Header>
                <Table.Column isRowHeader>Certificate No.</Table.Column>
                <Table.Column>Type</Table.Column>
                <Table.Column>Asset / customer</Table.Column>
                <Table.Column>Description</Table.Column>
                <Table.Column>Calibrated</Table.Column>
                <Table.Column>Issued</Table.Column>
                <Table.Column>Issued by</Table.Column>
              </Table.Header>
              <Table.Body
                items={data}
                renderEmptyState={() => (
                  <EmptyNote icon={<FileBadge className="size-5" />} title={query ? "No matches" : "No certificates yet"}>
                    {query ? `Nothing matches “${query}”.` : "Certificates you generate are listed here."}
                  </EmptyNote>
                )}
              >
                {(c) => (
                  <Table.Row id={c.id} className="cursor-pointer">
                    <Table.Cell className="font-medium">{c.certificate_number}</Table.Cell>
                    <Table.Cell>{c.kind === "internal" ? "Internal" : "External"}</Table.Cell>
                    <Table.Cell>{c.subject}</Table.Cell>
                    <Table.Cell>{c.description}</Table.Cell>
                    <Table.Cell>{formatDate(c.cal_date)}</Table.Cell>
                    <Table.Cell>{formatDateTime(c.created_at)}</Table.Cell>
                    <Table.Cell>{c.issued_by}</Table.Cell>
                  </Table.Row>
                )}
              </Table.Body>
            </Table.Content>
          </Table.ScrollContainer>
        </Table>
      )}
    </>
  );
}

export function CertificatePage() {
  const { id } = useParams();
  const navigate = useNavigate();
  const { can } = useAuth();
  const { data: c, error, loading, reload } = useAsync(() => api.getCertificate(Number(id)), [id]);
  const [busy, setBusy] = useState(false);
  // Test data isn't kept with the certificate; attach the file again to include it in a copy.
  const [testData, setTestData] = useState<string | null>(null);

  if (error) return <ErrorAlert error={error} onRetry={reload} />;
  if (loading && !c) return <Loading />;
  if (!c) return null;
  const number = c.certificate_number;

  async function savePdf() {
    if (!c) return;
    const path = await saveDialog({
      title: "Save certificate as PDF",
      defaultPath: `Certificate ${number}.pdf`,
      filters: [{ name: "PDF document", extensions: ["pdf"] }],
    });
    if (!path) return;
    setBusy(true);
    try {
      await api.saveCertificatePdf(path, c.template.paper, testData);
      toast.success(`Saved ${path.split(/[\\/]/).pop()}`);
    } catch (e) {
      toast.danger(e instanceof ApiError ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="flex flex-col gap-4 print:block">
      <div className="print:hidden">
        <Link
          to="/certificates"
          className="mb-3 inline-flex items-center gap-1.5 text-sm text-muted hover:text-foreground"
        >
          <ArrowLeft className="size-4" /> Issued certificates
        </Link>
        <PageHeader
          title={`Certificate ${number}`}
          description={`${c.kind === "internal" ? "Internal" : "External"} · ${c.subject} · issued ${formatDateTime(c.created_at)} by ${c.issued_by}`}
          actions={
            <>
              {can("edit_records") && (
                <Button
                  variant="secondary"
                  onPress={() => navigate("/certificates/new", { state: { details: c.details } })}
                >
                  <Copy className="size-4" /> Use as starting point
                </Button>
              )}
              <Button isPending={busy} onPress={() => void savePdf()}>
                <FileDown className="size-4" /> Save PDF…
              </Button>
            </>
          }
        />
        {c.details.asset_id != null && (
          <p className="-mt-3 mb-2 text-sm">
            <Link to={`/assets/${c.details.asset_id}`} className="text-accent hover:underline">
              Open {c.details.asset_tag}
            </Link>
          </p>
        )}
      </div>
      <div className="flex flex-wrap items-center gap-3 text-sm print:hidden">
        <span className="text-muted">
          {c.details.test_data_file
            ? `Issued with test data: ${c.details.test_data_file}. To include it in a new copy, attach it again.`
            : "Test data (optional):"}
        </span>
        {testData && (
          <span className="flex items-center gap-1.5">
            <Paperclip className="size-4 text-muted" />
            {testData.split(/[\/]/).pop()}
          </span>
        )}
        <Button
          variant="secondary"
          size="sm"
          onPress={async () => {
            const path = await openDialog({
              title: "Choose the test data",
              multiple: false,
              filters: [{ name: "PDF document", extensions: ["pdf"] }],
            });
            if (typeof path === "string") setTestData(path);
          }}
        >
          <Paperclip className="size-4" /> {testData ? "Change…" : "Attach PDF…"}
        </Button>
        {testData && (
          <Button variant="tertiary" size="sm" onPress={() => setTestData(null)}>
            <X className="size-4" /> Remove
          </Button>
        )}
      </div>
      <div className="max-w-4xl print:max-w-none">
        <CertificatePreview details={c.details} template={c.template} number={number} />
      </div>
    </div>
  );
}
