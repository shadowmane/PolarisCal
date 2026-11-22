import { Alert, Button, Form, Label, toast, ToggleButton, ToggleButtonGroup } from "@heroui/react";
import { Boxes, Gauge } from "lucide-react";
import { useEffect, useState } from "react";
import { useNavigate, useParams, useSearchParams } from "react-router";
import type { AssetKind } from "../bindings/AssetKind";
import type { IntervalUnit } from "../bindings/IntervalUnit";
import { ErrorAlert, Loading, PageHeader } from "../components/common";
import { ConfirmDialog } from "../components/ConfirmDialog";
import { LookupSelect } from "../components/LookupSelect";
import {
  CheckboxInput,
  DateInput,
  FormSection,
  NumberInput,
  SelectInput,
  TextAreaInput,
  TextInput,
} from "../components/fields";
import { ApiError, api } from "../lib/api";
import { formatDate } from "../lib/format";
import {
  type AssetForm,
  assetToForm,
  cloneAssetForm,
  emptyAssetForm,
  type FieldErrors,
  fieldErrors,
  formToAssetInput,
} from "../lib/forms";
import { lookupOptions, useLookups } from "../lib/lookups";
import { useAsync } from "../lib/useAsync";
import { useAuth } from "../lib/auth";

/** Which server error key a form field reports under. */
const ERROR_KEY: Partial<Record<keyof AssetForm, string>> = {
  interval_value: "cal_interval",
  interval_unit: "cal_interval",
};

