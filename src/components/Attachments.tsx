// Attached files: list, open (with a fingerprint check), show in folder, archive,
// and attach new files from disk.

import { Button, Chip, toast, Tooltip } from "@heroui/react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { Archive, ArchiveRestore, File, FileImage, FileText, FolderOpen, Paperclip, X } from "lucide-react";
import { useState } from "react";
import type { Attachment } from "../bindings/Attachment";
import type { AttachmentCategory } from "../bindings/AttachmentCategory";
import { ApiError, api } from "../lib/api";
import { useAuth } from "../lib/auth";
import {
  ATTACHMENT_CATEGORIES,
  attachmentCategoryMeta,
  formatBytes,
  formatDate,
  formatDateTime,
} from "../lib/format";
import { useAsync } from "../lib/useAsync";
import { ConfirmDialog } from "./ConfirmDialog";
import { SelectInput, ToggleSwitch } from "./fields";

function fileIcon(a: Pick<Attachment, "content_type">) {
  if (a.content_type?.startsWith("image/")) return FileImage;
  if (a.content_type === "application/pdf" || a.content_type?.startsWith("text/")) return FileText;
  return File;
}

/** Asks the OS for files to attach. Returns their paths (empty if cancelled). */
export async function pickFiles(title = "Choose files to attach"): Promise<string[]> {
  const picked = await openDialog({ multiple: true, directory: false, title });
  if (!picked) return [];
  return Array.isArray(picked) ? picked : [picked];
}

export function baseName(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}

/** Opens a file, offering to open it anyway if it changed since it was attached. */
function useOpenAttachment() {
  const [changed, setChanged] = useState<{ attachment: Attachment; message: string } | null>(null);

  async function open(a: Attachment, force = false) {
    try {
      await api.openAttachment(a.id, force);
    } catch (e) {
      const err = e instanceof ApiError ? e : new ApiError("database", String(e));
      if (err.kind === "conflict" && !force) setChanged({ attachment: a, message: err.message });
      else toast.danger(err.message, { timeout: 8000 });
    }
  }

  const dialog = (
    <ConfirmDialog
      isOpen={!!changed}
      onOpenChange={(o) => !o && setChanged(null)}
      title="This file has changed"
      tone="accent"
      confirmLabel="Open anyway"
      onConfirm={async () => {
        if (changed) await open(changed.attachment, true);
      }}
    >
      {changed?.message} Its contents no longer match the fingerprint recorded when it was attached.
    </ConfirmDialog>
  );
  return { open, dialog };
}

