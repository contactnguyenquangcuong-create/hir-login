import { create } from "zustand";
import { hirsAllowed, hirsFolders, hirsFolderSet } from "../model/api";

/** HirS as one more person to share a folder with: it only sees and edits profiles in the
 *  folders shared here, and only while "Cho phép HirS kết nối" (`allowed`) is on. */
type HirsShareState = {
  allowed: boolean;
  folders: string[];
  load: () => Promise<void>;
  set: (folder: string, shared: boolean) => Promise<void>;
};

export const useHirsShare = create<HirsShareState>((set) => ({
  allowed: false,
  folders: [],
  load: async () => {
    try { set({ folders: await hirsFolders(), allowed: await hirsAllowed() }); } catch { /* keeps the last known */ }
  },
  set: async (folder, shared) => {
    set({ folders: await hirsFolderSet(folder, shared) });
  },
}));
