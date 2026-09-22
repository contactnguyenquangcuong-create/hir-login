import { useEffect, useRef, useState } from "react";
import { Button, DialogModal, Input } from "@proxyshard/shardx-ui-kit";
import { FolderIcon } from "../../../shared/icons";
import { useT } from "../../../shared/i18n";

/// Folder picker/creator modal (replaces native prompt). mode: "create" | "move".
export function FolderModal({
  mode, existing, onPick, onCreate, onClose,
}: {
  mode: "create" | "move";
  existing: string[];
  onPick: (folder: string) => void;
  onCreate: (name: string) => void;
  onClose: () => void;
}) {
  const t = useT();
  const [name, setName] = useState("");
  const ref = useRef<HTMLInputElement>(null);
  useEffect(() => { ref.current?.focus(); }, []);
  const trimmed = name.trim();
  const dup = existing.includes(trimmed);
  const create = () => { if (trimmed && !dup) onCreate(trimmed); };
  const showList = mode === "move" && existing.length > 0;
  return (
    <DialogModal
      open
      onClose={onClose}
      icon={<FolderIcon className="size-5" />}
      title={mode === "move" ? t("folderModal.moveTitle") : t("folderModal.createTitle")}
      confirmLabel={showList ? t("folderModal.createAndMove") : t("folderModal.create")}
      onConfirm={create}
      isDisabled={!trimmed || dup}
      cancelLabel={t("folderModal.cancel")}
      onCancel={onClose}
    >
      <div className="flex flex-col gap-3 py-4">
        {showList && (
          <>
            <span className="text-label-xs text-text-sub-600">{t("folderModal.existingFolders")}</span>
            <div className="flex max-h-[220px] flex-col gap-1 overflow-y-auto">
              {existing.map((f) => (
                <Button
                  key={f}
                  variant="neutral"
                  mode="stroke"
                  size="small"
                  className="w-full justify-start"
                  leftIcon={<FolderIcon className="size-4 text-icon-soft-400" />}
                  onClick={() => onPick(f)}
                >
                  {f}
                </Button>
              ))}
            </div>
            <div className="my-0.5 flex items-center gap-2.5 text-paragraph-xs text-text-soft-400 [&::before]:h-px [&::before]:flex-1 [&::before]:bg-stroke-soft-200 [&::before]:content-[''] [&::after]:h-px [&::after]:flex-1 [&::after]:bg-stroke-soft-200 [&::after]:content-['']">
            <span>{t("folderModal.orCreateNew")}</span>
            </div>
          </>
        )}
        <Input
          ref={ref}
          label={showList ? t("folderModal.newFolderNameLabel") : t("folderModal.folderNameLabel")}
          inputSize="small"
          value={name}
          placeholder={t("folderModal.namePlaceholder")}
          error={dup ? t("folderModal.duplicateError", { name: trimmed }) : undefined}
          onChange={(e) => setName(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") create();
            if (e.key === "Escape") onClose();
          }}
        />
      </div>
    </DialogModal>
  );
}
