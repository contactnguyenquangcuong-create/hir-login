import { LibraryFolderButton, ImportFromFileButton, PasteJsonButton } from "../../features/manage-fingerprints";

export function FingerprintToolbar() {
  return (
    <div className="flex items-center flex-none gap-2">
      <LibraryFolderButton />
      <ImportFromFileButton />
      <PasteJsonButton />
    </div>
  );
}