export function AttachmentsPanel({
  assetId,
  calibrationId,
  readOnly = false,
  emptyText = "No files attached.",
  onChanged,
}: {
  assetId: number;
  /** Show and attach files for one calibration record instead of the whole item. */
  calibrationId?: number;
  /** E.g. the item is archived. */
  readOnly?: boolean;
  emptyText?: string;
  onChanged?: () => void;
}) {
  const { can } = useAuth();
  const canEdit = can("edit_records") && !readOnly;
  const [showArchived, setShowArchived] = useState(false);
  const [category, setCategory] = useState<AttachmentCategory>(calibrationId ? "certificate" : "document");
  const [adding, setAdding] = useState(false);
  const [archiving, setArchiving] = useState<Attachment | null>(null);
  const settings = useAsync(() => api.getSettings(), []);
  const files = useAsync(
    () =>
      api.listAttachments(
        calibrationId
          ? { calibration_id: calibrationId, include_archived: showArchived }
          : { asset_id: assetId, include_archived: showArchived },
      ),
    [assetId, calibrationId, showArchived],
  );
  const { open, dialog: changedDialog } = useOpenAttachment();
  const folderSet = !!settings.data?.attachments_root;

  async function attach() {
    const paths = await pickFiles();
    if (paths.length === 0) return;
    setAdding(true);
    try {
      const saved = await api.addAttachments({ assetId, calibrationId, category, paths });
      toast.success(saved.length === 1 ? `Attached ${saved[0].file_name}` : `Attached ${saved.length} files`);
    } catch (e) {
      toast.danger(e instanceof ApiError ? e.message : String(e), { timeout: 8000 });
    } finally {
      setAdding(false);
      files.reload();
      onChanged?.();
    }
  }

  async function reveal(a: Attachment) {
    try {
      await api.revealAttachment(a.id);
    } catch (e) {
      toast.danger(e instanceof ApiError ? e.message : String(e), { timeout: 8000 });
    }
  }

  const list = files.data ?? [];
  return (
    <div className="flex flex-col gap-3">
      {canEdit && (
        <div className="flex flex-wrap items-end justify-between gap-3">
          <div className="flex items-end gap-2">
            <SelectInput<AttachmentCategory>
              label="Type"
              value={category}
              onChange={(c) => c && setCategory(c)}
              options={ATTACHMENT_CATEGORIES.map((c) => ({ id: c, label: attachmentCategoryMeta[c].label }))}
              allowEmpty={false}
              className="w-44"
              isDisabled={!folderSet}
            />
            <Button variant="secondary" isPending={adding} isDisabled={!folderSet} onPress={() => void attach()}>
              <Paperclip className="size-4" /> Attach files
            </Button>
          </div>
          <ToggleSwitch label="Show archived" isSelected={showArchived} onChange={setShowArchived} />
        </div>
      )}
      {canEdit && settings.data && !folderSet && (
        <p className="text-xs text-warning">
          An administrator needs to choose the attachments folder in Settings before files can be attached.
        </p>
      )}

      {files.error && <p className="text-sm text-danger">{files.error.message}</p>}
      {files.data && list.length === 0 && <p className="py-2 text-sm text-muted">{emptyText}</p>}

      <ul className="flex flex-col divide-y divide-separator">
        {list.map((a) => {
          const Icon = fileIcon(a);
          const cat = attachmentCategoryMeta[a.category];
          return (
            <li key={a.id} className={`flex items-center gap-3 py-2.5 ${a.is_archived ? "opacity-55" : ""}`}>
              <Icon className="size-5 shrink-0 text-muted" />
              <div className="min-w-0 flex-1">
                <button
                  type="button"
                  className="max-w-full truncate text-left text-sm font-medium text-accent hover:underline"
                  onClick={() => void open(a)}
                  title={`Open ${a.file_name}`}
                >
                  {a.file_name}
                </button>
                <div className="flex flex-wrap items-center gap-x-2 gap-y-0.5 text-xs text-muted">
                  <Chip size="sm" variant="soft" color={cat.color}>
                    {cat.label}
                  </Chip>
                  {!calibrationId && a.calibration_date && <span>Calibration of {formatDate(a.calibration_date)}</span>}
                  <span>{formatBytes(a.size_bytes)}</span>
                  <span>
                    Added by {a.uploaded_by}, {formatDateTime(a.created_at)}
                  </span>
                  {a.is_archived && <span>(archived)</span>}
                </div>
                {a.description && <div className="text-xs">{a.description}</div>}
              </div>
              <Tooltip delay={300}>
                <Button isIconOnly size="sm" variant="ghost" aria-label="Show in folder" onPress={() => void reveal(a)}>
                  <FolderOpen className="size-4" />
                </Button>
                <Tooltip.Content>Show in folder</Tooltip.Content>
              </Tooltip>
              {canEdit && (
                <Tooltip delay={300}>
                  <Button
                    isIconOnly
                    size="sm"
                    variant="ghost"
                    aria-label={a.is_archived ? "Restore file" : "Archive file"}
                    onPress={() => setArchiving(a)}
                  >
                    {a.is_archived ? <ArchiveRestore className="size-4" /> : <Archive className="size-4" />}
                  </Button>
                  <Tooltip.Content>{a.is_archived ? "Restore" : "Archive"}</Tooltip.Content>
                </Tooltip>
              )}
            </li>
          );
        })}
      </ul>

      {changedDialog}
      <ConfirmDialog
        isOpen={!!archiving}
        onOpenChange={(o) => !o && setArchiving(null)}
        title={archiving?.is_archived ? `Restore ${archiving?.file_name}?` : `Archive ${archiving?.file_name}?`}
        confirmLabel={archiving?.is_archived ? "Restore" : "Archive"}
        tone={archiving?.is_archived ? "accent" : "danger"}
        askReason
        onConfirm={async (reason) => {
          if (!archiving) return;
          await api.setAttachmentArchived(archiving.id, archiving.row_version, !archiving.is_archived, reason);
          toast.success(archiving.is_archived ? "File restored" : "File archived");
          files.reload();
          onChanged?.();
        }}
      >
        {archiving?.is_archived
          ? "The file will be listed again."
          : "The file is hidden from the list but stays in the attachments folder and the audit trail. You can restore it later."}
      </ConfirmDialog>
    </div>
  );
}

/** Files chosen for a calibration record that hasn't been saved yet. */
export function PendingFiles({ paths, onChange }: { paths: string[]; onChange: (paths: string[]) => void }) {
  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center justify-between">
        <span className="text-sm font-medium">Certificates and files</span>
        <Button
          size="sm"
          variant="secondary"
          onPress={async () => {
            const picked = await pickFiles("Choose certificates to attach");
            if (picked.length) onChange([...paths, ...picked.filter((p) => !paths.includes(p))]);
          }}
        >
          <Paperclip className="size-4" /> Choose files
        </Button>
      </div>
      {paths.length === 0 ? (
        <p className="text-xs text-muted">Optional. Files are attached as certificates when the record is saved.</p>
      ) : (
        <ul className="flex flex-col gap-1">
          {paths.map((p) => (
            <li key={p} className="flex items-center gap-2 rounded-lg bg-surface-secondary px-3 py-1.5 text-sm">
              <FileText className="size-4 shrink-0 text-muted" />
              <span className="min-w-0 flex-1 truncate" title={p}>
                {baseName(p)}
              </span>
              <button
                type="button"
                aria-label={`Remove ${baseName(p)}`}
                className="grid size-6 place-items-center rounded-md text-muted hover:bg-default hover:text-foreground"
                onClick={() => onChange(paths.filter((x) => x !== p))}
              >
                <X className="size-3.5" />
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
