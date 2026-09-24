import {
  BulkActionsBar,
  ImportProfilesButton,
  ExportProfilesButton,
  FromTemplateButton,
  NewProfileButton,
  ProfileFilterBar,
} from "../../features/manage-profiles";

export function ProfileToolbar() {
  return (
    <div className="flex items-center flex-none gap-2">
      <BulkActionsBar />
      <ProfileFilterBar />
      <ImportProfilesButton />
      <ExportProfilesButton />
      <FromTemplateButton />
      <NewProfileButton />
    </div>
  );
}
