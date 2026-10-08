import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { settingsGet } from "../../entities/settings";
import { toast, useToastStore } from "./toast";

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
  /** Whether this machine has team sync switched on at all — read straight from
   *  local settings, no network round trip, so it is never flaky. Null only for
   *  the instant before the very first read completes. Everything below this
   *  reflects the last successful call to the team server, which — unlike this
   *  field — can lag behind or fail for a moment without meaning "not on a team".
   */
  configured: boolean | null;
  /** null = role not yet confirmed for this session (could still be any role —
   *  never treat this the same as "no team", see `canEdit`/`canShare`). */
  role: TeamRole | null;
  name: string;
  id: string;
  /** True only for whoever holds the server's own token — never a named "admin"
   *  member. Only they may rank (promote/demote/disable/delete) another admin. */
  isServerAdmin: boolean;
  /** Folder names this person can see on the server; null until known. */
  folderNames: string[] | null;
  refresh: () => Promise<void>;
};

export const useTeam = create<State>((set, get) => ({
  configured: null,
  role: cached(),
  name: "",
  id: "",
  isServerAdmin: false,
  folderNames: null,
  refresh: async () => {
    // Ground truth for "is a team even set up here" — local, instant, never flaky.
    try {
      const s = await settingsGet();
      set({ configured: !!(s.sync?.enabled && s.sync?.server_url && s.sync?.token) });
    } catch { /* keep whatever we knew */ }

    if (get().configured === false) {
      set({ role: null, name: "", id: "", isServerAdmin: false, folderNames: null });
      try { localStorage.removeItem(CACHE); } catch { /* ignore */ }
      return;
    }

    try {
      const me = await teamCall<{ role: TeamRole; name: string; id: string; isServerAdmin: boolean; folders: Record<string, string> | null }>("GET", "/me");
      set({ role: me.role, name: me.name, id: me.id, isServerAdmin: me.isServerAdmin });
      // Folders that were taken away (or deleted from above) must disappear here too.
      try {
        const names = me.role === "member"
          ? Object.keys(me.folders ?? {})
          : (await teamCall<{ folders: { name: string }[] }>("GET", "/admin/folders")).folders.map((f) => f.name);
        set({ folderNames: names });
      } catch { /* keep what we knew */ }
      try { localStorage.setItem(CACHE, me.role); } catch { /* ignore */ }
    } catch (e) {
      // Configured (we just confirmed it above) but this one call to the server
      // failed — offline for a moment, still routing over Tailscale, the server
      // mid-restart. Never erase what we knew over a blip; the explicit "sync is
      // not enabled" case (the setting itself is off) is the only real reset.
      if (/sync is not enabled/i.test(String(e))) {
        set({ role: null, name: "", id: "", isServerAdmin: false, folderNames: null });
        try { localStorage.removeItem(CACHE); } catch { /* ignore */ }
      }
    }
  },
}));

let started = false;
/** Start refreshing the role in the background (once). Retries quickly (every
 *  3s) while a team is configured but no role has been confirmed yet — the
 *  state a fresh join, a just-toggled server, or a network blip leaves this
 *  in — and settles into a slow 60s cadence once a role is known. Also listens
 *  for "team:kicked-out", emitted by the Rust side the moment any team-server
 *  call comes back 401 (this machine's token disabled or deleted from above):
 *  that reacts immediately instead of waiting for the next poll. */
export function startTeamRole() {
  if (started) return;
  started = true;
  const tick = async () => {
    await useTeam.getState().refresh();
    const { configured, role } = useTeam.getState();
    const delay = configured && role === null ? 3_000 : 60_000;
    setTimeout(tick, delay);
  };
  void tick();
  void listen("team:kicked-out", () => {
    toast.err("Tài khoản của bạn đã bị thu hồi quyền hoặc mã đã hết hiệu lực. Các profile của nhóm đã bị xoá khỏi máy này. Cần mã mới từ người cấp trên.");
    void useTeam.getState().refresh();
  });
  void listen<number>("team:wiped", (e) => {
    toast.err(`Đã xoá ${e.payload} profile của nhóm khỏi máy này vì quyền truy cập đã bị thu hồi.`);
  });
  // A profile closed but its state (logins included) did not reach the server.
  // Never silent: the copy on this machine is kept and saved on the next close.
  void listen<{ id: string; name: string; error: string }>("sync:checkin-failed", (e) => {
    useToastStore.getState().push(
      "err",
      `Chưa lưu được phiên của profile "${e.payload.name}" lên server. Bản trên máy này được giữ nguyên và sẽ lưu lại ở lần đóng sau.`,
      e.payload.error,
    );
  });
}

/** Members cannot add, change or delete anything. Fails closed: once a team is
 *  confirmed configured, an unconfirmed role (`null`) is never treated as
 *  unrestricted — only a genuinely local, no-team install defaults to open. */
export const canEdit = (role: TeamRole | null, configured: boolean | null) =>
  configured ? role !== "member" && role !== null : true;
export const canShare = (role: TeamRole | null, configured: boolean | null) =>
  configured ? role === "admin" || role === "manager" : true;
