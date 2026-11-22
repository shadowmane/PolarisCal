// Form fields built from HeroUI v3 parts, wired for controlled state and
// server-side validation messages (`error` shows below the field).

import {
  Calendar,
  Checkbox,
  DateField,
  DatePicker,
  Description,
  FieldError,
  I18nProvider,
  Input,
  Label,
  ListBox,
  NumberField,
  Select,
  Switch,
  TextArea,
  TextField,
} from "@heroui/react";
import { X } from "lucide-react";
import type { ReactNode } from "react";
import { useCurrencySymbol } from "../lib/currency";
import { dateInputLocale, useDateFormat } from "../lib/dates";
import { fromDateValue, toDateValue } from "../lib/forms";

interface BaseProps {
  label: string;
  error?: string;
  description?: ReactNode;
  isRequired?: boolean;
  isDisabled?: boolean;
  className?: string;
}

export function TextInput({
  label,
  value,
  onChange,
  error,
  description,
  isRequired,
  isDisabled,
  className,
  placeholder,
  autoFocus,
  maxLength,
  type = "text",
  autoComplete,
}: BaseProps & {
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
  autoFocus?: boolean;
  maxLength?: number;
  type?: "text" | "password" | "email";
  autoComplete?: string;
}) {
  return (
    <TextField
      variant="secondary"
      value={value}
      onChange={onChange}
      isRequired={isRequired}
      isDisabled={isDisabled}
      isInvalid={!!error}
      validationBehavior="aria"
      className={className}
      autoFocus={autoFocus}
      maxLength={maxLength}
    >
      <Label>{label}</Label>
      <Input placeholder={placeholder} type={type} autoComplete={autoComplete} />
      {description && <Description>{description}</Description>}
      <FieldError>{error}</FieldError>
    </TextField>
  );
}

export function TextAreaInput({
  label,
  value,
  onChange,
  error,
  description,
  isDisabled,
  className,
  rows = 3,
  placeholder,
}: BaseProps & { value: string; onChange: (v: string) => void; rows?: number; placeholder?: string }) {
  return (
    <TextField
      variant="secondary"
      value={value}
      onChange={onChange}
      isDisabled={isDisabled}
      isInvalid={!!error}
      validationBehavior="aria"
      className={className}
    >
      <Label>{label}</Label>
      <TextArea rows={rows} placeholder={placeholder} />
      {description && <Description>{description}</Description>}
      <FieldError>{error}</FieldError>
    </TextField>
  );
}

export function NumberInput({
  label,
  value,
  onChange,
  error,
  description,
  isRequired,
  isDisabled,
  className,
  minValue,
  formatOptions,
  step,
  prefix,
  isInvalid,
  "aria-label": ariaLabel,
}: Omit<BaseProps, "label"> & {
  label?: string;
  /** Shows the field as invalid without an error message under it (shown elsewhere). */
  isInvalid?: boolean;
  value: number | null;
  onChange: (v: number | null) => void;
  minValue?: number;
  step?: number;
  formatOptions?: Intl.NumberFormatOptions;
  /** Shown inside the field before the number, e.g. a currency symbol. */
  prefix?: ReactNode;
  "aria-label"?: string;
}) {
  return (
    <NumberField
      variant="secondary"
      value={value ?? Number.NaN}
      onChange={(v) => onChange(v == null || Number.isNaN(v) ? null : v)}
      isRequired={isRequired}
      isDisabled={isDisabled}
      isInvalid={isInvalid ?? !!error}
      validationBehavior="aria"
      minValue={minValue}
      step={step}
      formatOptions={formatOptions}
      className={className}
      aria-label={ariaLabel}
    >
      {label && <Label>{label}</Label>}
      <NumberField.Group className={prefix ? "flex items-center" : undefined}>
        {prefix && <span className="shrink-0 pl-3 text-sm leading-none text-muted select-none">{prefix}</span>}
        <NumberField.Input />
      </NumberField.Group>
      {description && <Description>{description}</Description>}
      <FieldError>{error}</FieldError>
    </NumberField>
  );
}

