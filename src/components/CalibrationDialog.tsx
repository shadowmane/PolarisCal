import { Alert, Button, ComboBox, FieldError, Input, Label, ListBox, Modal, toast } from "@heroui/react";
import { today, getLocalTimeZone } from "@internationalized/date";
import { useEffect, useState } from "react";
import type { CalibrationRecord } from "../bindings/CalibrationRecord";
import type { ConditionReceived } from "../bindings/ConditionReceived";
import type { ConditionReturned } from "../bindings/ConditionReturned";
import { ApiError, api } from "../lib/api";
import { conditionReceivedMeta, conditionReturnedMeta } from "../lib/format";
import {
  type CalibrationForm,
  calibrationToForm,
  emptyCalibrationForm,
  type FieldErrors,
  fieldErrors,
  formToCalibrationInput,
} from "../lib/forms";
import { lookupOptions, useLookups } from "../lib/lookups";
import { useAsync } from "../lib/useAsync";
import { AttachmentsPanel, PendingFiles } from "./Attachments";
import { ConfirmDialog } from "./ConfirmDialog";
import { DateInput, MoneyInput, SelectInput, TextAreaInput, TextInput } from "./fields";

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
 * Create or edit a calibration record. Pass `record` to edit; pass `asset` to
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
  }, [isOpen, record, asset]);

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

  async function save() {
    setFormError(undefined);
    const converted = formToCalibrationInput(form);
    const missingReason = record && !reason.trim();
    if (!converted.ok || missingReason) {
      setErrors({
        ...(converted.ok ? {} : converted.errors),
        ...(missingReason ? { reason: "Enter a reason for this change." } : {}),
      });
      return;
    }
    setSaving(true);
    try {
      const saved = record
        ? await api.updateCalibration(record.id, record.row_version, converted.value, reason)
        : await api.createCalibration(converted.value);
      toast.success(record ? "Calibration record saved" : `Calibration recorded for ${saved.asset_tag}`);
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

  /** Archives or restores; errors propagate so the confirm dialog can show them. */
  async function archive(why: string) {
    if (!record) return;
    const saved = await api.setCalibrationArchived(record.id, record.row_version, !record.is_archived, why);
    toast.success(saved.is_archived ? "Record archived" : "Record restored");
    onSaved(saved);
    onOpenChange(false);
  }

  const fixedAsset = record ? { asset_tag: record.asset_tag } : asset;
  const title = record ? "Edit calibration record" : "Record calibration";

  return (
    <>
      <Modal>
        <Modal.Backdrop isOpen={isOpen} onOpenChange={onOpenChange} isDismissable={!saving}>
          <Modal.Container size="lg" scroll="inside">
            <Modal.Dialog>
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
                  <DateInput
                    label="Date of calibration"
                    value={form.cal_date}
                    onChange={(v) => set("cal_date", v)}
                    error={errors.cal_date}
                    isRequired
                  />
                  <TextInput
                    label="Certificate number"
                    value={form.certificate_number}
                    onChange={(v) => set("certificate_number", v)}
                    error={errors.certificate_number}
                  />
                  <SelectInput
                    label="Calibrated by"
                    value={form.provider_id}
                    onChange={(v) => set("provider_id", v)}
                    options={lookupOptions(lookups.data, "provider", form.provider_id)}
                    error={errors.provider_id}
                    isRequired
                    placeholder={
                      lookups.data && lookups.data.provider.length === 0
                        ? "Add providers under Lists first"
                        : "Select…"
                    }
                  />
                  <TextInput
                    label="Technician"
                    value={form.technician}
                    onChange={(v) => set("technician", v)}
                    error={errors.technician}
                    description="The person who did the work, if known."
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
                      description="Required when editing an existing record. Recorded in the audit trail."
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

      <ConfirmDialog
        isOpen={confirmArchive}
        onOpenChange={setConfirmArchive}
        title={record?.is_archived ? "Restore this calibration record?" : "Archive this calibration record?"}
        confirmLabel={record?.is_archived ? "Restore record" : "Archive record"}
        tone={record?.is_archived ? "accent" : "danger"}
        requireReason
        onConfirm={archive}
      >
        {record?.is_archived
          ? "The record will count toward the item's last and next calibration dates again."
          : "The record stays in the history but stops counting toward the item's last and next calibration dates. You can restore it later."}
      </ConfirmDialog>
    </>
  );
}
