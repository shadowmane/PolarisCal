import { Alert, AlertDialog, Button, ComboBox, FieldError, Input, Label, ListBox, Modal, toast } from "@heroui/react";
import { today, getLocalTimeZone } from "@internationalized/date";
import { useEffect, useState } from "react";
import type { CalibratedBy } from "../bindings/CalibratedBy";
import type { CalibrationRecord } from "../bindings/CalibrationRecord";
import type { ConditionReceived } from "../bindings/ConditionReceived";
import type { ConditionReturned } from "../bindings/ConditionReturned";
import type { StatusRole } from "../bindings/StatusRole";
import { ApiError, api } from "../lib/api";
import { useAuth } from "../lib/auth";
import { conditionReceivedMeta, conditionReturnedMeta, formatDate } from "../lib/format";
import { addDays } from "../lib/reports";
import {
  type CalibrationForm,
  calibrationToForm,
  emptyCalibrationForm,
  type FieldErrors,
  fieldErrors,
  formToCalibrationInput,
} from "../lib/forms";
import { lookupOptions, useLookups } from "../lib/lookups";
import { LookupSelect } from "./LookupSelect";
import { useAsync } from "../lib/useAsync";
import { AttachmentsPanel, PendingFiles } from "./Attachments";
import { ConfirmDialog } from "./ConfirmDialog";
import { DateInput, MoneyInput, NumberInput, SelectInput, TextAreaInput, TextInput } from "./fields";

/** What saving a record with this outcome asks first. */
type OutcomePrompt =
  | { kind: "calibration_failed" }
  | { kind: "repair_failed" }
  | { kind: "repair_passed"; statusId: number | null; currentStatusId: number };

export interface AssetRef {
  id: number;
  asset_tag: string;
  description: string;
}

const RECEIVED: ConditionReceived[] = ["pass", "out_of_tolerance", "inoperative"];
const RETURNED: ConditionReturned[] = ["pass", "limited", "fail"];

function AssetPicker({
  value,
  onChange,
  error,
}: {
  value: number | null;
  onChange: (id: number | null) => void;
  error?: string;
}) {
  const assets = useAsync(() => api.listAssets(), []);
  return (
    <ComboBox
      variant="secondary"
      selectedKey={value}
      onSelectionChange={(k) => onChange(k == null ? null : Number(k))}
      isRequired
      isInvalid={!!error}
      validationBehavior="aria"
      menuTrigger="focus"
      className="md:col-span-2"
    >
      <Label>Instrument or System</Label>
      <ComboBox.InputGroup>
        <Input placeholder={assets.loading ? "Loading…" : "Type an asset tag or description"} />
        <ComboBox.Trigger />
      </ComboBox.InputGroup>
      <FieldError>{error}</FieldError>
      <ComboBox.Popover>
        <ListBox>
          {(assets.data ?? []).map((a) => (
            <ListBox.Item key={a.id} id={a.id} textValue={`${a.asset_tag} ${a.description}`}>
              <div className="flex flex-col">
                <span className="font-medium">{a.asset_tag}</span>
                <span className="text-xs text-muted">{a.description}</span>
              </div>
              <ListBox.ItemIndicator />
            </ListBox.Item>
          ))}
        </ListBox>
      </ComboBox.Popover>
    </ComboBox>
  );
}

/**
 * Create or edit a service record. Pass `record` to edit; pass `asset` to
 * fix which item a new record belongs to (otherwise the user picks one).
 */
