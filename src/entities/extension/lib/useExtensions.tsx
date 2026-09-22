import { create } from "zustand";
import { open } from "@tauri-apps/plugin-dialog";
import { toast } from "../../../shared/lib/toast";
import { confirmModal } from "../../../shared/lib/confirm";
import { storeBus } from "../../../shared/lib/storeBus";
import { t } from "../../../shared/i18n";
import { extensionDelete, extensionImport, extensionImportUrl, extensionList } from "../model/api";
import type { ExtensionEntry } from "../model/types";

export type ExtensionStore = {
  status: "idle" | "loading" | "ready" | "error";
  items: ExtensionEntry[];
  busy: boolean;
  search: string;
  /// The "paste a link" dialog.
  linkOpen: boolean;

  init: () => Promise<void>;
  reload: () => Promise<void>;
  setSearch: (q: string) => void;
  setLinkOpen: (open: boolean) => void;
  importUrl: (url: string) => Promise<void>;
  importFiles: () => Promise<void>;
  importFolder: () => Promise<void>;
  remove: (e: ExtensionEntry) => Promise<void>;
};

export const useExtensions = create<ExtensionStore>((set, get) => ({
  status: "idle",
  items: [],
  busy: false,
  search: "",
  linkOpen: false,

  init: async () => {
    if (get().status === "loading" || get().status === "ready") return;
    set({ status: "loading" });
    try {
      set({ items: await extensionList(), status: "ready" });
    } catch (e) {
      set({ status: "error" });
      toast.err(String(e));
    }
  },
  reload: async () => {
    try {
      set({ items: await extensionList() });
      storeBus.emit("extensions");
    } catch (e) { toast.err(String(e)); }
  },
  setSearch: (search) => set({ search }),
  setLinkOpen: (linkOpen) => set({ linkOpen }),

  importUrl: async (url) => {
    if (!url.trim()) return;
    set({ busy: true });
    try {
      const added = await extensionImportUrl(url.trim());
      await get().reload();
      set({ linkOpen: false });
      toast.ok(t("useExtensions.addedNamed", { name: added.name }));
    } catch (e) { toast.err(t("useExtensions.downloadFailed", { error: String(e) })); }
    finally { set({ busy: false }); }
  },

  importFiles: async () => {
    const picked = await open({
      multiple: true,
      title: t("useExtensions.pickFilesTitle"),
      filters: [{ name: "Extension", extensions: ["crx", "zip"] }],
    });
    const paths = Array.isArray(picked) ? picked : picked ? [picked] : [];
    if (paths.length === 0) return;
    set({ busy: true });
    try {
      const added = await extensionImport(paths as string[]);
      await get().reload();
      toast.ok(added.length === 1
        ? t("useExtensions.addedCountOne", { n: added.length })
        : t("useExtensions.addedCountMany", { n: added.length }));
    } catch (e) { toast.err(t("useExtensions.importFilesFailed", { error: String(e) })); }
    finally { set({ busy: false }); }
  },

  importFolder: async () => {
    const dir = await open({ directory: true, title: t("useExtensions.pickFolderTitle") });
    if (typeof dir !== "string") return;
    set({ busy: true });
    try {
      const added = await extensionImport([dir]);
      await get().reload();
      toast.ok(added.length > 0
        ? t("useExtensions.addedFolderNamed", { name: added[0].name })
        : t("useExtensions.nothingAdded"));
    } catch (e) { toast.err(t("useExtensions.importFolderFailed", { error: String(e) })); }
    finally { set({ busy: false }); }
  },

  remove: async (e) => {
    const ok = await confirmModal({
      title: t("useExtensions.removeTitle"),
      message: t("useExtensions.removeMessage", { name: e.name }),
      danger: true,
    });
    if (ok !== true) return;
    try {
      await extensionDelete(e.id);
      await get().reload();
      toast.ok(t("useExtensions.removed"));
    } catch (err) { toast.err(String(err)); }
  },
}));
