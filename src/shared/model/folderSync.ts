import { invoke } from "@tauri-apps/api/core";
import { confirmModal } from "./confirm";
import { teamCall, useTeam, canShare } from "./teamRole";
import { toast, useToastStore } from "./toast";

/** Answers already given this session, so one new folder is asked about once. */
const answered = new Map<string, "team" | "local">();

const SEEN = "hir.teamFoldersSeen";
/**
 * The team folders the server showed before and no longer does (deleted, or access taken away),
 * given what it shows now; remembers `now` for next time. A folder this machine turned into its
 * own ("Chỉ trên máy này") is not among them, and neither is one the server never showed — an
 * empty folder made only on this machine stays.
 */
export function teamFoldersGone(now: string[]): string[] {
  let before: string[] = [];
  try { before = JSON.parse(localStorage.getItem(SEEN) || "[]"); } catch { /* none */ }
  try { localStorage.setItem(SEEN, JSON.stringify(now)); } catch { /* ignore */ }
  return before.filter((f) => !now.includes(f) && answered.get(f) !== "local");
}

/**
 * A folder was just made on this machine: on a team, someone who may make team folders (an admin,
 * a group manager, the server's own admin) is asked whether it is the team's — a folder on the
 * team's server, whose profiles go to the team and which can be shared with members — or stays on
 * this machine. Nobody else is asked: with no team, or as a member, a new folder is this
 * machine's. Returns the choice, or null when the question was closed without an answer.
 */
export async function offerTeamFolder(name: string): Promise<"team" | "local" | null> {
  const folder = name.trim();
  const { configured, role, folderNames } = useTeam.getState();
  if (!folder || configured !== true || !canShare(role, configured)) return "local";
  if (folderNames?.includes(folder)) return "team";
  const earlier = answered.get(folder);
  if (earlier) return earlier;

  const choice = await confirmModal({
    title: `Đồng bộ thư mục "${folder}" cho cả nhóm?`,
    message:
      "Đồng bộ: thư mục có trên máy chủ của nhóm, profile trong đó được đồng bộ lên nhóm, và bạn chia sẻ được cho nhân sự. " +
      "Chỉ trên máy này: thư mục và profile trong đó ở lại máy này. Muốn đổi sau thì bấm nhãn của thư mục ở trang Thư mục.",
    buttons: [
      { label: "Chỉ trên máy này", value: "local" },
      { label: "Đồng bộ cho cả nhóm", value: "team", primary: true },
    ],
    equal: true,
  });
  if (choice === "team") {
    const ok = await makeTeamFolder(folder);
    answered.set(folder, ok ? "team" : "local");
    return ok ? "team" : "local";
  }
  if (choice === "local") {
    answered.set(folder, "local");
    return "local";
  }
  return null;
}

/** Whether the team's server lists `folder` for this person right now. */
async function onServer(folder: string): Promise<boolean> {
  try {
    const r = await teamCall<{ folders: { name: string }[] }>("GET", "/admin/folders");
    return r.folders.some((f) => f.name === folder);
  } catch { return false; }
}

/**
 * Puts a folder on the team's server; its profiles then go up with the next sync. True only when
 * the server really lists it afterwards — a "done" that left the folder marked local is worse than
 * an error, so a refusal is said out loud.
 */
export async function makeTeamFolder(folder: string): Promise<boolean> {
  let why = "";
  try {
    await teamCall("PUT", "/admin/folders", { name: folder });
  } catch (e) {
    const msg = String(e);
    if (!/409|exists/i.test(msg)) { toast.err(msg); return false; }
    // The name is taken: already the team's, or in the team's trash after "stop syncing" — then
    // bring it back, with whom it was shared.
    try { await teamCall("POST", `/admin/folders/${encodeURIComponent(folder)}/restore`); }
    catch (e2) { why = String(e2); }
  }
  if (!(await onServer(folder))) {
    // Pushed as is (not through friendlyError): the server's own words go in the details.
    useToastStore.getState().push("err", `Máy chủ nhóm chưa nhận thư mục "${folder}", nên chưa đồng bộ được. Hãy thử lại, hoặc nhờ quản trị kiểm tra máy chủ.`, why || undefined);
    return false;
  }
  answered.set(folder, "team");
  toast.ok(`Đã đồng bộ thư mục "${folder}" cho cả nhóm`);
  void useTeam.getState().refresh();
  void invoke("sync_kick").catch(() => {});
  return true;
}

/**
 * A team folder goes back to this machine only: it leaves the team's server (into the team's
 * trash for 30 days, so syncing it again brings back whom it was shared with) and the people it
 * was shared with no longer see it. Only an empty one: profiles already on the team stay the
 * team's — taking them off would delete them on every other machine.
 */
export async function stopTeamFolder(folder: string, teamProfiles: number): Promise<boolean> {
  if (teamProfiles > 0) {
    await confirmModal({
      title: `Chưa ngừng đồng bộ được "${folder}"`,
      message: `Thư mục đang có ${teamProfiles} profile trên nhóm. Ngừng đồng bộ lúc này sẽ xoá các profile đó trên máy của mọi người, nên phần mềm không làm. Hãy chuyển các profile sang thư mục khác (hoặc xoá) trước, rồi ngừng đồng bộ.`,
      buttons: [{ label: "Đã hiểu", value: true, primary: true }],
    });
    return false;
  }
  try {
    await teamCall("POST", `/admin/folders/${encodeURIComponent(folder)}/delete`);
  } catch (e) {
    const msg = String(e);
    toast.err(/409/.test(msg) ? "Thư mục còn profile trên nhóm, hãy chuyển hoặc xoá hết profile trước."
      : /403|folder-owner/.test(msg) ? "Thư mục do cấp trên tạo, bạn không ngừng đồng bộ được."
      : msg);
    return false;
  }
  answered.set(folder, "local");
  toast.ok(`Thư mục "${folder}" giờ chỉ trên máy này`);
  void useTeam.getState().refresh();
  return true;
}

/**
 * Waits until the team's server shows no profile left in `folder` (profiles moved or deleted here
 * reach it with the next sync round, which is asked for every few seconds). Returns how many it
 * still shows — 0 when it is empty, more when the wait ran out.
 */
export async function waitTeamFolderEmpty(folder: string, ms = 150_000): Promise<number> {
  const t0 = Date.now();
  let left = -1;
  for (let i = 0; Date.now() - t0 < ms; i++) {
    if (i % 3 === 0) void invoke("sync_kick").catch(() => {});
    try {
      const r = await teamCall<{ folders: { name: string; profiles: number }[] }>("GET", "/admin/folders");
      const f = r.folders.find((x) => x.name === folder);
      left = f ? f.profiles : 0;
      if (left === 0) return 0;
    } catch { /* try again */ }
    await new Promise((res) => setTimeout(res, 3000));
  }
  return left;
}

/** The choice behind a folder's sync mark: the team's, or this machine's only. Null when closed. */
export async function askFolderSync(folder: string, now: "team" | "local"): Promise<"team" | "local" | null> {
  return confirmModal({
    title: `Thư mục "${folder}"`,
    message: now === "team"
      ? "Đang đồng bộ cho cả nhóm: thư mục có trên máy chủ của nhóm, profile trong đó được đồng bộ, và chia sẻ được cho nhân sự."
      : "Đang chỉ trên máy này: thư mục và profile trong đó không đồng bộ lên nhóm.",
    buttons: [
      { label: "Chỉ trên máy này", value: "local", primary: now === "local", current: now === "local" },
      { label: "Đồng bộ cho cả nhóm", value: "team", primary: now === "team", current: now === "team" },
    ],
    equal: true,
  });
}
