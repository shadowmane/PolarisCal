// Generate Certificate: the details on the left, a live preview on the right, and a
// button that issues the certificate (next number, logged) and saves it as a PDF.

import { Button, ComboBox, Input, Label, ListBox, toast, ToggleButton, ToggleButtonGroup } from "@heroui/react";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import {
  Building2,
  FileCheck2,
  GripVertical,
  History,
  Paperclip,
  Plus,
  Signature,
  Trash2,
  Warehouse,
  X,
} from "lucide-react";
import { type PointerEvent, useEffect, useMemo, useRef, useState } from "react";
import { useLocation, useNavigate } from "react-router";
import type { Asset } from "../bindings/Asset";
import type { CertificateDetails } from "../bindings/CertificateDetails";
import type { CertificateEquipment } from "../bindings/CertificateEquipment";
import type { CertificateSigner } from "../bindings/CertificateSigner";
import type { CertificateTemplate } from "../bindings/CertificateTemplate";
import type { ConditionReceived } from "../bindings/ConditionReceived";
import type { ConditionReturned } from "../bindings/ConditionReturned";
import { CertificatePreview } from "../components/CertificateView";
import { ErrorAlert, Loading, PageHeader } from "../components/common";
import { CheckboxInput, DateInput, SelectInput, TextAreaInput, TextInput } from "../components/fields";
import { ApiError, api } from "../lib/api";
import { useAuth } from "../lib/auth";
import {
  addInterval,
  calibrationDatesText,
  emptyCertificateDetails,
  nextCertificateNumber,
  temperatureSymbol,
} from "../lib/certificates";
import { conditionReceivedMeta, conditionReturnedMeta, formatDate } from "../lib/format";
import { type FieldErrors, fieldErrors } from "../lib/forms";
import { lookupOptions, useLookups } from "../lib/lookups";
import { isoToday } from "../lib/reports";
import { useAsync } from "../lib/useAsync";

const RECEIVED: ConditionReceived[] = ["pass", "out_of_tolerance", "inoperative"];
const RETURNED: ConditionReturned[] = ["pass", "limited", "fail"];

/**
 * Closes any open menu and waits until the browser has painted the latest render, so the
 * PDF shows the certificate as it is now.
 */
const painted = () => {
  if (document.activeElement instanceof HTMLElement) document.activeElement.blur();
  return new Promise<void>((r) => requestAnimationFrame(() => requestAnimationFrame(() => r())));
};

function ItemComboBox({
  label,
  assets,
  inputValue,
  onInputChange,
  onPick,
  placeholder,
  error,
  isRequired,
  allowsCustomValue,
}: {
  label: string;
  assets: Asset[];
  inputValue: string;
  onInputChange: (text: string) => void;
  onPick: (asset: Asset) => void;
  placeholder: string;
  error?: string;
  isRequired?: boolean;
  allowsCustomValue?: boolean;
}) {
  return (
    <ComboBox
      variant="secondary"
      inputValue={inputValue}
      onInputChange={onInputChange}
      selectedKey={null}
      onSelectionChange={(k) => {
        const asset = assets.find((a) => a.id === Number(k));
        if (asset) onPick(asset);
      }}
      allowsCustomValue={allowsCustomValue}
      isRequired={isRequired}
      isInvalid={!!error}
      validationBehavior="aria"
      menuTrigger="focus"
    >
      <Label>{label}</Label>
      <ComboBox.InputGroup>
        <Input placeholder={placeholder} />
        <ComboBox.Trigger />
      </ComboBox.InputGroup>
      {error && <p className="text-xs text-danger">{error}</p>}
      <ComboBox.Popover>
        <ListBox>
          {assets.map((a) => (
            <ListBox.Item key={a.id} id={a.id} textValue={`${a.asset_tag} ${a.description}`}>
              <div className="flex flex-col">
                <span className="font-medium">{a.asset_tag}</span>
                <span className="text-xs text-muted">
                  {[a.description, a.manufacturer_name, a.model_number].filter(Boolean).join(" · ")}
                </span>
                <span className="text-xs text-muted">
                  {calibrationDatesText(a.last_cal_date, a.next_cal_date, formatDate)}
                </span>
              </div>
              <ListBox.ItemIndicator />
            </ListBox.Item>
          ))}
        </ListBox>
      </ComboBox.Popover>
    </ComboBox>
  );
}

