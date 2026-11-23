// Settings > Certificate Template: the organization's certificate layout and numbering,
// with a live preview using example details.

import { Button, Card, toast } from "@heroui/react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { ImagePlus, Trash2 } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import type { CertificateTemplate } from "../bindings/CertificateTemplate";
import { ApiError, api } from "../lib/api";
import { useAuth } from "../lib/auth";
import { nextCertificateNumber, sampleCertificateDetails } from "../lib/certificates";
import { type FieldErrors, fieldErrors } from "../lib/forms";
import { CertificatePreview } from "./CertificateView";
import { ErrorAlert, Loading, LockableTitle } from "./common";
import { NumberInput, SelectInput, SwitchInput, TextAreaInput, TextInput } from "./fields";

function Group({ title, description, children }: { title: string; description?: string; children: React.ReactNode }) {
  const { can } = useAuth();
  return (
    <Card>
      <Card.Header>
        <LockableTitle locked={!can("admin")}>{title}</LockableTitle>
        {description && <Card.Description>{description}</Card.Description>}
      </Card.Header>
      <Card.Content className="grid grid-cols-1 gap-4 md:grid-cols-2">{children}</Card.Content>
    </Card>
  );
}

export function CertificateTemplateSettings() {
  const { can } = useAuth();
  const isAdmin = can("admin");
  const [saved, setSaved] = useState<CertificateTemplate>();
  const [form, setForm] = useState<CertificateTemplate>();
  const [loadError, setLoadError] = useState<ApiError>();
  const [errors, setErrors] = useState<FieldErrors>({});
  const [busy, setBusy] = useState(false);
  const sample = useMemo(sampleCertificateDetails, []);

  useEffect(() => {
    api
      .getCertificateTemplate()
      .then((t) => {
        setSaved(t);
        setForm(t);
      })
      .catch((e) => setLoadError(e instanceof ApiError ? e : new ApiError("database", String(e))));
  }, []);

  if (loadError) return <ErrorAlert error={loadError} />;
  if (!form || !saved) return <Loading />;
  const dirty = JSON.stringify(form) !== JSON.stringify(saved);
  const locked = !isAdmin || busy;

  function set<K extends keyof CertificateTemplate>(key: K, value: CertificateTemplate[K]) {
    setForm((f) => (f ? { ...f, [key]: value } : f));
    setErrors(({ [key]: _cleared, ...rest }) => rest);
  }

  async function chooseLogo() {
    const path = await openDialog({
      title: "Choose a logo",
      multiple: false,
      filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "gif", "webp", "bmp", "svg"] }],
    });
    if (typeof path !== "string") return;
    try {
      set("logo", await api.readImageFile(path));
    } catch (e) {
      toast.danger(e instanceof ApiError ? e.message : String(e));
    }
  }

  async function save() {
    if (!form) return;
    setBusy(true);
    try {
      const next = await api.updateCertificateTemplate(form);
      setSaved(next);
      setForm(next);
      setErrors({});
      toast.success("Certificate template saved");
    } catch (e) {
      const fields = fieldErrors(e);
      setErrors(fields);
      if (Object.keys(fields).length === 0) toast.danger(e instanceof ApiError ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="grid grid-cols-1 items-start gap-6 xl:grid-cols-[minmax(0,1fr)_minmax(0,1fr)]">
      <div className="flex flex-col gap-6">
        {!isAdmin && (
          <p className="text-sm text-muted">Only an administrator can change the certificate template.</p>
        )}
        <Group title="Organization" description="Shown at the top of every certificate.">
          <TextInput
            label="Name"
            value={form.organization_name}
            onChange={(v) => set("organization_name", v)}
            isDisabled={locked}
            className="md:col-span-2"
          />
          <TextAreaInput
            label="Address"
            value={form.organization_address}
            onChange={(v) => set("organization_address", v)}
            isDisabled={locked}
            rows={3}
            className="md:col-span-2"
          />
          <TextInput
            label="Contact line"
            value={form.organization_contact}
            onChange={(v) => set("organization_contact", v)}
            isDisabled={locked}
            placeholder="e.g. (555) 010-0100 · lab@example.com · example.com"
            className="md:col-span-2"
          />
          <div className="flex flex-col gap-2 md:col-span-2">
            <span className="text-sm font-medium">Logo</span>
            <div className="flex flex-wrap items-center gap-3">
              {form.logo ? (
                <img src={form.logo} alt="Logo" className="h-14 max-w-48 rounded bg-white object-contain p-1" />
              ) : (
                <span className="text-sm text-muted">No logo</span>
              )}
              <Button variant="secondary" size="sm" isDisabled={locked} onPress={() => void chooseLogo()}>
                <ImagePlus className="size-4" /> {form.logo ? "Change…" : "Choose…"}
              </Button>
              {form.logo && (
                <Button variant="tertiary" size="sm" isDisabled={locked} onPress={() => set("logo", null)}>
                  <Trash2 className="size-4" /> Remove
                </Button>
              )}
            </div>
            {errors.logo && <p className="text-xs text-danger">{errors.logo}</p>}
          </div>
          <SelectInput
            label="Logo size"
            value={form.logo_size}
            onChange={(v) => v && set("logo_size", v)}
            options={[
              { id: "small", label: "Small" },
              { id: "medium", label: "Medium" },
              { id: "large", label: "Large" },
            ]}
            isRequired
            allowEmpty={false}
            isDisabled={locked}
          />
          <SelectInput
            label="Header layout"
            value={form.header_layout}
            onChange={(v) => v && set("header_layout", v)}
            options={[
              { id: "logo_left", label: "Logo left, details right" },
              { id: "logo_center", label: "Everything centered" },
            ]}
            isRequired
            allowEmpty={false}
            isDisabled={locked}
          />
        </Group>

        <Group title="Appearance">
          <TextInput
            label="Title"
            value={form.title}
            onChange={(v) => set("title", v)}
            error={errors.title}
            isRequired
            isDisabled={locked}
            className="md:col-span-2"
          />
          <div className="flex flex-col gap-1">
            <label htmlFor="accent-color" className="text-sm font-medium">
              Accent color
            </label>
            <div className="flex items-center gap-2">
              <input
                id="accent-color"
                type="color"
                value={/^#[0-9a-f]{6}$/i.test(form.accent_color) ? form.accent_color : "#000000"}
                onChange={(e) => set("accent_color", e.target.value)}
                disabled={locked}
                className="h-10 w-12 shrink-0 cursor-pointer rounded-lg border border-separator bg-transparent p-1 disabled:cursor-default"
              />
              <TextInput
                label=""
                value={form.accent_color}
                onChange={(v) => set("accent_color", v)}
                error={errors.accent_color}
                isDisabled={locked}
                maxLength={7}
                className="flex-1 [&>label]:hidden"
              />
            </div>
            <p className="text-xs text-muted">Title, lines, and headings.</p>
          </div>
          <SelectInput
            label="Border"
            value={form.border}
            onChange={(v) => v && set("border", v)}
            options={[
              { id: "none", label: "None" },
              { id: "single", label: "Single line" },
              { id: "double", label: "Double line" },
              { id: "thick", label: "Thick line" },
            ]}
            isRequired
            allowEmpty={false}
            isDisabled={locked}
          />
          <SelectInput
            label="Font"
            value={form.font}
            onChange={(v) => v && set("font", v)}
            options={[
              { id: "sans", label: "Sans-serif (Segoe UI)" },
              { id: "serif", label: "Serif (Cambria)" },
            ]}
            isRequired
            allowEmpty={false}
            isDisabled={locked}
          />
          <SelectInput
            label="Paper size"
            value={form.paper}
            onChange={(v) => v && set("paper", v)}
            options={[
              { id: "letter", label: "Letter (8.5 × 11 in)" },
              { id: "a4", label: "A4 (210 × 297 mm)" },
            ]}
            isRequired
            allowEmpty={false}
            isDisabled={locked}
          />
          <SelectInput
            label="Temperature unit"
            value={form.temperature_unit}
            onChange={(v) => v && set("temperature_unit", v)}
            options={[
              { id: "celsius", label: "°C  Celsius" },
              { id: "fahrenheit", label: "°F  Fahrenheit" },
            ]}
            isRequired
            allowEmpty={false}
            isDisabled={locked}
          />
        </Group>

        <Group title="Wording">
          <TextAreaInput
            label="Statement"
            description="Printed above the sign-off lines, e.g. how the calibration is traceable."
            value={form.statement}
            onChange={(v) => set("statement", v)}
            isDisabled={locked}
            rows={4}
            className="md:col-span-2"
          />
          <TextAreaInput
            label="Footer"
            description="Small print at the bottom of the page."
            value={form.footer}
            onChange={(v) => set("footer", v)}
            isDisabled={locked}
            rows={2}
            className="md:col-span-2"
          />
        </Group>

        <Group
          title="Certificate numbers"
          description="Each certificate gets the next number when it's generated."
        >
          <TextInput
            label="Number format"
            value={form.number_pattern}
            onChange={(v) => set("number_pattern", v)}
            error={errors.number_pattern}
            description="{SEQ} is the counter. {YYYY}, {YY}, {MM}, and {DD} are the date it's issued."
            isRequired
            isDisabled={locked}
            maxLength={40}
            className="md:col-span-2"
          />
          <NumberInput
            label="Counter digits"
            value={form.number_digits}
            onChange={(v) => set("number_digits", v ?? 1)}
            minValue={1}
            error={errors.number_digits}
            description="Padded with zeros, e.g. 4 digits gives 0007."
            isDisabled={locked}
          />
          <NumberInput
            label="Next number"
            value={form.next_number}
            onChange={(v) => set("next_number", v ?? 1)}
            minValue={1}
            error={errors.next_number}
            description="Change it to continue from an earlier system."
            isDisabled={locked}
          />
          <div className="md:col-span-2">
            <SwitchInput
              label="Start again at 1 each year"
              isSelected={form.number_reset_yearly}
              onChange={(v) => set("number_reset_yearly", v)}
              isDisabled={locked}
            />
          </div>
          <p className="text-sm md:col-span-2">
            Next certificate: <b>{nextCertificateNumber(form)}</b>
          </p>
        </Group>

        {isAdmin && (
          <div className="sticky bottom-0 -mx-1 flex justify-end gap-2 bg-background/90 px-1 py-3 backdrop-blur">
            <Button variant="tertiary" isDisabled={!dirty || busy} onPress={() => setForm(saved)}>
              Discard changes
            </Button>
            <Button isDisabled={!dirty} isPending={busy} onPress={() => void save()}>
              Save template
            </Button>
          </div>
        )}
      </div>

      <div className="flex flex-col gap-2 xl:sticky xl:top-0">
        <p className="text-sm text-muted">Preview with example details</p>
        <CertificatePreview details={sample} template={form} number={nextCertificateNumber(form)} />
      </div>
    </div>
  );
}
