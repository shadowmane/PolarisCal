import { Alert, Button, Modal, toast } from "@heroui/react";
import { useEffect, useState } from "react";
import type { LookupInput } from "../bindings/LookupInput";
import type { LookupItem } from "../bindings/LookupItem";
import type { LookupKind } from "../bindings/LookupKind";
import { ApiError, api } from "../lib/api";
import { lookupKindMeta } from "../lib/format";
import { emptyLookupInput, type FieldErrors, fieldErrors, lookupToInput } from "../lib/forms";
import { Trash2 } from "lucide-react";
import { ConfirmDialog } from "./ConfirmDialog";
import { NumberInput, SwitchInput, TextAreaInput, TextInput } from "./fields";

/** Add or edit one entry in a pick list. */
export function LookupDialog({
  kind,
  item,
  nextSortOrder,
  isOpen,
  onOpenChange,
  onSaved,
  onDeleted,
}: {
  kind: LookupKind;
  item?: LookupItem;
  nextSortOrder: number;
  isOpen: boolean;
  onOpenChange: (open: boolean) => void;
  onSaved: (item: LookupItem) => void;
  /** When given, editing an existing entry offers Delete (refused if the entry is in use). */
  onDeleted?: () => void;
}) {
  const meta = lookupKindMeta[kind];
  const [input, setInput] = useState<LookupInput>(emptyLookupInput(kind));
  const [errors, setErrors] = useState<FieldErrors>({});
  const [formError, setFormError] = useState<string>();
  const [saving, setSaving] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);

  useEffect(() => {
    if (!isOpen) return;
    setInput(item ? lookupToInput(item) : emptyLookupInput(kind, nextSortOrder));
    setErrors({});
    setFormError(undefined);
  }, [isOpen, item, kind, nextSortOrder]);

  function set<K extends keyof LookupInput>(key: K, value: LookupInput[K]) {
    setInput((i) => ({ ...i, [key]: value }));
    setErrors(({ [key]: _cleared, ...rest }) => rest);
  }

  async function save() {
    setFormError(undefined);
    if (!input.name.trim()) {
      setErrors({ name: "Name is required." });
      return;
    }
    setSaving(true);
    try {
      const payload = { ...input, description: input.description?.trim() || null };
      const saved = item
        ? await api.updateLookup(kind, item.id, payload)
        : await api.createLookup(kind, payload);
      toast.success(item ? `Saved ${saved.name}` : `Added ${saved.name}`);
      onSaved(saved);
      onOpenChange(false);
    } catch (e) {
      const err = e instanceof ApiError ? e : new ApiError("database", String(e));
      if (err.kind === "duplicate") setErrors({ name: err.message });
      else {
        const fields = fieldErrors(err);
        setErrors(fields);
        if (Object.keys(fields).length === 0) setFormError(err.message);
      }
    } finally {
      setSaving(false);
    }
  }

  return (
    <Modal>
      <Modal.Backdrop isOpen={isOpen} onOpenChange={onOpenChange} isDismissable={!saving}>
        <Modal.Container size="md">
          <Modal.Dialog>
            <Modal.CloseTrigger />
            <Modal.Header>
              <Modal.Heading>{item ? `Edit ${meta.singular.toLowerCase()}` : `Add ${meta.singular.toLowerCase()}`}</Modal.Heading>
            </Modal.Header>
            <Modal.Body>
              <form
                id="lookup-form"
                noValidate
                className="flex flex-col gap-4 p-1"
                onSubmit={(e) => {
                  e.preventDefault();
                  void save();
                }}
              >
                {formError && (
                  <Alert status="danger">
                    <Alert.Indicator />
                    <Alert.Content>
                      <Alert.Title>Couldn't save</Alert.Title>
                      <Alert.Description>{formError}</Alert.Description>
                    </Alert.Content>
                  </Alert>
                )}
                <TextInput
                  label="Name"
                  value={input.name}
                  onChange={(v) => set("name", v)}
                  error={errors.name}
                  isRequired
                  autoFocus
                  maxLength={100}
                />
                <TextAreaInput
                  label="Description"
                  value={input.description ?? ""}
                  onChange={(v) => set("description", v)}
                  rows={2}
                />
                <NumberInput
                  label="Sort order"
                  value={input.sort_order}
                  onChange={(v) => set("sort_order", Math.trunc(v ?? 0))}
                  step={1}
                  description="Lower numbers appear first in dropdowns."
                />
                {kind === "provider" && (
                  <SwitchInput
                    label="Internal"
                    description="An in-house technician or lab rather than an outside vendor."
                    isSelected={!!input.is_internal}
                    onChange={(v) => set("is_internal", v)}
                  />
                )}
                {kind === "status" && (
                  <SwitchInput
                    label="Track due dates"
                    description="Items in this status appear in overdue and due-soon tracking."
                    isSelected={!!input.tracks_due}
                    onChange={(v) => set("tracks_due", v)}
                  />
                )}
                <SwitchInput
                  label="Active"
                  description="Inactive entries stay on existing records but can't be chosen for new ones."
                  isSelected={input.is_active}
                  onChange={(v) => set("is_active", v)}
                />
              </form>
            </Modal.Body>
            <Modal.Footer>
              {item && onDeleted && (
                <Button variant="danger-soft" className="mr-auto" isDisabled={saving} onPress={() => setConfirmDelete(true)}>
                  <Trash2 className="size-4" /> Delete
                </Button>
              )}
              <Button slot="close" variant="tertiary" isDisabled={saving}>
                Cancel
              </Button>
              <Button type="submit" form="lookup-form" isPending={saving}>
                {item ? "Save" : "Add"}
              </Button>
            </Modal.Footer>
          </Modal.Dialog>
        </Modal.Container>
      </Modal.Backdrop>
      {item && onDeleted && (
        <ConfirmDialog
          isOpen={confirmDelete}
          onOpenChange={setConfirmDelete}
          title={`Delete ${item.name}?`}
          confirmLabel="Delete"
          onConfirm={async () => {
            await api.deleteLookup(kind, item.id);
            toast.success(`Deleted ${item.name}`);
            setConfirmDelete(false);
            onOpenChange(false);
            onDeleted();
          }}
        >
          This removes it from the {meta.label.toLowerCase()} list for good. Entries that items or calibration
          records already use can't be deleted; make them inactive instead.
        </ConfirmDialog>
      )}
    </Modal>
  );
}
