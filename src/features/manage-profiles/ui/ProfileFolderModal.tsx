import { useProfile, useFolders } from "../../../entities/profile";
import { FolderModal } from "./FolderModal";

/// Store-gated FolderModal handling both "create" (no profileId) and "move"
/// (with profileId) flows. Renders nothing while closed.
export function ProfileFolderModal() {
  const folderModal = useProfile((s) => s.folderModal);
  const profiles = useProfile((s) => s.profiles);
  const folders = useFolders();
  const setFolder = useProfile((s) => s.setFolder);
  const setFolderModal = useProfile((s) => s.setFolderModal);
  const setProfileFolder = useProfile((s) => s.setProfileFolder);
  const rememberFolder = useProfile((s) => s.rememberFolder);
  const bulkMoveToFolder = useProfile((s) => s.bulkMoveToFolder);
  const clearSelected = useProfile((s) => s.clearSelected);

  if (!folderModal) return null;

  const many = folderModal.profileIds;
  const moving = folderModal.profileId
    ? profiles.find((p) => p.id === folderModal.profileId) ?? null
    : null;
  // "move" mode: pick from other folders; "create" mode: just the input. For a selection, any folder.
  const pickable = many ? folders : moving ? folders.filter((f) => f !== moving.folder) : [];

  const assign = (f: string) => {
    if (many) {
      void bulkMoveToFolder(many, f).then(() => clearSelected());
    } else if (folderModal.profileId) setProfileFolder(folderModal.profileId, f);
    else rememberFolder(f);
    setFolder(f);
    setFolderModal(null);
  };

  return (
    <FolderModal
      mode={many || folderModal.profileId ? "move" : "create"}
      existing={pickable}
      onPick={assign}
      onCreate={(name) => { const f = name.trim(); if (f) assign(f); }}
      onClose={() => setFolderModal(null)}
    />
  );
}