/** An amount of money in the organization's currency (Settings). */
export function MoneyInput(props: Omit<Parameters<typeof NumberInput>[0], "formatOptions" | "minValue" | "prefix">) {
  const symbol = useCurrencySymbol();
  return (
    <NumberInput
      {...props}
      minValue={0}
      step={0.01}
      prefix={symbol}
      formatOptions={{ minimumFractionDigits: 2, maximumFractionDigits: 2 }}
    />
  );
}

export function DateInput({
  label,
  value,
  onChange,
  error,
  description,
  isRequired,
  isDisabled,
  className,
}: BaseProps & { value: string | null; onChange: (v: string | null) => void }) {
  // The day, month and year boxes follow the organization's date format (Settings).
  const locale = dateInputLocale(useDateFormat());
  const picker = (
    <DatePicker
      value={toDateValue(value)}
      onChange={(v) => onChange(fromDateValue(v))}
      isRequired={isRequired}
      isDisabled={isDisabled}
      isInvalid={!!error}
      validationBehavior="aria"
      className={className}
      granularity="day"
    >
      <Label>{label}</Label>
      <DateField.Group variant="secondary">
        <DateField.Input>{(segment) => <DateField.Segment segment={segment} />}</DateField.Input>
        <DateField.Suffix>
          {value && !isDisabled && (
            // A plain <button>: a HeroUI/RAC Button here would become the picker trigger.
            <button
              type="button"
              aria-label={`Clear ${label}`}
              className="grid size-6 place-items-center rounded-md text-muted hover:bg-default hover:text-foreground"
              onClick={() => onChange(null)}
            >
              <X className="size-3.5" />
            </button>
          )}
          <DatePicker.Trigger>
            <DatePicker.TriggerIndicator />
          </DatePicker.Trigger>
        </DateField.Suffix>
      </DateField.Group>
      {description && <Description>{description}</Description>}
      <FieldError>{error}</FieldError>
      <DatePicker.Popover>
        <Calendar aria-label={label}>
          <Calendar.Header>
            <Calendar.YearPickerTrigger>
              <Calendar.YearPickerTriggerHeading />
              <Calendar.YearPickerTriggerIndicator />
            </Calendar.YearPickerTrigger>
            <Calendar.NavButton slot="previous" />
            <Calendar.NavButton slot="next" />
          </Calendar.Header>
          <Calendar.Grid>
            <Calendar.GridHeader>{(day) => <Calendar.HeaderCell>{day}</Calendar.HeaderCell>}</Calendar.GridHeader>
            <Calendar.GridBody>{(date) => <Calendar.Cell date={date} />}</Calendar.GridBody>
          </Calendar.Grid>
        </Calendar>
      </DatePicker.Popover>
    </DatePicker>
  );
  return locale ? <I18nProvider locale={locale}>{picker}</I18nProvider> : picker;
}

export interface Option<K extends string | number> {
  id: K;
  label: string;
  description?: string;
}

/** Single-choice dropdown. `allowEmpty` adds a clear button for optional fields. */
export function SelectInput<K extends string | number>({
  label,
  value,
  onChange,
  options,
  error,
  description,
  isRequired,
  isDisabled,
  className,
  placeholder = "Select…",
  allowEmpty = !isRequired,
}: BaseProps & {
  value: K | null;
  onChange: (v: K | null) => void;
  options: Option<K>[];
  placeholder?: string;
  allowEmpty?: boolean;
}) {
  return (
    <Select
      variant="secondary"
      value={value}
      onChange={(k) => {
        if (k == null) return onChange(null);
        // Keys come back as given for strings, but stringified numbers stay numbers in RAC; normalize.
        const match = options.find((o) => String(o.id) === String(k));
        onChange(match ? match.id : null);
      }}
      placeholder={placeholder}
      isRequired={isRequired}
      isDisabled={isDisabled}
      isInvalid={!!error}
      validationBehavior="aria"
      className={className}
    >
      <Label>{label}</Label>
      <Select.Trigger>
        {/* Show just the label when closed; descriptions only appear in the open list. */}
        <Select.Value>
          {({ isPlaceholder, selectedText, defaultChildren }) =>
            isPlaceholder ? defaultChildren : selectedText
          }
        </Select.Value>
        {allowEmpty && value != null && <Select.ClearButton />}
        <Select.Indicator />
      </Select.Trigger>
      {description && <Description>{description}</Description>}
      <FieldError>{error}</FieldError>
      <Select.Popover>
        <ListBox>
          {options.map((o) => (
            <ListBox.Item key={String(o.id)} id={o.id} textValue={o.label}>
              <div className="flex flex-col">
                <span>{o.label}</span>
                {o.description && <span className="text-xs text-muted">{o.description}</span>}
              </div>
              <ListBox.ItemIndicator />
            </ListBox.Item>
          ))}
        </ListBox>
      </Select.Popover>
    </Select>
  );
}