/** A sign-off name: type anyone, or pick a PolarisCal user to print their saved signature. */
function SignerComboBox({
  label,
  signers,
  name,
  userId,
  onChange,
  placeholder,
  description,
}: {
  label: string;
  signers: CertificateSigner[];
  name: string;
  userId: number | null;
  onChange: (name: string, userId: number | null) => void;
  placeholder?: string;
  description?: string;
}) {
  const signer = signers.find((u) => u.id === userId);
  return (
    <div className="flex flex-col gap-1">
      <ComboBox
        variant="secondary"
        inputValue={name}
        onInputChange={(text) => {
          if (text === name) return;
          // A name that is exactly a user's counts as that user.
          const match = signers.find((u) => u.display_name.toLowerCase() === text.trim().toLowerCase());
          onChange(text, match?.id ?? null);
        }}
        selectedKey={null}
        onSelectionChange={(k) => {
          const u = signers.find((s) => s.id === Number(k));
          if (u) onChange(u.display_name, u.id);
        }}
        allowsCustomValue
        menuTrigger="focus"
      >
        <Label>{label}</Label>
        <ComboBox.InputGroup>
          <Input placeholder={placeholder} />
          <ComboBox.Trigger />
        </ComboBox.InputGroup>
        <ComboBox.Popover>
          <ListBox>
            {signers.map((u) => (
              <ListBox.Item key={u.id} id={u.id} textValue={u.display_name}>
                <div className="flex flex-col">
                  <span>{u.display_name}</span>
                  <span className="text-xs text-muted">{u.has_signature ? "Signature on file" : "No signature saved"}</span>
                </div>
                <ListBox.ItemIndicator />
              </ListBox.Item>
            ))}
          </ListBox>
        </ComboBox.Popover>
      </ComboBox>
      <p className="flex items-center gap-1 text-xs text-muted">
        {signer?.has_signature && <Signature className="size-3.5" />}
        {signer
          ? signer.has_signature
            ? "Their saved signature is printed."
            : "No signature saved for this user (add one on the Users page)."
          : (description ?? "Not a PolarisCal user, so no signature is printed.")}
      </p>
    </div>
  );
}

const fileName = (path: string) => path.split(/[\\/]/).pop() ?? path;

function equipmentFrom(a: Asset): CertificateEquipment {
  return {
    asset_id: a.id,
    asset_tag: a.asset_tag,
    description: a.description,
    model_number: a.model_number ?? "",
    serial_number: a.serial_number ?? "",
    cal_due: a.next_cal_date,
  };
}

const blankEquipment = (): CertificateEquipment => ({
  asset_id: null,
  asset_tag: "",
  description: "",
  model_number: "",
  serial_number: "",
  cal_due: null,
});

