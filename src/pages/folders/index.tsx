import { useCallback, useEffect, useMemo, useState } from "react";
import { Button, Input } from "@proxyshard/shardx-ui-kit";
import { Topbar } from "../../shared/ui/Topbar";
import { FolderIcon } from "../../shared/icons";
import { toast } from "../../shared/model/toast";
import { confirmModal } from "../../shared/model/confirm";
import { useNav } from "../../shared/model/navigation";
import { startTeamRole, teamCall, useTeam, canShare } from "../../shared/model/teamRole";
import { useProfile, useFolders } from "../../entities/profile";
import { ShareFolderModal } from "../../features/manage-profiles/ui/ShareFolderModal";
import { Section, Pill } from "../settings/ui";

type Member = { id: string; name: string; role: "manager" | "member"; disabled: boolean };
type ServerFolder = { name: string; profiles: number; access: Record<string, string>; canDelete?: boolean; createdBy?: { role: "admin" | "manager"; name: string } };
type TrashedFolder = { name: string; deletedBy: string; daysLeft: number };

/** Every folder in one place: how many profiles it holds, who may use or manage it, and the sharing dialog. */
export function FoldersPage() {
  const role = useTeam((s) => s.role);
  const setSection = useNav((s) => s.setSection);
  const profiles = useProfile((s) => s.profiles);
  const rememberFolder = useProfile((s) => s.rememberFolder);
  const forgetFolder = useProfile((s) => s.forgetFolder);
  const localFolders = useFolders();
  const [server, setServer] = useState<ServerFolder[] | null>(null);
  const [members, setMembers] = useState<Member[]>([]);
  const [sharing, setSharing] = useState<string | null>(null);
  const [trash, setTrash] = useState<TrashedFolder[]>([]);
  const [name, setName] = useState("");

  useEffect(() => { startTeamRole(); }, []);

  const load = useCallback(async () => {
    if (!canShare(role)) return;
    try {
      setServer((await teamCall<{ folders: ServerFolder[] }>("GET", "/admin/folders")).folders);
      setMembers((await teamCall<{ members: Member[] }>("GET", "/admin/members")).members);
      setTrash((await teamCall<{ items: TrashedFolder[] }>("GET", "/admin/folders/trash").catch(() => ({ items: [] as TrashedFolder[] }))).items);
    } catch (e) { toast.err(String(e)); }
  }, [role]);
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

  const create = async () => {
    const n = name.trim();
    if (!n) return;
    try { await teamCall("PUT", "/admin/folders", { name: n }); rememberFolder(n); setName(""); toast.ok(`Đã tạo thư mục "${n}"`); await load(); }
    catch (e) { toast.err(/409|exists/i.test(String(e)) ? "Thư mục này đã tồn tại." : String(e)); }
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
  const purge = async (f: string) => {
    const ok = await confirmModal({ title: `Xoá vĩnh viễn "${f}"?`, message: "Sau khi xoá vĩnh viễn không khôi phục được nữa.", danger: true });
    if (ok !== true) return;
    try { await teamCall("POST", `/admin/folders/${encodeURIComponent(f)}/purge`); await load(); }
    catch (e) { toast.err(String(e)); }
  };

  return (
    <section className="flex flex-col">
      <Topbar crumbs={["Workspace", "Thư mục"]} search="" onSearch={() => {}} />
      <h1 className="m-0 mb-1 text-title-h5 text-text-strong-950">Thư mục</h1>
      <p className="m-0 mb-4 text-paragraph-sm text-text-soft-400">Chia thư mục cho từng nhóm. Ai không được chia sẻ sẽ không thấy thư mục đó.</p>

      <div className="flex max-w-[980px] flex-col gap-4">
        {role === null && (
          <Section title="Chưa kết nối team" desc="Kết nối tới máy chủ của team để chia sẻ thư mục cho nhân sự.">
            <div className="px-5 py-4"><Button variant="primary" size="small" onClick={() => setSection("settings")}>Mở cài đặt đồng bộ nhóm</Button></div>
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
                  <Pill tone="neutral">Chỉ dùng</Pill>
                </div>
              ))}
          </Section>
        )}

        {canShare(role) && (
          <Section
            title="Tất cả thư mục"
            desc={role === "admin" ? "Bạn thấy mọi thư mục của team." : "Bạn thấy các thư mục được giao cho bạn hoặc do bạn tạo."}
          >
            <form className="flex items-end gap-2 px-5 py-4" onSubmit={(e) => { e.preventDefault(); create(); }}>
              <div className="flex-1"><Input inputSize="small" label="Tạo thư mục mới" value={name} onChange={(e) => setName(e.target.value)} placeholder="Tên thư mục, ví dụ: Shop A" /></div>
              <Button type="submit" variant="primary" size="small" disabled={!name.trim()}>Tạo</Button>
            </form>
            {rows.length === 0 && <div className="px-5 py-8 text-center text-paragraph-xs text-text-soft-400">Chưa có thư mục nào.</div>}
            {rows.map((f) => {
              const people = Object.entries(f.access);
              return (
                <div key={f.name} className="flex flex-wrap items-center gap-3 px-5 py-3.5">
                  <FolderIcon className="size-5 flex-none text-icon-soft-400" />
                  <div className="flex min-w-0 flex-1 flex-col gap-1.5">
                    <div className="flex items-center gap-2">
                      <span className="truncate text-label-sm text-text-strong-950">{f.name}</span>
                      <Pill>{f.profiles} profile</Pill>
                      {f.local && <Pill tone="warning">Chỉ có trên máy này</Pill>}
                    </div>
                    <div className="flex flex-wrap gap-1">
                      {people.length === 0
                        ? <span className="text-paragraph-xs text-text-soft-400">Chưa chia sẻ cho ai</span>
                        : people.map(([id, lvl]) => (
                          <Pill key={id} tone={lvl === "manage" ? "primary" : "neutral"}>{nameOf(id)} · {lvl === "manage" ? "quản lý" : "được dùng"}</Pill>
                        ))}
                    </div>
                  </div>
                  <div className="flex flex-none gap-1">
                    <Button variant="primary" mode="stroke" size="xsmall" onClick={() => setSharing(f.name)}>Chia sẻ</Button>
                    {f.profiles === 0 && (f.local || f.canDelete !== false) && <Button variant="neutral" mode="ghost" size="xsmall" onClick={() => remove(f.name, !f.local)}>Xoá</Button>}
                    {!f.local && f.canDelete === false && <span className="self-center text-paragraph-xs text-text-soft-400" title="Chỉ người tạo thư mục hoặc Quản trị mới xoá được. Bạn vẫn dùng và chia sẻ được.">{f.createdBy?.role === "manager" ? `Do Quản lý ${f.createdBy.name} tạo`.replace("  ", " ") : "Do Quản trị tạo"}</span>}
                  </div>
                </div>
              );
            })}
          </Section>
        )}

        {canShare(role) && trash.length > 0 && (
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
    </section>
  );
}