// The clickable part of a HeroUI switch is `Switch.Content`, so the control must sit
// inside it; descriptions go beside it, under the root.
export function SwitchInput({
  label,
  description,
  isSelected,
  onChange,
  isDisabled,
}: {
  label: string;
  description?: string;
  isSelected: boolean;
  onChange: (v: boolean) => void;
  isDisabled?: boolean;
}) {
  return (
    <Switch isSelected={isSelected} onChange={onChange} isDisabled={isDisabled} className="flex flex-col items-start gap-0.5">
      <Switch.Content className="flex cursor-pointer flex-row items-start gap-3">
        <Switch.Control className="mt-0.5 shrink-0">
          <Switch.Thumb />
        </Switch.Control>
        <Label className="cursor-pointer">{label}</Label>
      </Switch.Content>
      {description && <Description className="pl-[3.25rem]">{description}</Description>}
    </Switch>
  );
}

/** A small inline on/off switch for filters ("Show archived"). */
export function ToggleSwitch({
  label,
  isSelected,
  onChange,
}: {
  label: string;
  isSelected: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <Switch size="sm" isSelected={isSelected} onChange={onChange}>
      <Switch.Content className="flex cursor-pointer flex-row items-center gap-2">
        <Switch.Control className="shrink-0">
          <Switch.Thumb />
        </Switch.Control>
        <span className="text-sm whitespace-nowrap">{label}</span>
      </Switch.Content>
    </Switch>
  );
}

/** A checkbox with a label and optional description. The whole label is clickable. */
export function CheckboxInput({
  label,
  description,
  isSelected,
  onChange,
  isDisabled,
}: {
  label: string;
  description?: string;
  isSelected: boolean;
  onChange: (v: boolean) => void;
  isDisabled?: boolean;
}) {
  return (
    <Checkbox
      variant="secondary"
      isSelected={isSelected}
      onChange={onChange}
      isDisabled={isDisabled}
      className="flex flex-col items-start gap-0.5"
    >
      <Checkbox.Content className="flex cursor-pointer flex-row items-center gap-2">
        {/* A visible border: the default unchecked box blends into cards, especially in dark mode. */}
        <Checkbox.Control className="size-5 shrink-0 border-2 border-muted">
          <Checkbox.Indicator />
        </Checkbox.Control>
        <Label className="cursor-pointer">{label}</Label>
      </Checkbox.Content>
      {description && <Description className="pl-8">{description}</Description>}
    </Checkbox>
  );
}

/** A titled group of fields laid out on a grid. */
export function FormSection({
  title,
  description,
  children,
}: {
  title: string;
  description?: string;
  children: ReactNode;
}) {
  return (
    <section className="grid grid-cols-1 gap-x-8 gap-y-4 border-t border-separator py-6 first:border-t-0 first:pt-0 lg:grid-cols-[220px_1fr]">
      <div>
        <h2 className="text-sm font-semibold">{title}</h2>
        {description && <p className="mt-1 text-xs text-muted">{description}</p>}
      </div>
      <div className="grid grid-cols-1 gap-4 md:grid-cols-2">{children}</div>
    </section>
  );
}
