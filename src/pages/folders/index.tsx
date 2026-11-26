import { useCallback, useEffect, useMemo, useState } from "react";
import { createPortal } from "react-dom";
import { Button, Input } from "@proxyshard/shardx-ui-kit";
import { Topbar } from "../../shared/ui/Topbar";
import { FolderIcon } from "../../shared/icons";
import { toast } from "../../shared/model/toast";
import { confirmModal } from "../../shared/model/confirm";
import { useNav } from "../../shared/model/navigation";
import { startTeamRole, teamCall, useTeam, canShare } from "../../shared/model/teamRole";
import { useProfile, useFolders, usePartnerShare, PARTNERS, folderDelete, folderRename } from "../../entities/profile";
import { FolderModal } from "../../features/manage-profiles/ui/FolderModal";
import { ShareFolderModal } from "../../features/manage-profiles/ui/ShareFolderModal";
import { offerTeamFolder, makeTeamFolder, stopTeamFolder, askFolderSync, waitTeamFolderEmpty } from "../../shared/model/folderSync";
import { friendlyError } from "../../shared/lib/errorText";
import { Section, Pill } from "../settings/ui";
import { useContextMenu } from "../../shared/hooks/useContextMenu";
import { MoreIcon, TeamCloudIcon, LocalOnlyIcon, SyncIcon, RefreshIcon } from "../../shared/icons";

type Member = { id: string; name: string; role: "manager" | "member"; disabled: boolean };
type ServerFolder = { name: string; profiles: number; access: Record<string, string>; canDelete?: boolean; createdBy?: { role: "admin" | "manager"; name: string } };
type TrashedFolder = { name: string; deletedBy: string; daysLeft: number };

/** One person or partner tool a folder is shared with. */
type Sharee = { key: string; name: string; manage: boolean; tool: boolean };

/**
 * A soft colour per name, so the same person keeps the same colour in every row. Plain values:
 * the theme only emits the colour variables some class uses, so `--color-stable-base` and the like
 * are empty at run time and an avatar painted with them had no background.
 */
const SHAREE_COLORS = ["#335cff", "#1fc16b", "#22d3bb", "#47c2ff", "#f6b51e", "#7d52f4", "#fb4ba3"];
const hueOf = (s: string) => { let h = 0; for (const c of s) h = (h * 31 + c.charCodeAt(0)) >>> 0; return SHAREE_COLORS[h % SHAREE_COLORS.length]; };
const initialOf = (s: string) => (Array.from(s.replace(/\(bạn\)/i, "").trim())[0] ?? "?").toUpperCase();
const SHAREE_MAX_AVATARS = 4;

function ShareeAvatar({ s, plain }: { s: Sharee; plain?: boolean }) {
  const hue = hueOf(s.name);
  return (
    <span
      className={`relative grid size-7 flex-none place-items-center text-[11px] font-semibold ${plain ? "" : "shadow-[0_0_0_2px_var(--color-bg-white-0)]"} ${s.tool ? "rounded-md" : "rounded-full"}`}
      style={{ background: `color-mix(in srgb, ${hue} 22%, var(--color-bg-white-0))`, color: `color-mix(in srgb, ${hue} 70%, var(--color-text-strong-950))` }}
    >
      {initialOf(s.name)}
      {s.manage && <span className="absolute -right-0.5 -top-0.5 z-10 size-2 rounded-full bg-primary-base shadow-[0_0_0_1.5px_var(--color-bg-white-0)]" />}
    </span>
  );
}

/** The one open "who has it" box at a time: opening another row's closes this one. */
let closeOpenSharee: (() => void) | null = null;