export function CalibrationDialog({
  isOpen,
  onOpenChange,
  record,
  asset,
  onSaved,
}: {
  isOpen: boolean;
  onOpenChange: (open: boolean) => void;
  record?: CalibrationRecord;
  asset?: AssetRef;
  onSaved: (record: CalibrationRecord) => void;
}) {
  const lookups = useLookups();
  const [form, setForm] = useState<CalibrationForm>(emptyCalibrationForm());
  const [errors, setErrors] = useState<FieldErrors>({});
  const [formError, setFormError] = useState<ApiError>();
  const [saving, setSaving] = useState(false);
  const [confirmArchive, setConfirmArchive] = useState(false);
  const [prompt, setPrompt] = useState<OutcomePrompt | null>(null);
  const [reason, setReason] = useState("");
  const [pendingFiles, setPendingFiles] = useState<string[]>([]);
  const settings = useAsync(() => api.getSettings(), [isOpen]);
  const folderSet = !!settings.data?.attachments_root;

  // Reset whenever the dialog opens.
  useEffect(() => {
    if (!isOpen) return;
    setForm(
      record
        ? calibrationToForm(record)
        : emptyCalibrationForm({
            asset_id: asset?.id ?? null,
            cal_date: today(getLocalTimeZone()).toString(),
          }),
    );
    setErrors({});
    setFormError(undefined);
    setReason("");
    setPendingFiles([]);
    setPrompt(null);
  }, [isOpen, record, asset]);

  // A new record starts as the first type that counts as a calibration (normally "Calibration").
  useEffect(() => {
    if (!isOpen || record || form.service_type_id != null || !lookups.data) return;
    const types = lookups.data.service_type.filter((t) => t.is_active);
    const first = types.find((t) => t.counts_as_calibration) ?? types[0];
    if (first) setForm((f) => ({ ...f, service_type_id: first.id }));
  }, [isOpen, record, form.service_type_id, lookups.data]);

  const { user } = useAuth();
  const isExtension =
    lookups.data?.service_type.find((t) => t.id === form.service_type_id)?.is_extension === true;
  const extensionEnds =
    isExtension && form.cal_date && form.extension_days && form.extension_days >= 1
      ? addDays(form.cal_date, form.extension_days)
      : null;

  function set<K extends keyof CalibrationForm>(key: K, value: CalibrationForm[K]) {
    setForm((f) => ({ ...f, [key]: value }));
    const errorKey = key === "cost" ? "cost_cents" : key;
    setErrors(({ [errorKey]: _cleared, ...rest }) => rest);
  }

  function handleError(e: unknown) {
    const err = e instanceof ApiError ? e : new ApiError("database", String(e));
    const fields = fieldErrors(err);
    setErrors(fields);
    if (Object.keys(fields).length === 0 || err.fields.some((f) => !f.field)) setFormError(err);
  }

  const statusWithRole = (role: StatusRole) => lookups.data?.status.find((s) => s.status_role === role);

  /**
   * Saves the record. The outcome can change the item's status, so some outcomes ask first
   * and call back with `decision` (the status to move the item to, or null to leave it):
   * a failed calibration asks repair or retire; a failed repair retires the item; a
   * passed (or limited) repair asks which status it goes back to.
   */
  async function save(decision?: { statusId: number | null }) {
    setFormError(undefined);
    const converted = formToCalibrationInput(form, isExtension);
    if (!converted.ok) {
      setErrors(converted.errors);
      return;
    }
    const type = lookups.data?.service_type.find((t) => t.id === form.service_type_id);
    const isRepair = type?.is_repair === true;
    const countsAsCalibration = type?.counts_as_calibration === true;
    const failed = form.condition_returned === "fail";
    // Editing a record without changing what it means doesn't ask again.
    const sameOutcome =
      !!record &&
      record.is_repair === isRepair &&
      record.counts_as_calibration === countsAsCalibration &&
      (record.condition_returned === "fail") === failed;
    if (!decision && !sameOutcome) {
      if (isRepair && failed) {
        setPrompt({ kind: "repair_failed" });
        return;
      }
      if (isRepair && form.asset_id != null) {
        try {
          const item = await api.getAsset(form.asset_id);
          // Back to the status before the failure; without one, the first ordinary tracked status.
          const fallback = lookups.data?.status.find((s) => s.is_active && s.tracks_due && !s.status_role);
          setPrompt({
            kind: "repair_passed",
            statusId: item.previous_status_id ?? fallback?.id ?? item.status_id,
            currentStatusId: item.status_id,
          });
        } catch (e) {
          handleError(e);
        }
        return;
      }
      if (countsAsCalibration && failed) {
        setPrompt({ kind: "calibration_failed" });
        return;
      }
    }
    const status = lookups.data?.status.find((s) => s.id === decision?.statusId);
    setSaving(true);
    try {
      const input = { ...converted.value, new_status_id: status?.id ?? null };
      const saved = record
        ? await api.updateCalibration(record.id, record.row_version, input, reason)
        : await api.createCalibration(input);
      const moved = status ? `; ${saved.asset_tag} is now ${status.name}` : "";
      toast.success(record ? `Service record saved${moved}` : `Service recorded for ${saved.asset_tag}${moved}`);
      if (!record && pendingFiles.length > 0) {
        try {
          await api.addAttachments({
            assetId: saved.asset_id,
            calibrationId: saved.id,
            category: "certificate",
            paths: pendingFiles,
          });
        } catch (e) {
          toast.warning("The record was saved, but its files couldn't be attached.", {
            description: e instanceof ApiError ? e.message : String(e),
            timeout: 10000,
          });
        }
      }
      onSaved(saved);
      onOpenChange(false);
    } catch (e) {
      handleError(e);
    } finally {
      setSaving(false);
    }
  }

  /** Closes the outcome prompt and saves with its answer. */
  function decide(statusId: number | null) {
    setPrompt(null);
    void save({ statusId });
  }

  /** Archives or restores; errors propagate so the confirm dialog can show them. */
  async function archive(why: string) {
    if (!record) return;
    const saved = await api.setCalibrationArchived(record.id, record.row_version, !record.is_archived, why);
    toast.success(saved.is_archived ? "Record archived" : "Record restored");
    onSaved(saved);
    onOpenChange(false);
  }

  const fixedAsset = record ? { asset_tag: record.asset_tag } : asset;
  const itemName = fixedAsset?.asset_tag ?? "This item";
  const title = record ? "Edit service record" : "Record service";

  return (
    <>
      <Modal>
        <Modal.Backdrop isOpen={isOpen} onOpenChange={onOpenChange} isDismissable={!saving}>
          {/* 48rem: half again the standard large dialog, for the two-column form. */}
        <Modal.Container size="lg" scroll="inside">
            <Modal.Dialog className="max-w-3xl!">
              <Modal.CloseTrigger />
              <Modal.Header>
                <Modal.Heading>{title}</Modal.Heading>
                {fixedAsset && (
                  <p className="text-sm text-muted">
                    {fixedAsset.asset_tag}
                    {"description" in fixedAsset && fixedAsset.description ? ` · ${fixedAsset.description}` : ""}
                  </p>
                )}
              </Modal.Header>
              <Modal.Body>
                <form
                  id="calibration-form"
                  noValidate
                  onSubmit={(e) => {
                    e.preventDefault();
                    void save();
                  }}
                  className="grid grid-cols-1 gap-4 p-1 md:grid-cols-2"
                >
                  {record?.is_archived && (
                    <Alert status="warning" className="md:col-span-2">
                      <Alert.Indicator />
                      <Alert.Content>
                        <Alert.Title>This record is archived</Alert.Title>
                        <Alert.Description>
                          It no longer counts toward the item's calibration dates.
                        </Alert.Description>
                      </Alert.Content>
                    </Alert>
                  )}
                  {formError && (
                    <Alert status="danger" className="md:col-span-2">
                      <Alert.Indicator />
                      <Alert.Content>
                        <Alert.Title>Couldn't save</Alert.Title>
                        <Alert.Description>{formError.message}</Alert.Description>
                      </Alert.Content>
                    </Alert>
                  )}

                  {!fixedAsset && (
                    <AssetPicker value={form.asset_id} onChange={(v) => set("asset_id", v)} error={errors.asset_id} />
                  )}
                  <LookupSelect
                    kind="service_type"
                    label="Service type"
                    value={form.service_type_id}
                    onChange={(v) => {
                      set("service_type_id", v);
                      // An extension is authorized by whoever records it.
                      if (lookups.data?.service_type.find((t) => t.id === v)?.is_extension) {
                        set("technician", user.display_name);
                      }
                    }}
                    lookups={lookups}
                    error={errors.service_type_id}
                    isRequired
                  />
                  <DateInput
                    label="Date of service"
                    value={form.cal_date}
                    onChange={(v) => set("cal_date", v)}
                    error={errors.cal_date}
                    isRequired
                  />
                  {isExtension && (
                    <NumberInput
                      label="Length of extension (days)"
                      value={form.extension_days}
                      onChange={(v) => set("extension_days", v == null ? null : Math.trunc(v))}
                      minValue={1}
                      step={1}
                      error={errors.extension_days}
                      isRequired
                      description={
                        extensionEnds
                          ? `Authorized for use until ${formatDate(extensionEnds)}. The next calibration date moves there; the last calibration date stays.`
                          : "The next calibration date moves to the date of service plus this many days."
                      }
                      className="md:col-span-2"
                    />
                  )}
                  <SelectInput<CalibratedBy>
                    label="Performed by"
                    isDisabled={isExtension}
                    value={form.calibrated_by}
                    onChange={(v) => {
                      if (!v || v === form.calibrated_by) return;
                      set("calibrated_by", v);
                      set("calibrated_by_id", null);
                    }}
                    options={[
                      {
                        id: "provider",
                        label: "Calibration provider",
                        description: "An outside calibration lab or an in-house technician.",
                      },
                      { id: "manufacturer", label: "Manufacturer", description: "The item's maker did the work." },
                    ]}
                    isRequired={!isExtension}
                    allowEmpty={false}
                  />
                  {form.calibrated_by === "manufacturer" ? (
                    <LookupSelect
                      key="manufacturer"
                      kind="manufacturer"
                      label="Manufacturer"
                      value={form.calibrated_by_id}
                      onChange={(v) => set("calibrated_by_id", v)}
                      lookups={lookups}
                      error={errors.calibrated_by_id}
                      isRequired={!isExtension}
                      isDisabled={isExtension}
                    />
                  ) : (
                    <LookupSelect
                      key="provider"
                      kind="provider"
                      label="Calibration provider"
                      value={form.calibrated_by_id}
                      onChange={(v) => set("calibrated_by_id", v)}
                      lookups={lookups}
                      error={errors.calibrated_by_id}
                      isRequired={!isExtension}
                      isDisabled={isExtension}
                    />
                  )}
                  <TextInput
                    label="Technician"
                    value={form.technician}
                    onChange={(v) => set("technician", v)}
                    error={errors.technician}
                    isDisabled={isExtension}
                    description={
                      isExtension ? "Whoever records the extension authorizes it." : "The person who did the work, if known."
                    }
                  />
                  <TextInput
                    label="Certificate number"
                    value={form.certificate_number}
                    onChange={(v) => set("certificate_number", v)}
                    error={errors.certificate_number}
                    isDisabled={isExtension}
                  />
                  <SelectInput<ConditionReceived>
                    label="Condition received"
                    value={form.condition_received}
                    onChange={(v) => set("condition_received", v)}
                    options={RECEIVED.map((c) => ({ id: c, label: conditionReceivedMeta[c].label }))}
                    error={errors.condition_received}
                    isRequired
                  />
                  <SelectInput<ConditionReturned>
                    label="Condition returned"
                    value={form.condition_returned}
                    onChange={(v) => set("condition_returned", v)}
                    options={RETURNED.map((c) => ({ id: c, label: conditionReturnedMeta[c].label }))}
                    error={errors.condition_returned}
                    isRequired
                  />
                  <MoneyInput
                    label="Cost"
                    value={form.cost}
                    onChange={(v) => set("cost", v)}
                    error={errors.cost_cents}
                    isDisabled={isExtension}
                  />
                  <div />
                  <TextAreaInput
                    label="Notes"
                    value={form.notes}
                    onChange={(v) => set("notes", v)}
                    className="md:col-span-2"
                  />
                  <div className="border-t border-separator pt-4 md:col-span-2">
                    {record ? (
                      <>
                        <div className="mb-2 text-sm font-medium">Certificates and files</div>
                        <AttachmentsPanel
                          assetId={record.asset_id}
                          calibrationId={record.id}
                          readOnly={record.is_archived}
                          emptyText="No files attached to this record."
                          onChanged={() => onSaved(record)}
                        />
                      </>
                    ) : folderSet ? (
                      <PendingFiles paths={pendingFiles} onChange={setPendingFiles} />
                    ) : (
                      settings.data && (
                        <p className="text-xs text-muted">
                          To attach certificates, an administrator needs to choose the attachments folder in
                          Settings.
                        </p>
                      )
                    )}
                  </div>
                  {record && (
                    <TextAreaInput
                      label="Reason for change"
                      value={reason}
                      onChange={(v) => {
                        setReason(v);
                        setErrors(({ reason: _cleared, ...rest }) => rest);
                      }}
                      error={errors.reason}
                      rows={2}
                      className="md:col-span-2"
                      description="Optional, but recommended. Recorded in the audit trail."
                    />
                  )}
                </form>
              </Modal.Body>
              <Modal.Footer>
                {record && (
                  <Button
                    variant={record.is_archived ? "secondary" : "danger-soft"}
                    className="mr-auto"
                    isDisabled={saving}
                    onPress={() => setConfirmArchive(true)}
                  >
                    {record.is_archived ? "Restore record" : "Archive record"}
                  </Button>
                )}
                <Button slot="close" variant="tertiary" isDisabled={saving}>
                  Cancel
                </Button>
                <Button type="submit" form="calibration-form" isPending={saving}>
                  {record ? "Save changes" : "Save record"}
                </Button>
              </Modal.Footer>
            </Modal.Dialog>
          </Modal.Container>
        </Modal.Backdrop>
      </Modal>

      <AlertDialog>
        <AlertDialog.Backdrop isOpen={prompt != null} onOpenChange={(open) => !open && setPrompt(null)}>
          <AlertDialog.Container size="md">
            <AlertDialog.Dialog>
              <AlertDialog.Header>
                <AlertDialog.Icon status={prompt?.kind === "repair_passed" ? "success" : "danger"} />
                <AlertDialog.Heading>
                  {prompt?.kind === "calibration_failed"
                    ? "Calibration failed"
                    : prompt?.kind === "repair_failed"
                      ? "Repair failed"
                      : "Repair passed"}
                </AlertDialog.Heading>
              </AlertDialog.Header>
              <AlertDialog.Body>
                {prompt?.kind === "calibration_failed" && (
                  <p className="text-sm text-muted">
                    {itemName} was returned as failed. Is it being sent for repair or retired? Its status changes to{" "}
                    <strong>{statusWithRole("out_for_repair")?.name ?? "Out for Repair"}</strong> or{" "}
                    <strong>{statusWithRole("retired")?.name ?? "Retired"}</strong>. Its last and next calibration
                    dates stay as they are.
                  </p>
                )}
                {prompt?.kind === "repair_failed" && (
                  <p className="text-sm text-muted">
                    {itemName} failed its repair, so it will be retired: its status changes to{" "}
                    <strong>{statusWithRole("retired")?.name ?? "Retired"}</strong>.
                  </p>
                )}
                {prompt?.kind === "repair_passed" && (
                  <div className="flex flex-col gap-4">
                    <p className="text-sm text-muted">
                      {itemName} passed its repair. What status should it have now? The suggestion is the status it
                      had before it failed.
                    </p>
                    <SelectInput
                      label="Status"
                      value={prompt.statusId}
                      onChange={(v) => setPrompt({ ...prompt, statusId: v })}
                      options={lookupOptions(lookups.data, "status", prompt.statusId)}
                      isRequired
                    />
                  </div>
                )}
              </AlertDialog.Body>
              <AlertDialog.Footer>
                <Button slot="close" variant="tertiary">
                  Cancel
                </Button>
                {prompt?.kind === "calibration_failed" && (
                  <>
                    <Button variant="danger-soft" onPress={() => decide(statusWithRole("retired")?.id ?? null)}>
                      Retire it
                    </Button>
                    <Button onPress={() => decide(statusWithRole("out_for_repair")?.id ?? null)}>Send for repair</Button>
                  </>
                )}
                {prompt?.kind === "repair_failed" && (
                  <Button variant="danger" onPress={() => decide(statusWithRole("retired")?.id ?? null)}>
                    Retire it
                  </Button>
                )}
                {prompt?.kind === "repair_passed" && (
                  <Button isDisabled={prompt.statusId == null} onPress={() => decide(prompt.statusId)}>
                    Save
                  </Button>
                )}
              </AlertDialog.Footer>
            </AlertDialog.Dialog>
          </AlertDialog.Container>
        </AlertDialog.Backdrop>
      </AlertDialog>

      <ConfirmDialog
        isOpen={confirmArchive}
        onOpenChange={setConfirmArchive}
        title={record?.is_archived ? "Restore this service record?" : "Archive this service record?"}
        confirmLabel={record?.is_archived ? "Restore record" : "Archive record"}
        tone={record?.is_archived ? "accent" : "danger"}
        askReason
        onConfirm={archive}
      >
        {record?.is_archived
          ? "The record will count toward the item's last and next calibration dates again."
          : "The record stays in the history but stops counting toward the item's last and next calibration dates. You can restore it later."}
      </ConfirmDialog>
    </>
  );
}