export function AssetFormPage() {
  const { id } = useParams();
  const editing = id !== undefined;
  const assetId = Number(id);
  const [params] = useSearchParams();
  const navigate = useNavigate();
  // `?clone=ID`: a new item pre-filled from an existing one.
  const cloneId = editing ? null : Number(params.get("clone")) || null;

  const lookups = useLookups();
  const systems = useAsync(() => api.listAssets({ kind: "system" }), []);
  const existing = useAsync(() => (editing ? api.getAsset(assetId) : Promise.resolve(null)), [assetId]);
  const source = useAsync(() => (cloneId ? api.getAsset(cloneId) : Promise.resolve(null)), [cloneId]);
  const records = useAsync(
    () => (editing ? api.listCalibrations({ asset_id: assetId }) : Promise.resolve([])),
    [assetId],
  );

  const [form, setForm] = useState<AssetForm | null>(null);
  const [errors, setErrors] = useState<FieldErrors>({});
  const [formError, setFormError] = useState<ApiError>();
  const [saving, setSaving] = useState(false);
  const [askReason, setAskReason] = useState(false);
  // Set while the typed asset tag is already used by another item.
  const [tagTaken, setTagTaken] = useState<string>();
  const { can } = useAuth();

  // Initialize the form once its source data has loaded.
  useEffect(() => {
    if (form) return;
    if (editing) {
      if (existing.data) setForm(assetToForm(existing.data));
    } else if (cloneId) {
      if (source.data) setForm(cloneAssetForm(source.data));
    } else if (lookups.data) {
      const statuses = lookups.data.status.filter((s) => s.is_active);
      const active = statuses.find((s) => s.name === "Active") ?? statuses[0];
      const parent = params.get("parent");
      setForm(
        emptyAssetForm({
          kind: params.get("kind") === "system" ? "system" : "instrument",
          parent_id: parent ? Number(parent) : null,
          status_id: active?.id ?? null,
        }),
      );
    }
  }, [editing, existing.data, cloneId, source.data, lookups.data, form, params]);

  // Checks the asset tag shortly after typing stops, so a duplicate shows before saving.
  // Tags are unique ignoring case; archived items count. Saving still checks too.
  const typedTag = form?.asset_tag.trim() ?? "";
  useEffect(() => {
    setTagTaken(undefined);
    if (!typedTag) return;
    let current = true;
    const timer = setTimeout(async () => {
      try {
        const matches = await api.listAssets({ search: typedTag, include_archived: true });
        const other = matches.find(
          (a) => a.id !== assetId && a.asset_tag.toLowerCase() === typedTag.toLowerCase(),
        );
        if (current && other) {
          setTagTaken(
            `${other.asset_tag} is already used by ${other.description}${other.is_archived ? " (archived)" : ""}.`,
          );
        }
      } catch {
        // Not worth interrupting typing for; saving reports a duplicate anyway.
      }
    }, 300);
    return () => {
      current = false;
      clearTimeout(timer);
    };
  }, [typedTag, assetId]);

  if (!can("edit_records")) {
    return <ErrorAlert error={{ kind: "forbidden", message: "Your role can't add or edit items." }} />;
  }
  const loadError = lookups.error ?? existing.error ?? source.error ?? systems.error;
  if (loadError) return <ErrorAlert error={loadError} onRetry={() => window.location.reload()} />;
  if (!form || !lookups.data) return <Loading />;

  const original = existing.data;
  const hasRecords = (records.data?.length ?? 0) > 0;
  const hasChildren = (original?.child_count ?? 0) > 0;

  function set<K extends keyof AssetForm>(key: K, value: AssetForm[K]) {
    setForm((f) => (f ? { ...f, [key]: value } : f));
    const errorKey = ERROR_KEY[key] ?? key;
    setErrors(({ [errorKey]: _cleared, ...rest }) => rest);
  }

  /** Validates locally; edits then ask for a reason before saving. */
  function submit() {
    if (!form) return;
    setFormError(undefined);
    const converted = formToAssetInput(form);
    if (!converted.ok) {
      setErrors(converted.errors);
      return;
    }
    if (editing) setAskReason(true);
    else void save("");
  }

  async function save(reason: string) {
    if (!form) return;
    const converted = formToAssetInput(form);
    if (!converted.ok) return;
    setSaving(true);
    try {
      const saved = editing
        ? await api.updateAsset(assetId, original!.row_version, converted.value, reason)
        : await api.createAsset(converted.value);
      toast.success(editing ? `Saved ${saved.asset_tag}` : `Created ${saved.asset_tag}`);
      navigate(`/assets/${saved.id}`, { replace: true });
    } catch (e) {
      const err = e instanceof ApiError ? e : new ApiError("database", String(e));
      if (err.kind === "duplicate") {
        setErrors({ asset_tag: err.message });
      } else {
        const fields = fieldErrors(err);
        setErrors(fields);
        // Show a banner unless every problem is attached to a visible field.
        if (Object.keys(fields).length === 0 || err.fields.some((f) => !f.field)) setFormError(err);
      }
    } finally {
      setSaving(false);
    }
  }

  const systemOptions = (systems.data ?? [])
    .filter((s) => s.id !== assetId && (!s.is_archived || s.id === form.parent_id))
    .map((s) => ({ id: s.id, label: s.asset_tag, description: s.description }));

  const kindLabel = form.kind === "system" ? "System" : "Instrument";

  return (
    <Form
      validationBehavior="aria"
      onSubmit={(e) => {
        e.preventDefault();
        submit();
      }}
    >
      <PageHeader
        title={editing ? `Edit ${original?.asset_tag}` : `New ${kindLabel}`}
        description={
          editing
            ? original?.description
            : source.data
              ? `Copied from ${source.data.asset_tag}. Enter this one's own asset tag, serial number and dates.`
              : form.kind === "system"
              ? "A System groups the Instruments it contains. It can have its own calibration schedule too."
              : "A single item under calibration control."
        }
        actions={
          <>
            <Button variant="tertiary" onPress={() => navigate(-1)} isDisabled={saving}>
              Cancel
            </Button>
            <Button type="submit" isPending={saving}>
              {editing ? "Save changes" : `Create ${kindLabel}`}
            </Button>
          </>
        }
      />

      {formError && (
        <Alert status={formError.kind === "conflict" ? "warning" : "danger"} className="mb-6">
          <Alert.Indicator />
          <Alert.Content>
            <Alert.Title>
              {formError.kind === "conflict" ? "Someone else changed this item" : "Couldn't save"}
            </Alert.Title>
            <Alert.Description>{formError.message}</Alert.Description>
          </Alert.Content>
          {formError.kind === "conflict" && (
            <Button size="sm" variant="secondary" onPress={() => window.location.reload()}>
              Reload
            </Button>
          )}
        </Alert>
      )}

      <div className="w-full rounded-2xl bg-surface p-6">
        <FormSection title="Identification">
          <div className="flex flex-col gap-2 md:col-span-2">
            <Label>Type</Label>
            <ToggleButtonGroup
              aria-label="Type"
              selectionMode="single"
              disallowEmptySelection
              selectedKeys={[form.kind]}
              onSelectionChange={(keys) => {
                const [k] = [...keys];
                if (k) set("kind", k as AssetKind);
              }}
              isDisabled={hasChildren}
            >
              <ToggleButton id="instrument">
                <Gauge className="size-4" /> Instrument
              </ToggleButton>
              <ToggleButton id="system">
                <Boxes className="size-4" /> System
              </ToggleButton>
            </ToggleButtonGroup>
            {(hasChildren || errors.kind) && (
              <p className={`text-xs ${errors.kind ? "text-danger" : "text-muted"}`}>
                {errors.kind ?? "This System contains items, so it can't be changed to an Instrument."}
              </p>
            )}
          </div>
          <TextInput
            label="Asset tag"
            value={form.asset_tag}
            onChange={(v) => set("asset_tag", v)}
            error={errors.asset_tag ?? tagTaken}
            isRequired
            maxLength={64}
            autoFocus={!editing}
            placeholder="e.g. DMM-0042"
          />
          {form.kind === "instrument" ? (
            <SelectInput
              label="Part of System"
              value={form.parent_id}
              onChange={(v) => set("parent_id", v)}
              options={systemOptions}
              error={errors.parent_id}
              placeholder="Standalone"
            />
          ) : (
            <div />
          )}
          <TextInput
            label="Description"
            value={form.description}
            onChange={(v) => set("description", v)}
            error={errors.description}
            isRequired
            className="md:col-span-2"
            placeholder="e.g. Digital multimeter"
          />
          <LookupSelect
            kind="manufacturer"
            label="Manufacturer"
            value={form.manufacturer_id}
            onChange={(v) => set("manufacturer_id", v)}
            lookups={lookups}
            error={errors.manufacturer_id}
          />
          <TextInput
            label="Model number"
            value={form.model_number}
            onChange={(v) => set("model_number", v)}
            error={errors.model_number}
          />
          <TextInput
            label="Serial number"
            value={form.serial_number}
            onChange={(v) => set("serial_number", v)}
            error={errors.serial_number}
          />
        </FormSection>

        <FormSection title="Assignment" description="Where the item is and who is responsible for it.">
          <SelectInput
            label="Status"
            value={form.status_id}
            onChange={(v) => set("status_id", v)}
            options={lookupOptions(lookups.data, "status", form.status_id)}
            error={errors.status_id}
            isRequired
          />
          <LookupSelect
            kind="location"
            label="Location"
            value={form.location_id}
            onChange={(v) => set("location_id", v)}
            lookups={lookups}
            error={errors.location_id}
          />
          <LookupSelect
            kind="department"
            label="Department"
            value={form.department_id}
            onChange={(v) => set("department_id", v)}
            lookups={lookups}
            error={errors.department_id}
          />
          <LookupSelect
            kind="owner"
            label="Owner"
            value={form.owner_id}
            onChange={(v) => set("owner_id", v)}
            lookups={lookups}
            error={errors.owner_id}
          />
        </FormSection>

        <FormSection
          title="Calibration schedule"
          description="The next calibration date is calculated from the last calibration date plus the interval."
        >
          {/* Interval, unit and End of month share a full-width line (too wide for one column);
              the hint or error goes under all three. */}
          <div className="flex flex-col gap-1 md:col-span-2">
            <div className="flex flex-wrap items-end gap-x-3 gap-y-2">
              <NumberInput
                label="Calibration interval"
                value={form.interval_value}
                onChange={(v) => set("interval_value", v)}
                minValue={1}
                isInvalid={!!errors.cal_interval}
                className="w-36"
              />
              <SelectInput<IntervalUnit>
                label="Unit"
                value={form.interval_unit}
                onChange={(v) => v && set("interval_unit", v)}
                options={[
                  { id: "months", label: "Months" },
                  { id: "days", label: "Days" },
                ]}
                isRequired
                allowEmpty={false}
                className="w-32"
              />
              <div className="flex h-10 items-center">
                <CheckboxInput
                  label="End of month"
                  isSelected={form.interval_end_of_month}
                  onChange={(v) => set("interval_end_of_month", v)}
                  isDisabled={form.interval_value == null}
                />
              </div>
            </div>
            <p className={`text-xs ${errors.cal_interval ? "text-danger" : "text-muted"}`}>
              {errors.cal_interval ?? "Leave the interval empty if the item has no fixed cycle."}
            </p>
          </div>
          {/* Calibration dates on the left, the item's other dates on the right. */}
          <DateInput
            label="First calibration"
            value={form.first_cal_date}
            onChange={(v) => set("first_cal_date", v)}
            error={errors.first_cal_date}
            description="Filled in automatically from the earliest known calibration if left empty."
          />
          <DateInput
            label="Date put into service"
            value={form.in_service_date}
            onChange={(v) => set("in_service_date", v)}
            error={errors.in_service_date}
          />
          {hasRecords ? (
            <DateInput
              label="Last calibration"
              value={original?.last_cal_date ?? null}
              onChange={() => {}}
              isDisabled
              description="Set by the latest calibration record."
            />
          ) : (
            <DateInput
              label="Last calibration"
              value={form.last_cal_date}
              onChange={(v) => set("last_cal_date", v)}
              error={errors.last_cal_date}
              description="For items brought over from another system. Once a calibration is recorded, the latest record sets this date."
            />
          )}
          <DateInput
            label="Warranty ends"
            value={form.warranty_end_date}
            onChange={(v) => set("warranty_end_date", v)}
            error={errors.warranty_end_date}
            description="Leave empty for one year after the date put into service."
          />
          {editing && original && (
            <div className="flex flex-col justify-center gap-1 text-sm">
              <span className="text-muted">Next calibration (currently)</span>
              <span className="font-medium">{formatDate(original.next_cal_date)}</span>
            </div>
          )}
        </FormSection>

        <FormSection title="Notes">
          <TextAreaInput
            label="Notes"
            value={form.notes}
            onChange={(v) => set("notes", v)}
            className="md:col-span-2"
            rows={4}
          />
        </FormSection>
      </div>
      <ConfirmDialog
        isOpen={askReason}
        onOpenChange={setAskReason}
        title={`Save changes to ${original?.asset_tag ?? "this item"}?`}
        confirmLabel="Save changes"
        tone="accent"
        askReason
        onConfirm={(reason) => save(reason)}
      >
        Changes to an existing item are recorded in its history, with your reason if you give one.
      </ConfirmDialog>
    </Form>
  );
}
