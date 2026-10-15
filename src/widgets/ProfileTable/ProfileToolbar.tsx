import { Button } from "@proxyshard/shardx-ui-kit";
import { useTeam, canEdit } from "../../shared/model/teamRole";
import { useContextMenu } from "../../shared/hooks/useContextMenu";
import { useT } from "../../shared/i18n";
import { useProfile } from "../../entities/profile";
import { ChevronDownIcon } from "../../shared/icons";
import {
  BulkCreateButton,
  NewProfileButton,
  ProfileFilterBar,
} from "../../features/manage-profiles";

/// The occasional actions — moving profiles in and out as folders, starting from a template — in one
/// menu, so the row keeps to what is used all the time and never wraps.
function ToolsMenu() {
  const t = useT();
  const ctx = useContextMenu();
  const importFolder = useProfile((s) => s.importProfilesFromFolder);
  const exportFolder = useProfile((s) => s.exportProfilesToFolder);
  const selectedCount = useProfile((s) => s.selected.size);
  const setTemplatePickerOpen = useProfile((s) => s.setTemplatePickerOpen);
  return (
    <>
      <Button
        variant="neutral"
        mode="stroke"
        size="small"
        rightIcon={<ChevronDownIcon className="size-4" />}
        onClick={(e) => {
          // The menu closes on any click that reaches the window; this one opened it, so it
          // must not get there (it did, and the menu closed the moment it appeared).
          e.stopPropagation();
          // Opens under the button rather than at the pointer.
          const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
          ctx.open({ preventDefault: () => {}, clientX: r.left, clientY: r.bottom + 4 } as unknown as React.MouseEvent, [
            { label: t("importProfilesButton.label"), onClick: importFolder },
            {
              label: selectedCount > 0
                ? t("profileToolbar.exportSelected", { n: selectedCount })
                : t("profileToolbar.exportAll"),
              onClick: exportFolder,
            },
            { sep: true, label: "", onClick: () => {} },
            { label: t("fromTemplateButton.label"), onClick: () => setTemplatePickerOpen(true) },
          ]);
        }}
      >
        {t("profileToolbar.tools")}
      </Button>
      {ctx.node}
    </>
  );
}

export function ProfileToolbar() {
  const role = useTeam((s) => s.role);
  const configured = useTeam((s) => s.configured);
  const edit = canEdit(role, configured);
  return (
    <div className="flex max-w-full flex-wrap items-center justify-end gap-2">
      <ProfileFilterBar />
      {edit && <ToolsMenu />}
      {edit && <BulkCreateButton />}
      {edit && <NewProfileButton />}
    </div>
  );
}