/** Everyone a folder is shared with and how far, in a small box under the cell that was pressed. */
function ShareeBox({ list, at, onShare, onClose }: { list: Sharee[]; at: { x: number; y?: number; b?: number }; onShare: () => void; onClose: () => void }) {
  useEffect(() => {
    const down = (e: MouseEvent) => {
      const t = e.target as HTMLElement | null;
      if (!t?.closest("[data-sharee-box], [data-sharee-trigger]")) onClose();
    };
    const key = (e: KeyboardEvent) => { if (e.key === "Escape") onClose(); };
    document.addEventListener("mousedown", down);
    document.addEventListener("keydown", key);
    window.addEventListener("scroll", onClose, true);
    window.addEventListener("resize", onClose);
    return () => {
      document.removeEventListener("mousedown", down);
      document.removeEventListener("keydown", key);
      window.removeEventListener("scroll", onClose, true);
      window.removeEventListener("resize", onClose);
    };
  }, [onClose]);
  const manage = list.filter((x) => x.manage).length;
  return createPortal(
    <div
      data-sharee-box
      role="dialog"
      className="fixed z-[80] flex w-72 flex-col overflow-hidden rounded-lg bg-bg-white-0 shadow-[var(--shadow-md,0_8px_24px_rgba(0,0,0,0.18))] ring-1 ring-inset ring-stroke-soft-200"
      style={{ left: at.x, top: at.y, bottom: at.b }}
    >
      <div className="flex items-center justify-between border-b border-stroke-soft-200 px-3 py-2 text-subheading-2xs text-text-soft-400">
        <span>ĐÃ CHIA SẺ CHO ({list.length})</span>
        <span>{manage} quản lý</span>
      </div>
      <div className="flex max-h-64 flex-col overflow-y-auto py-1">
        {list.map((s) => (
          <div key={s.key} className="flex items-center gap-2.5 px-3 py-1.5">
            <ShareeAvatar s={s} plain />
            <span className="min-w-0 flex-1 truncate text-label-xs text-text-strong-950" title={s.name}>{s.name}</span>
            {s.tool && <span className="flex-none text-paragraph-xs text-text-soft-400">công cụ</span>}
            <span className={`flex-none rounded-full px-2 py-0.5 text-paragraph-xs font-medium ${s.manage ? "bg-primary-alpha-10 text-primary-base" : "bg-bg-weak-50 text-text-sub-600"}`}>{s.manage ? "Quản lý" : "Được dùng"}</span>
          </div>
        ))}
      </div>
      <div className="border-t border-stroke-soft-200 p-2">
        <Button variant="neutral" mode="stroke" size="xsmall" className="w-full" onClick={() => { onClose(); onShare(); }}>Chia sẻ…</Button>
      </div>
    </div>,
    document.body,
  );
}

/**
 * Who a folder is shared with, in the narrow "Chia sẻ cho" column: only the overlapping initials
 * (a dot on those who manage it; square for a partner tool) and two short counts. Names would not
 * fit; pressing the cell opens a box with everyone and their level, and the sharing dialog from it.
 */
function SharedWith({ list, onShare }: { list: Sharee[]; onShare: () => void }) {
  const [at, setAt] = useState<{ x: number; y?: number; b?: number } | null>(null);
  const close = useCallback(() => { setAt(null); if (closeOpenSharee === close) closeOpenSharee = null; }, []);
  if (list.length === 0) {
    return (
      <button
        type="button"
        onClick={onShare}
        title="Chưa chia sẻ cho ai. Bấm để chia sẻ."
        className="inline-flex w-fit cursor-pointer items-center gap-1.5 rounded-full border border-dashed border-stroke-soft-200 bg-transparent px-2.5 py-1 text-paragraph-xs text-text-soft-400 transition-colors hover:border-primary-base hover:text-primary-base"
      >
        <span className="text-label-xs leading-none">+</span>Chia sẻ
      </button>
    );
  }
  const manage = list.filter((x) => x.manage).length;
  const use = list.length - manage;
  const counts = [manage > 0 ? `${manage} quản lý` : "", use > 0 ? `${use} dùng` : ""].filter(Boolean).join(" · ");
  const toggle = (e: React.MouseEvent<HTMLButtonElement>) => {
    if (at) { close(); return; }
    closeOpenSharee?.();
    const r = e.currentTarget.getBoundingClientRect();
    const x = Math.max(8, Math.min(r.left, window.innerWidth - 296));
    // Under the cell, or above it when the row is near the bottom of the window.
    setAt(window.innerHeight - r.bottom < 300 ? { x, b: window.innerHeight - r.top + 6 } : { x, y: r.bottom + 6 });
    closeOpenSharee = close;
  };
  return (
    <>
      <button
        type="button"
        data-sharee-trigger
        onClick={toggle}
        title="Bấm để xem chi tiết"
        className={`flex min-w-0 cursor-pointer items-center gap-2.5 rounded-lg border-0 p-1 text-left transition-colors hover:bg-bg-weak-50 ${at ? "bg-bg-weak-50" : "bg-transparent"}`}
      >
        <span className="flex flex-none items-center">
          {list.slice(0, SHAREE_MAX_AVATARS).map((s, i) => <span key={s.key} className={i === 0 ? "" : "-ml-2"}><ShareeAvatar s={s} /></span>)}
          {list.length > SHAREE_MAX_AVATARS && (
            <span className="-ml-2 grid size-7 flex-none place-items-center rounded-full bg-bg-weak-50 text-[11px] font-semibold text-text-sub-600 shadow-[0_0_0_2px_var(--color-bg-white-0)]">+{list.length - SHAREE_MAX_AVATARS}</span>
          )}
        </span>
        <span className="min-w-0 truncate text-paragraph-xs text-text-soft-400">{counts}</span>
      </button>
      {at && <ShareeBox list={list} at={at} onShare={onShare} onClose={close} />}
    </>
  );
}

