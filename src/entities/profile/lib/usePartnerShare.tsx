import { create } from "zustand";
import { partnerFolders, partnerFolderSet, partnersAllowed, type PartnerKey, type ShareLevel } from "../model/api";

/** The partner tools that may borrow profiles, in the order they are listed. */
export const PARTNERS: { key: PartnerKey; name: string }[] = [
  { key: "hirs", name: "HirS" },
  { key: "hirf", name: "Hir-F" },
];

type Access = Record<string, ShareLevel>;

/** A partner tool as one more person to share a folder with: "use" (open, close) or "manage"
 *  (also add, edit, delete). It only sees the folders shared with it, and only while its
 *  "Cho phép … kết nối" switch (`allowed`) is on. */
type PartnerShareState = {
  allowed: Record<PartnerKey, boolean>;
  access: Record<PartnerKey, Access>;
  load: () => Promise<void>;
  set: (partner: PartnerKey, folder: string, level: ShareLevel | "none") => Promise<void>;
};

export const usePartnerShare = create<PartnerShareState>((set, get) => ({
  allowed: { hirs: false, hirf: false },
  access: { hirs: {}, hirf: {} },
  load: async () => {
    try {
      const [allowed, hirs, hirf] = await Promise.all([partnersAllowed(), partnerFolders("hirs"), partnerFolders("hirf")]);
      set({ allowed, access: { hirs, hirf } });
    } catch { /* keeps the last known */ }
  },
  set: async (partner, folder, level) => {
    const next = await partnerFolderSet(partner, folder, level);
    set({ access: { ...get().access, [partner]: next } });
  },
}));
