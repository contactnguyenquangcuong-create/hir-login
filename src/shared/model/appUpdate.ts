import { create } from "zustand";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

type AppUpdateState = {
  status: "idle" | "checking" | "available" | "downloading" | "ready" | "error";
  update: Update | null;
  version: string | null;
  progress: number;
  error: string | null;
  check: () => Promise<void>;
  install: () => Promise<void>;
};

/// Hir-Login's own update channel (Supabase Storage manifest) — unrelated to
/// `launcher_update_check`, which is the ShardX *engine's* remote version gate.
export const useAppUpdate = create<AppUpdateState>((set, get) => ({
  status: "idle",
  update: null,
  version: null,
  progress: 0,
  error: null,

  check: async () => {
    if (get().status === "checking" || get().status === "downloading") return;
    set({ status: "checking", error: null });
    try {
      const update = await check();
      if (update) {
        set({ status: "available", update, version: update.version });
      } else {
        set({ status: "idle", update: null });
      }
    } catch (e) {
      set({ status: "error", error: String(e) });
    }
  },

  install: async () => {
    const { update } = get();
    if (!update) return;
    set({ status: "downloading", progress: 0 });
    try {
      let total = 0;
      let downloaded = 0;
      await update.downloadAndInstall((event) => {
        if (event.event === "Started") {
          total = event.data.contentLength ?? 0;
        } else if (event.event === "Progress") {
          downloaded += event.data.chunkLength;
          set({ progress: total > 0 ? downloaded / total : 0 });
        }
      });
      set({ status: "ready" });
      // Windows' installer already relaunches the app itself; macOS/Linux
      // need this explicit call.
      await relaunch();
    } catch (e) {
      set({ status: "error", error: String(e) });
    }
  },
}));
