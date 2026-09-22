import { create } from "zustand";
import { open } from "@tauri-apps/plugin-dialog";
import { openPath } from "@tauri-apps/plugin-opener";
import { toast } from "../../../shared/lib/toast";
import { confirmModal } from "../../../shared/lib/confirm";
import { readTextFile } from "../../../shared/lib/utils";
import { profileCreateFromTemplate } from "../../profile/model/api";
import { FingerprintEntry } from "../model/types";
import { fingerprintList, fingerprintDelete, fingerprintImport, fingerprintDir } from "../model/api";
import { storeBus } from "../../../shared/lib/storeBus";
import { t } from "../../../shared/i18n";

export type FingerprintStore = {
    status: "idle" | "loading" | "ready" | "error";
    error: string | null;

    items: FingerprintEntry[];
    importerOpen: boolean;

    init: () => Promise<void>;
    reload: () => Promise<void>;
    setImporterOpen: (open: boolean) => void;

    useTemplate: (id: string) => Promise<void>;
    remove: (id: string) => Promise<void>;
    importJsonFile: () => Promise<void>;
    openLibraryFolder: () => Promise<void>;
};

export const useFingerprint = create<FingerprintStore>((set, get) => ({
    status: "idle",
    error: null,
    items: new Array<FingerprintEntry>(),
    importerOpen: false,

    init: async () => {
        if (get().status === "loading" || get().status === "ready") return;
        set({ status: "loading" });
        try {
            set({ items: await fingerprintList(), status: "ready" });
        } catch (e) {
            set({ status: "error", error: (e as Error).message });
            toast.err(String(e));
        }
    },
    reload: async () => {
        try {
            set({ items: await fingerprintList() });
        } catch (e) { toast.err(String(e)); }
    },
    setImporterOpen: (importerOpen) => set({ importerOpen }),

    // Spawn a new profile seeded from this library fingerprint.
    useTemplate: async (id) => {
        try {
            const meta = await profileCreateFromTemplate(id);
            // The one action that crosses into the profile store: without this the new
            // profile is on disk and nowhere on screen. Via the bus — a direct import cycles.
            storeBus.emit("profiles");
            toast.ok(t("useFingerprint.createdFromTemplate", { name: meta.name }));
        } catch (e) { toast.err(String(e)); }
    },
    remove: async (id) => {
        if ((await confirmModal({ title: t("useFingerprint.removeTitle"), message: t("useFingerprint.removeMessage"), danger: true })) !== true) return;
        try {
            await fingerprintDelete(id);
            toast.ok(t("useFingerprint.removed"));
            get().reload();
        } catch (e) { toast.err(String(e)); }
    },
    importJsonFile: async () => {
        const path = await open({
            multiple: false,
            directory: false,
            title: t("useFingerprint.pickJsonTitle"),
            filters: [{ name: "JSON", extensions: ["json"] }],
        });
        if (typeof path !== "string") return;
        try {
            const txt = await readTextFile(path);
            const e = await fingerprintImport(txt, null);
            toast.ok(t("useFingerprint.imported", { label: e.label }));
            get().reload();
        } catch (e) { toast.err(String(e)); }
    },
    openLibraryFolder: async () => {
        try {
            // Reveal folder via tauri-plugin-opener.
            await openPath(await fingerprintDir());
        } catch (e) { toast.err(String(e)); }
    },
}));
