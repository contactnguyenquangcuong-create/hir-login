import { create } from "zustand";
import { hirsAllowed, hirsFolders, hirsFolderSet, type HirsLevel } from "../model/api";

/** HirS as one more person to share a folder with: "use" (open, close) or "manage" (also add,
 *  edit, delete). It only sees the folders shared here, and only while "Cho phép HirS kết nối"
 *  (`allowed`) is on. */
type HirsShareState = {
  allowed: boolean;
  access: Record<string, HirsLevel>;
  load: () => Promise<void>;
  set: (folder: string, level: HirsLevel | "none") => Promise<void>;
};

export const useHirsShare = create<HirsShareState>((set) => ({
  allowed: false,
  access: {},
  load: async () => {
    try { set({ access: await hirsFolders(), allowed: await hirsAllowed() }); } catch { /* keeps the last known */ }
  },
  set: async (folder, level) => {
    set({ access: await hirsFolderSet(folder, level) });
  },
}));
