import { create } from "zustand";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { invoke } from "@tauri-apps/api/core";

type AppUpdateState = {
  status: "idle" | "checking" | "available" | "saving" | "downloading" | "ready" | "error";
  /** Profiles still being saved to the team while an install waits for them. */
  saving: number;
  update: Update | null;
  version: string | null;
  progress: number;
  error: string | null;
  check: () => Promise<void>;
  install: () => Promise<void>;
};

/// Hir-Login's own update channel (Cloudflare Worker `hirlogin-cap-nhat`, one
/// manifest per platform — see scripts/phat-hanh/) — unrelated to
/// `launcher_update_check`, which is the ShardX *engine's* remote version gate.
export const useAppUpdate = create<AppUpdateState>((set, get) => ({
  status: "idle",
  update: null,
  version: null,
  progress: 0,
  error: null,
  saving: 0,

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
    try {
      // The installer ends the app on the spot. A profile whose close is still being uploaded
      // would lose that upload: wait for it (at most two minutes). Browsers that are open stay
      // open — the new version takes them back when it starts.
      const until = Date.now() + 120_000;
      for (;;) {
        const n = await invoke<number>("sync_working").catch(() => 0);
        set({ saving: n });
        if (n === 0 || Date.now() > until) break;
        set({ status: "saving" });
        await new Promise((r) => setTimeout(r, 1000));
      }
      set({ status: "downloading", progress: 0 });
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