export function GenerateCertificatePage() {
  const navigate = useNavigate();
  const location = useLocation();
  const { user, can } = useAuth();
  const template = useAsync(() => api.getCertificateTemplate(), []);
  const assets = useAsync(() => api.listAssets(), []);
  // "Use as a starting point" from an issued certificate passes its details.
  const startFrom = (location.state as { details?: CertificateDetails } | null)?.details;
  const [form, setForm] = useState<CertificateDetails>(() =>
    startFrom
      ? { ...startFrom, cal_date: emptyCertificateDetails().cal_date, next_cal_date: null, test_data_file: "" }
      : emptyCertificateDetails({ calibrated_by: user.display_name, calibrated_by_id: user.id }),
  );
  const signers = useAsync(() => api.listCertificateSigners(), []);
  // Saved signatures by user, fetched as signers are chosen, for the preview.
  const [signatures, setSignatures] = useState<Record<number, string | null>>({});
  const [testData, setTestData] = useState<string | null>(null);
  // Reordering equipment: the row being dragged, and the rows and handles to measure and focus.
  const [dragging, setDragging] = useState<number | null>(null);
  const rowRefs = useRef<(HTMLDivElement | null)[]>([]);
  const gripRefs = useRef<(HTMLButtonElement | null)[]>([]);
  // Internal certificates can add a service record to the item in the same step.
  const lookups = useLookups();
  const [record, setRecord] = useState<{ on: boolean; serviceTypeId: number | null; providerId: number | null }>({
    on: false,
    serviceTypeId: null,
    providerId: null,
  });
  const [errors, setErrors] = useState<FieldErrors>({});
  const [busy, setBusy] = useState(false);
  const [issuedNumber, setIssuedNumber] = useState<string>();
  const [equipmentSearch, setEquipmentSearch] = useState("");

  const items = assets.data ?? [];
  const picked = useMemo(() => items.find((a) => a.id === form.asset_id), [items, form.asset_id]);

  useEffect(() => {
    for (const id of [form.calibrated_by_id, form.approved_by_id]) {
      if (id == null || id in signatures) continue;
      api
        .getUserSignature(id)
        .then((sig) => setSignatures((all) => ({ ...all, [id]: sig })))
        .catch(() => setSignatures((all) => ({ ...all, [id]: null })));
    }
  }, [form.calibrated_by_id, form.approved_by_id, signatures]);

  // Default the service record to a plain calibration by an in-house provider.
  useEffect(() => {
    const lists = lookups.data;
    if (!lists) return;
    setRecord((r) => {
      const types = lists.service_type.filter((t) => t.is_active);
      const providers = lists.provider.filter((p) => p.is_active);
      return {
        ...r,
        serviceTypeId:
          r.serviceTypeId ??
          (types.find((t) => t.counts_as_calibration && !t.is_repair && !t.is_extension) ?? types[0])?.id ??
          null,
        providerId: r.providerId ?? (providers.find((p) => p.is_internal) ?? providers[0])?.id ?? null,
      };
    });
  }, [lookups.data]);

  // With an item picked, the next calibration follows its interval from the calibration date.
  useEffect(() => {
    if (picked?.cal_interval && form.cal_date) {
      const next = addInterval(form.cal_date, picked.cal_interval);
      setForm((f) => (f.next_cal_date === next ? f : { ...f, next_cal_date: next }));
    }
  }, [picked, form.cal_date]);

  if (!can("edit_records")) {
    return <ErrorAlert error={{ kind: "forbidden", message: "Your role can't generate certificates." }} />;
  }
  if (template.error) return <ErrorAlert error={template.error} onRetry={template.reload} />;
  if (!template.data) return <Loading />;
  const tpl: CertificateTemplate = template.data;
  const unit = temperatureSymbol[tpl.temperature_unit];
  const internal = form.kind === "internal";
  const makeRecord = internal && record.on && form.asset_id != null;

  function set<K extends keyof CertificateDetails>(key: K, value: CertificateDetails[K]) {
    setForm((f) => ({ ...f, [key]: value }));
    setErrors(({ [key]: _cleared, ...rest }) => rest);
  }

  function pickItem(a: Asset) {
    setForm((f) => ({
      ...f,
      asset_id: a.id,
      asset_tag: a.asset_tag,
      manufacturer: a.manufacturer_name ?? "",
      description: a.description,
      model_number: a.model_number ?? "",
      serial_number: a.serial_number ?? "",
      next_cal_date: a.cal_interval ? addInterval(f.cal_date, a.cal_interval) : f.next_cal_date,
    }));
    setErrors({});
  }

  function moveEquipment(from: number, to: number) {
    setForm((f) => {
      const list = [...f.equipment];
      const [moved] = list.splice(from, 1);
      list.splice(to, 0, moved);
      return { ...f, equipment: list };
    });
  }

  /** While dragging by the handle, moves the row past any neighbor whose middle the pointer crosses. */
  function dragOver(e: PointerEvent) {
    if (dragging == null) return;
    let target = dragging;
    rowRefs.current.forEach((row, i) => {
      if (!row) return;
      const { top, height } = row.getBoundingClientRect();
      const middle = top + height / 2;
      if (i < dragging && e.clientY < middle) target = Math.min(target, i);
      if (i > dragging && e.clientY > middle) target = Math.max(target, i);
    });
    if (target !== dragging) {
      moveEquipment(dragging, target);
      setDragging(target);
    }
  }

  /** The arrow keys move the focused row's handle up or down, keeping focus on it. */
  function nudge(index: number, by: number) {
    const to = index + by;
    if (to < 0 || to >= form.equipment.length) return;
    moveEquipment(index, to);
    requestAnimationFrame(() => gripRefs.current[to]?.focus());
  }

  function setEquipment(index: number, patch: Partial<CertificateEquipment>) {
    setForm((f) => ({ ...f, equipment: f.equipment.map((e, i) => (i === index ? { ...e, ...patch } : e)) }));
  }

  /** The checks the server makes too, so mistakes show before choosing where to save. */
  function check(): boolean {
    const found: FieldErrors = {};
    if (internal && !form.asset_tag.trim()) found.asset_tag = "Asset number is required.";
    if (!internal && !form.customer_name.trim()) found.customer_name = "Customer name is required.";
    if (!form.description.trim()) found.description = "Product description is required.";
    if (!form.cal_date) found.cal_date = "Date of calibration is required.";
    if (form.next_cal_date && form.cal_date && form.next_cal_date < form.cal_date) {
      found.next_cal_date = "The next calibration can't be before the calibration date.";
    }
    if (makeRecord) {
      if (form.cal_date > isoToday()) found.cal_date = "A service record can't be dated in the future.";
      if (!form.condition_received) found.condition_received = "Needed for the service record.";
      if (!form.condition_returned) found.condition_returned = "Needed for the service record.";
      if (record.serviceTypeId == null) found.record_service_type = "Choose a service type.";
      if (record.providerId == null) found.record_provider = "Choose who performed it.";
    }
    setErrors(found);
    return Object.keys(found).length === 0;
  }

  async function generate() {
    if (!check()) {
      toast.danger("Fill in the highlighted fields first.");
      return;
    }
    const predicted = nextCertificateNumber(tpl);
    const path = await saveDialog({
      title: "Save certificate as PDF",
      defaultPath: `Certificate ${predicted}.pdf`,
      filters: [{ name: "PDF document", extensions: ["pdf"] }],
    });
    if (!path) return;
    setBusy(true);
    try {
      const issued = await api.createCertificate({ ...form, test_data_file: testData ? fileName(testData) : "" });
      const number = issued.certificate_number;
      setIssuedNumber(number);
      // The service record carries the certificate number; the PDF is attached to it below.
      let recordId: number | null = null;
      if (makeRecord && issued.details.asset_id != null) {
        try {
          const created = await api.createCalibration({
            asset_id: issued.details.asset_id,
            cal_date: issued.details.cal_date,
            service_type_id: record.serviceTypeId!,
            calibrated_by: "provider",
            calibrated_by_id: record.providerId,
            extension_days: null,
            technician: issued.details.calibrated_by || null,
            cost_cents: null,
            certificate_number: number,
            condition_received: issued.details.condition_received!,
            condition_returned: issued.details.condition_returned!,
            notes: null,
            new_status_id: null,
          });
          recordId = created.id;
        } catch (e) {
          toast.warning(
            `Certificate ${number} was issued, but the service record wasn't added: ${e instanceof ApiError ? e.message : String(e)}`,
          );
        }
      }
      await painted();
      // If someone else took the predicted number meanwhile, name the file after the real one.
      const file = number !== predicted ? path.split(predicted).join(number) : path;
      await api.saveCertificatePdf(file, tpl.paper, testData);
      let attached = "";
      if (internal && issued.details.asset_id != null) {
        const settings = await api.getSettings().catch(() => null);
        if (settings?.attachments_root) {
          try {
            await api.addAttachments({
              assetId: issued.details.asset_id,
              calibrationId: recordId,
              category: "certificate",
              paths: [file],
              description: `Calibration certificate ${number}`,
            });
            attached = ` and attached to ${issued.details.asset_tag}`;
          } catch (e) {
            toast.warning(`Saved, but not attached to the item: ${e instanceof ApiError ? e.message : String(e)}`);
          }
        }
      }
      const recorded = recordId != null ? `; service record added to ${issued.details.asset_tag}` : "";
      toast.success(`Certificate ${number} saved${attached}${recorded}`);
      navigate(`/certificates/${issued.id}`);
    } catch (e) {
      setIssuedNumber(undefined);
      const fields = fieldErrors(e);
      setErrors(fields);
      toast.danger(e instanceof ApiError ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  const number = issuedNumber ?? nextCertificateNumber(tpl);

  return (
    <div className="flex flex-col print:block">
      <div className="print:hidden">
        <PageHeader
          title="Generate Certificate"
          description="Fill in the details on the left; the certificate on the right updates as you type."
          actions={
            <Button variant="secondary" onPress={() => navigate("/certificates")}>
              <History className="size-4" /> Issued certificates
            </Button>
          }
        />
      </div>

      <div className="grid grid-cols-1 items-start gap-8 xl:grid-cols-[minmax(0,1fr)_minmax(0,1fr)] print:block">
        <div className="flex flex-col gap-5 print:hidden">
          <ToggleButtonGroup
            aria-label="Calibration type"
            selectionMode="single"
            disallowEmptySelection
            selectedKeys={[form.kind]}
            onSelectionChange={(keys) => {
              const [k] = [...keys];
              if (k) set("kind", k as CertificateDetails["kind"]);
            }}
            className="self-start"
          >
            <ToggleButton id="internal">
              <Warehouse className="size-4" /> Internal
            </ToggleButton>
            <ToggleButton id="external">
              <Building2 className="size-4" /> External
            </ToggleButton>
          </ToggleButtonGroup>

          <div className="grid grid-cols-1 gap-4 rounded-2xl bg-surface p-5 md:grid-cols-2">
            {internal ? (
              <div className="md:col-span-2">
                <ItemComboBox
                  label="Asset number"
                  assets={items}
                  inputValue={form.asset_tag}
                  onInputChange={(text) => {
                    setForm((f) =>
                      text === f.asset_tag ? f : { ...f, asset_tag: text, asset_id: picked?.asset_tag === text ? f.asset_id : null },
                    );
                    setErrors(({ asset_tag: _cleared, ...rest }) => rest);
                  }}
                  onPick={pickItem}
                  placeholder={assets.loading ? "Loading…" : "Type or pick an asset number"}
                  error={errors.asset_tag}
                  isRequired
                  allowsCustomValue
                />
                <p className="mt-1 text-xs text-muted">
                  Picking an item fills in its details and next calibration date.
                </p>
              </div>
            ) : (
              <>
                <TextInput
                  label="Customer name"
                  value={form.customer_name}
                  onChange={(v) => set("customer_name", v)}
                  error={errors.customer_name}
                  isRequired
                  className="md:col-span-2"
                />
                <TextAreaInput
                  label="Customer address"
                  value={form.customer_address}
                  onChange={(v) => set("customer_address", v)}
                  rows={3}
                  className="md:col-span-2"
                />
              </>
            )}
            <TextInput label="Manufacturer" value={form.manufacturer} onChange={(v) => set("manufacturer", v)} />
            <TextInput label="Model number" value={form.model_number} onChange={(v) => set("model_number", v)} />
            <TextInput
              label="Product description"
              value={form.description}
              onChange={(v) => set("description", v)}
              error={errors.description}
              isRequired
            />
            <TextInput label="Serial number" value={form.serial_number} onChange={(v) => set("serial_number", v)} />
            <TextInput
              label="Calibration test procedure number"
              value={form.procedure}
              onChange={(v) => set("procedure", v)}
              className="md:col-span-2"
            />
            <TextInput
              label={`Temperature (${unit})`}
              value={form.temperature}
              onChange={(v) => set("temperature", v)}
              placeholder="e.g. 21.5"
            />
            <TextInput
              label="Humidity (% RH)"
              value={form.humidity}
              onChange={(v) => set("humidity", v)}
              placeholder="e.g. 45"
            />
            <DateInput
              label="Date of calibration"
              value={form.cal_date}
              onChange={(v) => set("cal_date", v ?? "")}
              error={errors.cal_date}
              isRequired
            />
            <DateInput
              label="Date of next calibration"
              value={form.next_cal_date}
              onChange={(v) => set("next_cal_date", v)}
              error={errors.next_cal_date}
              description={picked?.cal_interval ? "From the item's calibration interval." : undefined}
            />
            <SelectInput<ConditionReceived>
              label="As received"
              value={form.condition_received}
              onChange={(v) => set("condition_received", v)}
              options={RECEIVED.map((c) => ({ id: c, label: conditionReceivedMeta[c].label }))}
              error={errors.condition_received}
            />
            <SelectInput<ConditionReturned>
              label="As returned"
              value={form.condition_returned}
              onChange={(v) => set("condition_returned", v)}
              options={RETURNED.map((c) => ({ id: c, label: conditionReturnedMeta[c].label }))}
              error={errors.condition_returned}
            />
          </div>

          {internal && (
            <div className="flex flex-col gap-3 rounded-2xl bg-surface p-5">
              <CheckboxInput
                label="Also add a service record for this item"
                description={
                  form.asset_id != null
                    ? `Records this calibration on ${form.asset_tag}, with the certificate number, and attaches the PDF to it.`
                    : "Pick the item from the Asset number list to add a service record to it."
                }
                isSelected={makeRecord}
                onChange={(on) => setRecord((r) => ({ ...r, on }))}
                isDisabled={form.asset_id == null}
              />
              {makeRecord && (
                <div className="grid grid-cols-1 gap-4 pl-8 md:grid-cols-2">
                  <SelectInput<number>
                    label="Service type"
                    value={record.serviceTypeId}
                    onChange={(v) => {
                      setRecord((r) => ({ ...r, serviceTypeId: v }));
                      setErrors(({ record_service_type: _cleared, ...rest }) => rest);
                    }}
                    options={lookupOptions(lookups.data, "service_type", record.serviceTypeId)}
                    error={errors.record_service_type}
                    isRequired
                  />
                  <SelectInput<number>
                    label="Calibration provider"
                    value={record.providerId}
                    onChange={(v) => {
                      setRecord((r) => ({ ...r, providerId: v }));
                      setErrors(({ record_provider: _cleared, ...rest }) => rest);
                    }}
                    options={lookupOptions(lookups.data, "provider", record.providerId)}
                    error={errors.record_provider}
                    isRequired
                  />
                  {form.condition_returned === "fail" && (
                    <p className="text-xs text-warning md:col-span-2">
                      Returned as Fail: the item's status isn't changed here. If it's going for repair or being
                      retired, change its status from the item's page.
                    </p>
                  )}
                </div>
              )}
            </div>
          )}

          <div className="flex flex-col gap-3 rounded-2xl bg-surface p-5">
            <div>
              <h2 className="text-sm font-semibold">Equipment used</h2>
              <p className="text-xs text-muted">The standards this unit was calibrated against.</p>
            </div>
            {form.equipment.length > 1 && (
              <p className="-mt-1 text-xs text-muted">Drag a row by its handle to change the order.</p>
            )}
            {form.equipment.map((e, i) => (
              <div
                key={i}
                ref={(el) => {
                  rowRefs.current[i] = el;
                }}
                className={`flex gap-2 rounded-xl border p-3 transition-shadow ${
                  dragging === i ? "border-accent shadow-lg ring-2 ring-accent/40" : "border-separator"
                }`}
              >
                {form.equipment.length > 1 && (
                  <button
                    type="button"
                    ref={(el) => {
                      gripRefs.current[i] = el;
                    }}
                    aria-label={`Move ${e.asset_tag || e.description || `row ${i + 1}`}: drag, or press the up and down arrow keys`}
                    title="Drag to reorder"
                    className={`-ml-1 flex w-6 shrink-0 touch-none items-center justify-center rounded-md text-muted hover:bg-default hover:text-foreground focus-visible:ring-2 focus-visible:ring-accent focus-visible:outline-none ${
                      dragging === i ? "cursor-grabbing" : "cursor-grab"
                    }`}
                    onPointerDown={(ev) => {
                      ev.preventDefault();
                      ev.currentTarget.setPointerCapture(ev.pointerId);
                      setDragging(i);
                    }}
                    onPointerMove={dragOver}
                    onPointerUp={() => setDragging(null)}
                    onPointerCancel={() => setDragging(null)}
                    onKeyDown={(ev) => {
                      if (ev.key === "ArrowUp" || ev.key === "ArrowDown") {
                        ev.preventDefault();
                        nudge(i, ev.key === "ArrowUp" ? -1 : 1);
                      }
                    }}
                  >
                    <GripVertical className="size-4" />
                  </button>
                )}
                <div className="grid flex-1 grid-cols-2 items-end gap-2 md:grid-cols-[1fr_2fr_1fr_1fr]">
                <TextInput label="Asset no." value={e.asset_tag} onChange={(v) => setEquipment(i, { asset_tag: v })} />
                <TextInput
                  label="Description"
                  value={e.description}
                  onChange={(v) => setEquipment(i, { description: v })}
                />
                <TextInput label="Model" value={e.model_number} onChange={(v) => setEquipment(i, { model_number: v })} />
                <TextInput
                  label="Serial"
                  value={e.serial_number}
                  onChange={(v) => setEquipment(i, { serial_number: v })}
                />
                <DateInput
                  label="Calibration valid until"
                  value={e.cal_due}
                  onChange={(v) => setEquipment(i, { cal_due: v })}
                  className="col-span-2 md:col-span-2"
                />
                <div className="col-span-2 flex justify-end md:col-span-2">
                  <Button
                    variant="tertiary"
                    size="sm"
                    onPress={() => set("equipment", form.equipment.filter((_, j) => j !== i))}
                  >
                    <Trash2 className="size-4" /> Remove
                  </Button>
                </div>
                </div>
              </div>
            ))}
            <div className="grid grid-cols-1 items-end gap-3 md:grid-cols-[1fr_auto]">
              <ItemComboBox
                label="Add from Instruments & Systems"
                assets={items.filter((a) => !form.equipment.some((e) => e.asset_id === a.id))}
                inputValue={equipmentSearch}
                onInputChange={setEquipmentSearch}
                onPick={(a) => {
                  set("equipment", [...form.equipment, equipmentFrom(a)]);
                  setEquipmentSearch("");
                }}
                placeholder="Type an asset number or description"
              />
              <Button variant="secondary" onPress={() => set("equipment", [...form.equipment, blankEquipment()])}>
                <Plus className="size-4" /> Add by hand
              </Button>
            </div>
          </div>

          <div className="flex flex-col gap-3 rounded-2xl bg-surface p-5">
            <div>
              <h2 className="text-sm font-semibold">Test data</h2>
              <p className="text-xs text-muted">
                Optional. A PDF of the measurements, added to the end of the certificate PDF.
              </p>
            </div>
            <div className="flex flex-wrap items-center gap-3">
              {testData ? (
                <span className="flex min-w-0 items-center gap-2 text-sm">
                  <Paperclip className="size-4 shrink-0 text-muted" />
                  <span className="truncate" title={testData}>
                    {fileName(testData)}
                  </span>
                </span>
              ) : (
                <span className="text-sm text-muted">None attached</span>
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
          </div>

          <div className="grid grid-cols-1 gap-4 rounded-2xl bg-surface p-5 md:grid-cols-2">
            <SignerComboBox
              label="Calibrated by"
              signers={signers.data ?? []}
              name={form.calibrated_by}
              userId={form.calibrated_by_id}
              onChange={(name, id) => setForm((f) => ({ ...f, calibrated_by: name, calibrated_by_id: id }))}
            />
            <SignerComboBox
              label="Approved by"
              signers={signers.data ?? []}
              name={form.approved_by}
              userId={form.approved_by_id}
              onChange={(name, id) => setForm((f) => ({ ...f, approved_by: name, approved_by_id: id }))}
              placeholder={form.calibrated_by ? `Same as Calibrated by (${form.calibrated_by})` : undefined}
              description={form.approved_by.trim() ? undefined : "Left blank, the person who calibrated it approves it."}
            />
          </div>
        </div>

        <div className="flex flex-col gap-2 xl:sticky xl:top-0 print:static">
          <p className="text-sm text-muted print:hidden">
            Preview · the number <b>{number}</b> is {issuedNumber ? "assigned" : "the next one, assigned when generated"}
          </p>
          <CertificatePreview
            details={{
              ...form,
              calibrated_by_signature: form.calibrated_by_id != null ? (signatures[form.calibrated_by_id] ?? null) : null,
              approved_by_signature: form.approved_by_id != null ? (signatures[form.approved_by_id] ?? null) : null,
            }}
            template={tpl}
            number={number}
          />
          {testData && (
            <p className="text-sm text-muted print:hidden">
              <Paperclip className="mr-1 inline size-3.5" />
              {fileName(testData)} is added after the certificate in the PDF.
            </p>
          )}
        </div>
      </div>

      <div className="sticky bottom-0 z-10 -mx-8 mt-6 flex items-center justify-end gap-3 border-t border-separator bg-background/90 px-8 py-3 backdrop-blur print:hidden">
        <Button
          variant="tertiary"
          isDisabled={busy}
          onPress={() => {
            setForm(
              emptyCertificateDetails({ kind: form.kind, calibrated_by: user.display_name, calibrated_by_id: user.id }),
            );
            setTestData(null);
            setErrors({});
          }}
        >
          Clear
        </Button>
        <Button isPending={busy} onPress={() => void generate()}>
          <FileCheck2 className="size-4" /> Generate Certificate
        </Button>
      </div>
    </div>
  );
}
