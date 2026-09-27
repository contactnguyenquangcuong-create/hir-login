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
  const edit = canEdit(role);
  return (
    <div className="flex items-center flex-none gap-2">
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
