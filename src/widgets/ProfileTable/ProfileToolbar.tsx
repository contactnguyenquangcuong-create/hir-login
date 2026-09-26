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
  return (
    <div className="flex items-center flex-none gap-2">
      <BulkActionsBar />
      <ProfileFilterBar />
      <BulkCreateButton />
      <ImportProfilesButton />
      <ExportProfilesButton />
      <FromTemplateButton />
      <NewProfileButton />
    </div>
  );
}
