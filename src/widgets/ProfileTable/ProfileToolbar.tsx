import { useTeam, canEdit } from "../../shared/model/teamRole";
import {
  BulkActionsBar,
  BulkCreateButton,
  ImportProfilesButton,
  ExportProfilesButton,
  FromTemplateButton,
  NewProfileButton,
  ProfileFilterBar,
} from "../../features/manage-profiles";

export function ProfileToolbar() {
  const role = useTeam((s) => s.role);
  const configured = useTeam((s) => s.configured);
  const edit = canEdit(role, configured);
  return (
    <div className="flex max-w-full flex-wrap items-center justify-end gap-2">
      <BulkActionsBar />
      <ProfileFilterBar />
      {edit && <BulkCreateButton />}
      {edit && <ImportProfilesButton />}
      {edit && <ExportProfilesButton />}
      {edit && <FromTemplateButton />}
      {edit && <NewProfileButton />}
    </div>
  );
}