/** Every folder in one place: how many profiles it holds, who may use or manage it, and the sharing dialog. */
export function FoldersPage() {
  const role = useTeam((s) => s.role);
  const configured = useTeam((s) => s.configured);
  const setSection = useNav((s) => s.setSection);
  const profiles = useProfile((s) => s.profiles);
  const rememberFolder = useProfile((s) => s.rememberFolder);
  const rememberFolderSilent = useProfile((s) => s.rememberFolderSilent);
  const forgetFolder = useProfile((s) => s.forgetFolder);
  const bulkMoveToFolder = useProfile((s) => s.bulkMoveToFolder);
  const setFolderTab = useProfile((s) => s.setFolder);
  const currentTab = useProfile((s) => s.folder);
  const setFolder = useProfile((s) => s.setFolder);
  const ctx = useContextMenu();
  const localFolders = useFolders();
  const [server, setServer] = useState<ServerFolder[] | null>(null);
  const [members, setMembers] = useState<Member[]>([]);
  const [sharing, setSharing] = useState<string | null>(null);
  const [trash, setTrash] = useState<TrashedFolder[]>([]);
  const [name, setName] = useState("");
  const [filter, setFilter] = useState("");
  /** Folders whose sync mark is being switched: the mark says so until the server has answered. */
  const [switching, setSwitching] = useState<Set<string>>(new Set());
  const [limit, setLimit] = useState(50);
  // The partner tools (HirS, Hir-F) count as more people a folder is shared with (in the Chia sẻ
  // dialog); each only sees the folders shared with it, so it never writes its proxies into any
  // other.
  const partnerAllowed = usePartnerShare((s) => s.allowed);
  const partnerAccess = usePartnerShare((s) => s.access);
  const loadPartners = usePartnerShare((s) => s.load);
  /** The partner tools a folder is shared with, and how far. */
  const partnersOf = (f: string) =>
    PARTNERS.filter((p) => partnerAllowed[p.key] && partnerAccess[p.key][f]).map((p) => ({ name: p.name, level: partnerAccess[p.key][f] }));
  const partnerPills = (f: string) =>
    partnersOf(f).map((p) => (
      <Pill key={p.name} tone={p.level === "manage" ? "primary" : "neutral"}>{p.name} · {p.level === "manage" ? "quản lý" : "dùng"}</Pill>
    ));

  useEffect(() => { startTeamRole(); loadPartners(); }, [loadPartners]);

  /** Why the team server's folder list could not be read, while it cannot. */
  const [loadError, setLoadError] = useState<string | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  const load = useCallback(async () => {
    // No team on this machine: nothing to ask the server (canShare is "open" for a
    // local install, which must not turn into a call that can only fail).
    if (configured !== true || !canShare(role, configured)) return;
    try {
      setServer((await teamCall<{ folders: ServerFolder[] }>("GET", "/admin/folders")).folders);
      setLoadError(null);
      setMembers((await teamCall<{ members: Member[] }>("GET", "/admin/members")).members);
      setTrash((await teamCall<{ items: TrashedFolder[] }>("GET", "/admin/folders/trash").catch(() => ({ items: [] as TrashedFolder[] }))).items);
    } catch (e) {
      if (!/sync is not enabled/i.test(String(e))) setLoadError(friendlyError(String(e)).text);
    }
  }, [role, configured]);
  useEffect(() => { load(); }, [load, sharing]);
  // The list is read again every so often, and quickly while it is unknown: a first read made as
  // the app starts (network or VPN not up yet) used to leave every folder shown as "only on this
  // machine" until the page was left and entered again.
  useEffect(() => {
    if (configured !== true || !canShare(role, configured)) return;
    const t = setInterval(() => { void load(); }, server === null || loadError ? 8_000 : 60_000);
    return () => clearInterval(t);
  }, [configured, role, load, server, loadError]);
  const refresh = async () => { setRefreshing(true); try { await load(); } finally { setRefreshing(false); } };
  /** Whether it is not known which folders the team has (the read failed, and nothing was read before). */
  const unknown = configured === true && canShare(role, configured) && server === null;

  const localCount = useMemo(() => {
    const m: Record<string, number> = {};
    for (const p of profiles) if (p.folder) m[p.folder] = (m[p.folder] ?? 0) + 1;
    return m;
  }, [profiles]);

  const meId = useTeam((s) => s.id);
  const nameOf = (id: string) => {
    const m = members.find((x) => x.id === id);
    if (id === meId) return `${m?.name ?? "Bạn"} (bạn)`;
    return m?.name ?? "Quản lý khác";
  };

  // Folders on the server, then any that exist only on this machine so far.
  const rows = useMemo(() => {
    const seen = new Set((server ?? []).map((f) => f.name));
    const extra = localFolders.filter((f) => !seen.has(f)).map((f) => ({ name: f, profiles: localCount[f] ?? 0, access: {} as Record<string, string>, canDelete: true, createdBy: undefined, local: true }));
    return [...(server ?? []).map((f) => ({ ...f, local: false })), ...extra];
  }, [server, localFolders, localCount]);

  // A page of fifty at a time, filtered by what is typed: with a thousand folders the rows were a
  // thousand blocks of sharing pills to scroll through.
  const filtered = useMemo(() => {
    const q = filter.trim().toLowerCase();
    return q ? rows.filter((r) => r.name.toLowerCase().includes(q)) : rows;
  }, [rows, filter]);

  const create = async () => {
    const n = name.trim();
    if (!n) return;
    if (rows.some((r) => r.name === n)) { toast.err("Thư mục này đã tồn tại."); return; }
    // On a team, whoever makes it says whether it is the team's or this machine's only.
    const where = await offerTeamFolder(n);
    if (where === null) return;
    rememberFolder(n);
    setName("");
    toast.ok(where === "team" ? `Đã tạo thư mục "${n}" cho cả nhóm` : `Đã tạo thư mục "${n}"`);
    await load();
  };
  const remove = async (f: string, onServer: boolean) => {
    const ok = await confirmModal({
      title: `Xoá thư mục "${f}"?`,
      message: onServer
        ? "Thư mục sẽ vào thùng rác và được giữ 30 ngày, có thể khôi phục. Trong lúc đó mọi người được chia sẻ sẽ không còn thấy nó."
        : "Thư mục này chỉ có trên máy này và đang trống. Xoá khỏi danh sách?",
      danger: true,
    });
    if (ok !== true) return;
    if (onServer) {
      try { await teamCall("POST", `/admin/folders/${encodeURIComponent(f)}/delete`); }
      catch (e) {
        const msg = String(e);
        toast.err(/409/.test(msg) ? "Thư mục còn profile, hãy chuyển hoặc xoá hết profile trước."
          : /404|405/.test(msg) ? "Máy chủ chưa hỗ trợ xoá thư mục. Hãy cập nhật máy chủ lên bản mới nhất."
          : msg);
        return;
      }
      toast.ok(`Đã chuyển "${f}" vào thùng rác, giữ 30 ngày`);
    }
    // Also drop it from this machine's own list, or it would keep showing here.
    forgetFolder(f);
    await load();
  };
  const restore = async (f: string) => {
    try { await teamCall("POST", `/admin/folders/${encodeURIComponent(f)}/restore`); toast.ok(`Đã khôi phục "${f}"`); await load(); }
    catch (e) { toast.err(/409/.test(String(e)) ? "Đã có thư mục cùng tên, hãy đổi tên hoặc xoá thư mục đó trước." : String(e)); }
  };
  /** The folder's sync mark was pressed: keep it as it is, or switch between the team's and this machine's. */
  const changeSync = async (f: { name: string; local: boolean; profiles: number; access: Record<string, string> }) => {
    if (switching.has(f.name)) return;
    const want = await askFolderSync(f.name, f.local ? "local" : "team");
    if (!want || want === (f.local ? "local" : "team")) return;
    setSwitching((s) => new Set(s).add(f.name));
    try { await switchSync(f, want); }
    finally { setSwitching((s) => { const n = new Set(s); n.delete(f.name); return n; }); }
  };
  const switchSync = async (f: { name: string; profiles: number; access: Record<string, string> }, want: "team" | "local") => {
    if (want === "team") { if (await makeTeamFolder(f.name)) await load(); return; }
    const people = Object.keys(f.access).length + partnersOf(f.name).length;
    if (f.profiles === 0 && people > 0) {
      const ok = await confirmModal({
        title: `Ngừng đồng bộ "${f.name}"?`,
        message: `Thư mục đang chia sẻ cho ${people} người / công cụ; sau khi ngừng đồng bộ họ không còn thấy nó. Đồng bộ lại sau sẽ trả lại phần chia sẻ.`,
      });
      if (ok !== true) return;
    }
    if (await stopTeamFolder(f.name, f.profiles)) {
      rememberFolder(f.name);
      await load();
    }
  };
  /** A folder whose profiles are being moved out (to delete it, or just to empty it): asks where to. */
  const [mover, setMover] = useState<{ from: string; team: boolean; deleteAfter: boolean } | null>(null);
  /** Folder currently being renamed. */
  const [renaming, setRenaming] = useState<{ name: string; team: boolean } | null>(null);
  const busy = (name: string, on: boolean) => setSwitching((s) => { const n = new Set(s); if (on) n.add(name); else n.delete(name); return n; });

  const runRename = async (newName: string) => {
    const r = renaming;
    setRenaming(null);
    const next = newName.trim();
    if (!r || !next || next === r.name) return;
    if (rows.some((x) => x.name === next)) { toast.err("Thư mục này đã tồn tại."); return; }
    busy(r.name, true);
    try {
      if (r.team) {
        try {
          await teamCall("POST", `/admin/folders/${encodeURIComponent(r.name)}/rename`, { name: next });
        } catch (e) {
          const msg = String(e);
          toast.err(
            /409|exists/i.test(msg)
              ? "Tên thư mục này đã tồn tại."
              : /404|405/.test(msg)
                ? "Máy chủ chưa hỗ trợ đổi tên thư mục. Hãy cập nhật máy chủ lên bản mới nhất."
                : msg,
          );
          return;
        }
      }
      await folderRename(r.name, next);
      forgetFolder(r.name);
      rememberFolderSilent(next);
      if (currentTab === r.name) setFolderTab(next);
      await useProfile.getState().reload();
      if (r.team) void useTeam.getState().refresh();
      toast.ok(`Đã đổi tên "${r.name}" thành "${next}"`);
    } catch (e) { toast.err(String(e)); }
    finally { busy(r.name, false); await load(); }
  };

  /** The folder itself, once nothing is in it: off the team's server (to its trash) or this machine's list. */
  const dropFolder = async (name: string, team: boolean): Promise<boolean> => {
    if (team) {
      const left = await waitTeamFolderEmpty(name);
      if (left > 0) {
        toast.err(`Thư mục "${name}" chưa xoá được: máy chủ nhóm còn ${left} profile trong đó (chưa đồng bộ xong, hoặc profile chưa có trên máy này). Hãy thử lại sau ít phút.`);
        return false;
      }
      try { await teamCall("POST", `/admin/folders/${encodeURIComponent(name)}/delete`); }
      catch (e) { toast.err(String(e)); return false; }
    }
    forgetFolder(name);
    if (currentTab === name) setFolderTab("all");
    toast.ok(`Đã xoá thư mục "${name}"`);
    return true;
  };

  /** ⋮ → Xoá thư mục: with profiles in it, asks what becomes of them. */
  const removeFolder = async (f: { name: string; local: boolean; profiles: number }) => {
    const here = profiles.filter((p) => p.folder === f.name);
    if (here.length === 0 && f.profiles === 0) { await remove(f.name, !f.local); return; }
    const n = Math.max(here.length, f.profiles);
    const choice = await confirmModal({
      title: `Xoá thư mục "${f.name}"?`,
      message: `Thư mục đang có ${n} profile. Anh/chị muốn làm gì với chúng?`,
      buttons: [
        { label: "Hủy", value: "cancel" },
        { label: "Chuyển sang thư mục khác…", value: "move", primary: true },
        { label: `Xoá cả ${n} profile`, value: "delete", danger: true },
      ],
      equal: true,
    });
    if (choice === "move") setMover({ from: f.name, team: !f.local, deleteAfter: true });
    else if (choice === "delete") await deleteWithProfiles(f, n);
  };

  const deleteWithProfiles = async (f: { name: string; local: boolean }, n: number) => {
    const ok = await confirmModal({
      title: `Xoá thư mục "${f.name}" và ${n} profile trong đó?`,
      message: f.local
        ? "Các profile vào thùng rác của Hir-Login, giữ 7 ngày rồi xoá hẳn."
        : "Các profile vào thùng rác (giữ 7 ngày) và biến mất khỏi mọi máy trong nhóm. Thư mục vào thùng rác của nhóm 30 ngày.",
      danger: true,
    });
    if (ok !== true) return;
    busy(f.name, true);
    try {
      await folderDelete(f.name, true);
      await useProfile.getState().reload();
      const still = useProfile.getState().profiles.filter((p) => p.folder === f.name).length;
      if (still > 0) { toast.err(`${still} profile đang mở nên chưa xoá được, thư mục được giữ lại. Đóng chúng rồi xoá lại.`); return; }
      await dropFolder(f.name, !f.local);
    } catch (e) { toast.err(String(e)); }
    finally { busy(f.name, false); await load(); }
  };

  /** Moves every profile of the folder to `dest`, then removes the folder if that was asked. */
  const runMove = async (dest: string) => {
    const m = mover;
    setMover(null);
    if (!m) return;
    busy(m.from, true);
    try {
      // A team folder's profiles go to a team folder: a new name is made one first.
      if (m.team && !(server ?? []).some((x) => x.name === dest)) {
        if (!(await makeTeamFolder(dest))) return;
      }
      const ids = profiles.filter((p) => p.folder === m.from).map((p) => p.id);
      const { failed } = await bulkMoveToFolder(ids, dest);
      if (failed > 0) return; // some stayed: the folder is not removed under them
      if (m.deleteAfter) await dropFolder(m.from, m.team);
    } finally { busy(m.from, false); await load(); }
  };

  const purge = async (f: string) => {
    const ok = await confirmModal({ title: `Xoá vĩnh viễn "${f}"?`, message: "Sau khi xoá vĩnh viễn không khôi phục được nữa.", danger: true });
    if (ok !== true) return;
    try { await teamCall("POST", `/admin/folders/${encodeURIComponent(f)}/purge`); await load(); }
    catch (e) { toast.err(String(e)); }
  };

  return (
    <section className="flex flex-col">
      <Topbar crumbs={["Workspace", "Thư mục"]} search={filter} onSearch={(v) => { setFilter(v); setLimit(50); }} />
      <div className="mb-3 flex flex-wrap items-end justify-between gap-x-4 gap-y-2">
        <div className="flex min-w-0 flex-col gap-1">
          <div className="flex items-center gap-3">
            <h1 className="m-0 text-title-h5 text-text-strong-950">Thư mục</h1>
            {rows.length > 0 && <Pill>{filter.trim() ? `${filtered.length} / ${rows.length}` : rows.length} thư mục</Pill>}
          </div>
          <p className="m-0 text-paragraph-xs text-text-soft-400">Chia thư mục cho từng nhóm. Ai không được chia sẻ sẽ không thấy thư mục đó.</p>
        </div>
        {canShare(role, configured) && (
          <form className="flex items-center gap-2" onSubmit={(e) => { e.preventDefault(); create(); }}>
            <div className="w-64"><Input inputSize="small" value={name} onChange={(e) => setName(e.target.value)} placeholder="Tên thư mục mới, ví dụ: Shop A" /></div>
            <Button type="submit" variant="primary" size="small" disabled={!name.trim()}>Tạo thư mục</Button>
            <Button type="button" variant="neutral" mode="stroke" size="small" onlyIcon title="Làm mới danh sách từ máy chủ nhóm" disabled={refreshing} leftIcon={<RefreshIcon className={`size-4 ${refreshing ? "spin-icon" : ""}`} />} onClick={refresh} />
          </form>
        )}
      </div>

      <div className="flex flex-col gap-4">
        {configured === false && (
          <Section title="Chưa kết nối team" desc="Kết nối tới máy chủ của team để chia sẻ thư mục cho nhân sự.">
            <div className="px-5 py-4"><Button variant="primary" size="small" onClick={() => setSection("settings")}>Mở cài đặt đồng bộ nhóm</Button></div>
          </Section>
        )}

        {configured && role === null && (
          <Section title="Đang tải quyền của bạn…" desc="Máy đã kết nối team, đang xác nhận quyền — chỉ mất vài giây.">
            <div className="px-5 py-6 text-center text-paragraph-xs text-text-soft-400">Nếu treo lâu hơn 1 phút, kiểm tra lại kết nối mạng ở mục Cài đặt.</div>
          </Section>
        )}

        {role === "member" && (
          <Section title="Thư mục được chia sẻ cho bạn" desc="Bạn chỉ được dùng (mở, đóng) các profile trong những thư mục này.">
            {localFolders.length === 0
              ? <div className="px-5 py-8 text-center text-paragraph-xs text-text-soft-400">Chưa có thư mục nào được chia sẻ cho bạn.</div>
              : localFolders.map((f) => (
                <div key={f} className="flex items-center gap-3 px-5 py-3">
                  <FolderIcon className="size-5 text-icon-soft-400" />
                  <span className="flex-1 text-label-sm text-text-strong-950">{f}</span>
                  <Pill>{localCount[f] ?? 0} profile</Pill>
                  {partnerPills(f)}
                  <Pill tone="neutral">Chỉ dùng</Pill>
                </div>
              ))}
          </Section>
        )}

        {unknown && loadError && (
          <div className="flex flex-wrap items-center gap-3 rounded-lg bg-warning-alpha-10 px-4 py-3 text-paragraph-xs text-warning-base">
            <span className="min-w-0 flex-1">
              Chưa lấy được danh sách thư mục từ máy chủ nhóm ({loadError}). Các thư mục bên dưới là bản trên máy này, chưa biết đã đồng bộ hay chưa. Đang thử lại tự động.
            </span>
            <Button variant="neutral" mode="stroke" size="xsmall" onClick={refresh} disabled={refreshing}>Thử lại ngay</Button>
          </div>
        )}

        {canShare(role, configured) && (
          // One row per folder, in columns: what it is, how many profiles, who has it, who made it.
          <div className="overflow-hidden rounded-lg bg-bg-white-0 shadow-[var(--shadow-xs)] ring-1 ring-inset ring-stroke-soft-200">
            <div className={`f-cols f-head ${configured === true ? "f-sync" : ""} border-b border-stroke-soft-200 bg-bg-weak-50 text-subheading-2xs text-text-soft-400`}>
              <div>TÊN</div>
              {configured === true && <div>ĐỒNG BỘ</div>}
              <div className="cell-count">PROFILE</div>
              <div>CHIA SẺ CHO</div>
              <div>NGƯỜI TẠO</div>
              <div></div>
            </div>
            {rows.length === 0 && <div className="px-5 py-10 text-center text-paragraph-xs text-text-soft-400">Chưa có thư mục nào. Đặt tên ở ô phía trên rồi bấm Tạo thư mục.</div>}
            {rows.length > 0 && filtered.length === 0 && <div className="px-5 py-10 text-center text-paragraph-xs text-text-soft-400">Không có thư mục nào khớp “{filter}”.</div>}
            {filtered.slice(0, limit).map((f) => {
              const people = Object.entries(f.access);
              const shared = partnersOf(f.name);
              const deletable = f.local ? !unknown : f.canDelete !== false;
              const hasProfiles = !unknown && (f.profiles > 0 || (localCount[f.name] ?? 0) > 0);
              const open = () => { setFolder(f.name); setSection("browsers"); };
              const menu = [
                { label: "Xem profile trong thư mục", onClick: open },
                { label: "Chia sẻ…", onClick: () => setSharing(f.name) },
                ...(deletable && !switching.has(f.name) ? [{ label: "Đổi tên thư mục…", onClick: () => setRenaming({ name: f.name, team: !f.local }) }] : []),
                ...(configured === true ? [{ label: f.local ? "Đồng bộ cho cả nhóm" : "Ngừng đồng bộ (chỉ trên máy này)", onClick: () => changeSync(f) }] : []),
                ...(hasProfiles && !switching.has(f.name) ? [{ label: "Chuyển profile sang thư mục khác…", onClick: () => setMover({ from: f.name, team: !f.local, deleteAfter: false }) }] : []),
                ...(deletable && !switching.has(f.name) ? [{ sep: true, label: "", onClick: () => {} }, { label: "Xoá thư mục", onClick: () => { void removeFolder(f); }, danger: true }] : []),
              ];
              return (
                <div key={f.name} className={`f-cols ${configured === true ? "f-sync" : ""} border-t border-stroke-soft-200 transition-colors first:border-t-0 hover:bg-bg-weak-50`} onContextMenu={(e) => ctx.open(e, menu)}>
                  <div className="flex min-w-0 items-center gap-2.5">
                    <FolderIcon className="size-[18px] flex-none text-icon-soft-400" />
                    <button
                      type="button"
                      className="min-w-0 cursor-pointer truncate border-0 bg-transparent p-0 text-left text-label-xs text-text-strong-950 hover:text-primary-base"
                      title="Xem profile trong thư mục này"
                      onClick={open}
                    >
                      {f.name}
                    </button>
                  </div>
                  {configured === true && (
                    <div className="flex min-w-0 items-center">
                      {/* Whether the folder is synced for the team; pressing it switches. */}
                      <button
                        type="button"
                        disabled={unknown}
                        className={`sync-mark inline-flex flex-none items-center gap-1 rounded-full border-0 px-2 py-0.5 text-paragraph-xs font-medium ${unknown ? "cursor-default bg-bg-weak-50 text-text-soft-400" : f.local ? "cursor-pointer bg-warning-alpha-10 text-warning-base hover:bg-warning-alpha-16" : "cursor-pointer bg-success-alpha-10 text-success-base hover:bg-success-alpha-16"}`}
                        title={unknown ? "Chưa lấy được danh sách thư mục từ máy chủ nhóm, nên chưa biết thư mục này đã đồng bộ hay chưa." : f.local ? "Chỉ trên máy này, không đồng bộ lên nhóm. Bấm để đổi." : "Đang đồng bộ cho cả nhóm. Bấm để đổi."}
                        onClick={() => changeSync(f)}
                      >
                        {unknown
                          ? <><span className="spin-icon inline-grid place-items-center"><SyncIcon className="size-3.5" /></span>Đang kiểm tra…</>
                          : switching.has(f.name)
                            ? <><span className="spin-icon inline-grid place-items-center"><SyncIcon className="size-3.5" /></span>Đang chuyển…</>
                            : <>{f.local ? <LocalOnlyIcon className="size-3.5" /> : <TeamCloudIcon className="size-3.5" />}{f.local ? "Chỉ trên máy này" : "Đồng bộ"}</>}
                      </button>
                    </div>
                  )}
                  <div className={`cell-count text-paragraph-xs ${f.profiles > 0 ? "text-text-sub-600" : "text-text-soft-400"}`}>{f.profiles}</div>
                  {/* Who has it: initials stack + first names + counts (the partner tools first, then people). */}
                  <div className="flex min-w-0 items-center">
                    <SharedWith
                      list={[
                        ...shared.map((p): Sharee => ({ key: `tool:${p.name}`, name: p.name, manage: p.level === "manage", tool: true })),
                        ...people.map(([id, lvl]): Sharee => ({ key: id, name: nameOf(id), manage: lvl === "manage", tool: false })),
                      ]}
                      onShare={() => setSharing(f.name)}
                    />
                  </div>
                  <div className="min-w-0 truncate text-paragraph-xs text-text-soft-400">
                    {f.local ? "Máy này" : f.createdBy?.role === "manager" ? f.createdBy.name : f.createdBy ? "Quản trị" : "—"}
                  </div>
                  <div className="flex justify-end gap-1">
                    <Button variant="neutral" mode="stroke" size="xsmall" className="w-[72px] flex-none" onClick={() => setSharing(f.name)}>Chia sẻ</Button>
                    <Button
                      variant="neutral"
                      mode="stroke"
                      size="xsmall"
                      onlyIcon
                      title="Thêm thao tác"
                      leftIcon={<MoreIcon className="size-4" />}
                      onClick={(e) => { e.stopPropagation(); ctx.open(e, menu); }}
                    />
                  </div>
                </div>
              );
            })}
            {filtered.length > limit && (
              <div className="flex justify-center border-t border-stroke-soft-200 px-5 py-3">
                <Button variant="neutral" mode="stroke" size="xsmall" onClick={() => setLimit(limit + 50)}>
                  Hiện thêm 50 (còn {filtered.length - limit})
                </Button>
              </div>
            )}
          </div>
        )}

        {canShare(role, configured) && trash.length > 0 && (
          <Section title="Thùng rác thư mục" desc="Thư mục đã xoá được giữ 30 ngày rồi tự xoá vĩnh viễn. Khôi phục sẽ trả lại cả phần chia sẻ.">
            {trash.map((f) => (
              <div key={f.name} className="flex flex-wrap items-center gap-3 px-5 py-3">
                <FolderIcon className="size-5 flex-none text-icon-soft-400" />
                <div className="flex min-w-0 flex-1 flex-col">
                  <span className="truncate text-label-sm text-text-strong-950">{f.name}</span>
                  <span className="text-paragraph-xs text-text-soft-400">Xoá bởi {f.deletedBy || "?"} · còn {f.daysLeft} ngày</span>
                </div>
                <Button variant="neutral" mode="stroke" size="xsmall" onClick={() => restore(f.name)}>Khôi phục</Button>
                <Button variant="neutral" mode="ghost" size="xsmall" onClick={() => purge(f.name)}>Xoá vĩnh viễn</Button>
              </div>
            ))}
          </Section>
        )}
      </div>

      {mover && (
        <FolderModal
          mode="move"
          existing={(mover.team ? (server ?? []).map((x) => x.name) : localFolders).filter((x) => x !== mover.from)}
          onPick={(dest) => { void runMove(dest); }}
          onCreate={(name) => { const d = name.trim(); if (d) void runMove(d); }}
          onClose={() => setMover(null)}
        />
      )}
      {renaming && (
        <FolderModal
          mode="rename"
          initialName={renaming.name}
          existing={rows.map((x) => x.name)}
          onPick={() => {}}
          onCreate={(next) => { void runRename(next); }}
          onClose={() => setRenaming(null)}
        />
      )}
      {sharing && <ShareFolderModal folder={sharing} onClose={() => { setSharing(null); load(); }} />}
      {ctx.node}
    </section>
  );
}
