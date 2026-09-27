import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";

export type TeamRole = "admin" | "manager" | "member";

/** A call to the team server's member / folder-sharing API. */
export const teamCall = <T,>(method: "GET" | "PUT" | "POST", path: string, body?: unknown) =>
  invoke<T>("team_admin", { method, path, body: body ?? null });

const CACHE = "hir.teamRole";
const cached = (): TeamRole | null => {
  try {
    const v = localStorage.getItem(CACHE);
    return v === "admin" || v === "manager" || v === "member" ? v : null;
  } catch { return null; }
};

type State = {
  /** null = not on a team (or server unreachable and never seen): everything is local and unrestricted. */
  role: TeamRole | null;
  name: string;
  id: string;
  refresh: () => Promise<void>;
};

export const useTeam = create<State>((set) => ({
  role: cached(),
  name: "",
  id: "",
  refresh: async () => {
    try {
      const me = await teamCall<{ role: TeamRole; name: string; id: string }>("GET", "/me");
      set({ role: me.role, name: me.name, id: me.id });
      try { localStorage.setItem(CACHE, me.role); } catch { /* ignore */ }
    } catch (e) {
      // Only forget the role when sync is off; a dropped connection keeps the last known one.
      if (/sync is not enabled/i.test(String(e))) {
        set({ role: null, name: "", id: "" });
        try { localStorage.removeItem(CACHE); } catch { /* ignore */ }
      }
    }
  },
}));

let started = false;
/** Start refreshing the role in the background (once). */
export function startTeamRole() {
  if (started) return;
  started = true;
  useTeam.getState().refresh();
  setInterval(() => useTeam.getState().refresh(), 60_000);
}

/** Members cannot add, change or delete anything. */
export const canEdit = (role: TeamRole | null) => role !== "member";
export const canShare = (role: TeamRole | null) => role === "admin" || role === "manager";
