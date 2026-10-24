import { useCallback, useEffect, useMemo, useState } from "react";
import { Button, Input } from "@proxyshard/shardx-ui-kit";
import { Topbar } from "../../shared/ui/Topbar";
import { FolderIcon } from "../../shared/icons";
import { toast } from "../../shared/model/toast";
import { confirmModal } from "../../shared/model/confirm";
import { useNav } from "../../shared/model/navigation";
import { startTeamRole, teamCall, useTeam, canShare } from "../../shared/model/teamRole";
import { useProfile, useFolders, usePartnerShare, PARTNERS } from "../../entities/profile";
import { ShareFolderModal } from "../../features/manage-profiles/ui/ShareFolderModal";
import { offerTeamFolder, makeTeamFolder, stopTeamFolder, askFolderSync } from "../../shared/model/folderSync";
import { Section, Pill } from "../settings/ui";
import { useContextMenu } from "../../shared/hooks/useContextMenu";
import { MoreIcon, TeamCloudIcon, LocalOnlyIcon } from "../../shared/icons";

type Member = { id: string; name: string; role: "manager" | "member"; disabled: boolean };
type ServerFolder = { name: string; profiles: number; access: Record<string, string>; canDelete?: boolean; createdBy?: { role: "admin" | "manager"; name: string } };
type TrashedFolder = { name: string; deletedBy: string; daysLeft: number };

/** Every folder in one place: how many profiles it holds, who may use or manage it, and the sharing dialog. */
export function FoldersPage() {
  const role = useTeam((s) => s.role);
  const configured = useTeam((s) => s.configured);
  const setSection = useNav((s) => s.setSection);
  const profiles = useProfile((s) => s.profiles);
  const rememberFolder = useProfile((s) => s.rememberFolder);
  const forgetFolder = useProfile((s) => s.forgetFolder);
  const setFolder = useProfile((s) => s.setFolder);
  const ctx = useContextMenu();
  const localFolders = useFolders();
  const [server, setServer] = useState<ServerFolder[] | null>(null);
  const [members, setMembers] = useState<Member[]>([]);
  const [sharing, setSharing] = useState<string | null>(null);
  const [trash, setTrash] = useState<TrashedFolder[]>([]);
  const [name, setName] = useState("");
  const [filter, setFilter] = useState("");
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

  const load = useCallback(async () => {
    // No team on this machine: nothing to ask the server (canShare is "open" for a
    // local install, which must not turn into a call that can only fail).
    if (configured !== true || !canShare(role, configured)) return;
    try {
      setServer((await teamCall<{ folders: ServerFolder[] }>("GET", "/admin/folders")).folders);
      setMembers((await teamCall<{ members: Member[] }>("GET", "/admin/members")).members);
      setTrash((await teamCall<{ items: TrashedFolder[] }>("GET", "/admin/folders/trash").catch(() => ({ items: [] as TrashedFolder[] }))).items);
    } catch (e) { if (!/sync is not enabled/i.test(String(e))) toast.err(String(e)); }
  }, [role, configured]);
  useEffect(() => { load(); }, [load, sharing]);

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
    const want = await askFolderSync(f.name, f.local ? "local" : "team");
    if (!want || want === (f.local ? "local" : "team")) return;
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

        {canShare(role, configured) && (
          // One row per folder, in columns: what it is, how many profiles, who has it, who made it.
          <div className="overflow-hidden rounded-lg bg-bg-white-0 shadow-[var(--shadow-xs)] ring-1 ring-inset ring-stroke-soft-200">
            <div className="f-cols f-head border-b border-stroke-soft-200 bg-bg-weak-50 text-subheading-2xs text-text-soft-400">
              <div>TÊN</div>
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
              const max = Math.max(1, 3 - shared.length);
              const deletable = f.profiles === 0 && (f.local || f.canDelete !== false);
              const open = () => { setFolder(f.name); setSection("browsers"); };
              const menu = [
                { label: "Xem profile trong thư mục", onClick: open },
                { label: "Chia sẻ…", onClick: () => setSharing(f.name) },
                ...(configured === true ? [{ label: f.local ? "Đồng bộ cho cả nhóm" : "Ngừng đồng bộ (chỉ trên máy này)", onClick: () => changeSync(f) }] : []),
                ...(deletable ? [{ sep: true, label: "", onClick: () => {} }, { label: "Xoá thư mục", onClick: () => remove(f.name, !f.local), danger: true }] : []),
              ];
              return (
                <div key={f.name} className="f-cols border-t border-stroke-soft-200 transition-colors first:border-t-0 hover:bg-bg-weak-50" onContextMenu={(e) => ctx.open(e, menu)}>
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
                    {configured === true && (
                      // Whether the folder is synced for the team; pressing it switches.
                      <button
                        type="button"
                        className={`sync-mark inline-flex flex-none cursor-pointer items-center gap-1 rounded-full border-0 px-2 py-0.5 text-paragraph-xs font-medium ${f.local ? "bg-warning-alpha-10 text-warning-base hover:bg-warning-alpha-16" : "bg-success-alpha-10 text-success-base hover:bg-success-alpha-16"}`}
                        title={f.local ? "Chỉ trên máy này, không đồng bộ lên nhóm. Bấm để đổi." : "Đang đồng bộ cho cả nhóm. Bấm để đổi."}
                        onClick={() => changeSync(f)}
                      >
                        {f.local ? <LocalOnlyIcon className="size-3.5" /> : <TeamCloudIcon className="size-3.5" />}
                        {f.local ? "Chỉ trên máy này" : "Đồng bộ"}
                      </button>
                    )}
                  </div>
                  <div className={`cell-count text-paragraph-xs ${f.profiles > 0 ? "text-text-sub-600" : "text-text-soft-400"}`}>{f.profiles}</div>
                  {/* Who has it, on one line: the first few, then how many more (all of them on hover). */}
                  <div
                    className="flex min-w-0 items-center gap-1 overflow-hidden"
                    title={[...shared.map((p) => `${p.name} · ${p.level === "manage" ? "quản lý" : "được dùng"}`), ...people.map(([id, lvl]) => `${nameOf(id)} · ${lvl === "manage" ? "quản lý" : "được dùng"}`)].join("\n") || undefined}
                  >
                    {people.length === 0 && shared.length === 0
                      ? <span className="text-paragraph-xs text-text-soft-400">Chưa chia sẻ</span>
                      : <>
                        {partnerPills(f.name)}
                        {people.slice(0, max).map(([id, lvl]) => (
                          <Pill key={id} tone={lvl === "manage" ? "primary" : "neutral"}>{nameOf(id)} · {lvl === "manage" ? "quản lý" : "dùng"}</Pill>
                        ))}
                        {people.length > max && <span className="flex-none text-paragraph-xs text-text-soft-400">+{people.length - max}</span>}
                      </>}
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

      {sharing && <ShareFolderModal folder={sharing} onClose={() => { setSharing(null); load(); }} />}
      {ctx.node}
    </section>
  );
}
