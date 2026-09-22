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

  if (!folderModal) return null;

  const moving = folderModal.profileId
    ? profiles.find((p) => p.id === folderModal.profileId) ?? null
    : null;
  // "move" mode: pick from other folders; "create" mode: just the input.
  const pickable = moving ? folders.filter((f) => f !== moving.folder) : [];

  const assign = (f: string) => {
    if (folderModal.profileId) setProfileFolder(folderModal.profileId, f);
    else rememberFolder(f);
    setFolder(f);
    setFolderModal(null);
  };

  return (
    <FolderModal
      mode={folderModal.profileId ? "move" : "create"}
      existing={pickable}
      onPick={assign}
      onCreate={(name) => { const f = name.trim(); if (f) assign(f); }}
      onClose={() => setFolderModal(null)}
    />
  );
}
