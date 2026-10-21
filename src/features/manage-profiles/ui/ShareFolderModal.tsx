import { useCallback, useEffect, useState } from "react";
import { Button, DialogModal, Select } from "@proxyshard/shardx-ui-kit";
import { FolderIcon } from "../../../shared/icons";
import { toast } from "../../../shared/model/toast";
import { teamCall, useTeam } from "../../../shared/model/teamRole";
import { PARTNERS, usePartnerShare, type PartnerKey } from "../../../entities/profile";

type Level = "none" | "use" | "manage";
type Member = { id: string; name: string; role: "manager" | "member"; disabled: boolean };
type Folder = { name: string; access: Record<string, string> };

const initial = (n: string) => (n.trim()[0] ?? "?").toUpperCase();

/** Share one folder: who may use it, and (admin only) who manages it. The partner tools on this
 *  machine (HirS, Hir-F) are listed like people once their "Cho phép … kết nối" is on: "use" lets
 *  one open and close the profiles here, "manage" also add, edit (its proxies) and delete them;
 *  unshared, it never sees this folder. */
export function ShareFolderModal({ folder, onClose }: { folder: string; onClose: () => void }) {
  const role = useTeam((s) => s.role);
  const configured = useTeam((s) => s.configured);
  const [members, setMembers] = useState<Member[] | null>(null);
  const [access, setAccess] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState<string | null>(null);
  const allowed = usePartnerShare((s) => s.allowed);
  const partnerAccess = usePartnerShare((s) => s.access);
  const loadPartners = usePartnerShare((s) => s.load);
  const setPartner = usePartnerShare((s) => s.set);
  const partners = PARTNERS.filter((p) => allowed[p.key]);

  const load = useCallback(async () => {
    loadPartners();
    // No team on this machine: only the partner tools are left to share with.
    if (configured !== true) { setMembers([]); return; }
    try {
      // A folder that only exists locally is registered on the server first.
      let folders = (await teamCall<{ folders: Folder[] }>("GET", "/admin/folders")).folders;
      if (!folders.some((f) => f.name === folder)) {
        try { await teamCall("PUT", "/admin/folders", { name: folder }); } catch { /* already there */ }
        folders = (await teamCall<{ folders: Folder[] }>("GET", "/admin/folders")).folders;
      }
      setAccess(folders.find((f) => f.name === folder)?.access ?? {});
      setMembers((await teamCall<{ members: Member[] }>("GET", "/admin/members")).members.filter((m) => !m.disabled));
    } catch (e) {
      toast.err(String(e));
      onClose();
    }
  }, [folder, onClose, configured, loadPartners]);
  useEffect(() => { load(); }, [load]);

  const set = async (m: Member, level: Level) => {
    setBusy(m.id);
    try {
      await teamCall("PUT", `/admin/folders/${encodeURIComponent(folder)}/access`, { memberId: m.id, level });
      setAccess((a) => {
        const n = { ...a };
        if (level === "none") delete n[m.id]; else n[m.id] = level;
        return n;
      });
    } catch (e) {
      toast.err(String(e));
    } finally {
      setBusy(null);
    }
  };

  const sharePartner = async (key: PartnerKey, level: Level) => {
    setBusy(key);
    try { await setPartner(key, folder, level); }
    catch (e) { toast.err(String(e)); }
    finally { setBusy(null); }
  };

  const optionsFor = (m: Member) => [
    { value: "none", label: "Không có quyền" },
    { value: "use", label: "Được dùng" },
    ...(role === "admin" && m.role === "manager" ? [{ value: "manage", label: "Quản lý thư mục" }] : []),
  ];
  // A group manager only shares with plain members; the admin with anyone.
  const shown = (members ?? []).filter((m) => role === "admin" || m.role === "member");

  return (
    <DialogModal
      open
      onClose={onClose}
      icon={<FolderIcon className="size-5" />}
      title={`Chia sẻ thư mục "${folder}"`}
      confirmLabel="Xong"
      onConfirm={onClose}
      cancelLabel="Đóng"
      onCancel={onClose}
    >
      <div className="flex flex-col gap-3 py-4">
        {configured === true && <p className="m-0 text-paragraph-xs text-text-soft-400">
          Thành viên chỉ được <b className="text-text-sub-600">dùng</b> profile trong thư mục (mở, đóng — không thêm, sửa, xoá).
          {role === "admin" && " Quản lý nhóm được toàn quyền trong thư mục mà bạn giao cho họ."}
          {" "}Ai không được chia sẻ sẽ không thấy thư mục này.
        </p>}

        {partners.map((p) => {
          const level: Level = partnerAccess[p.key][folder] ?? "none";
          return (
            <div key={p.key} className="flex items-center gap-3 rounded-lg border border-stroke-soft-200 px-3 py-2.5">
              <span className="grid size-8 flex-none place-items-center rounded-full bg-primary-alpha-10 text-label-xs text-primary-base">{p.name[0]}</span>
              <div className="flex min-w-0 flex-1 flex-col">
                <span className="truncate text-label-sm text-text-strong-950">{p.name}</span>
                <span className="text-paragraph-xs text-text-soft-400">
                  {level === "manage"
                    ? "Công cụ trên máy này — được thêm, sửa (gắn proxy), xoá profile trong thư mục"
                    : level === "use"
                      ? "Công cụ trên máy này — chỉ được mở, đóng profile trong thư mục"
                      : "Công cụ trên máy này — chưa thấy thư mục này"}
                </span>
              </div>
              <div className="w-44 flex-none">
                <Select
                  value={level}
                  onChange={(v) => sharePartner(p.key, v as Level)}
                  options={[{ value: "none", label: "Không có quyền" }, { value: "use", label: "Được dùng" }, { value: "manage", label: "Quản lý thư mục" }]}
                  disabled={busy === p.key}
                />
              </div>
            </div>
          );
        })}

        {members === null ? (
          <span className="text-paragraph-sm text-text-soft-400">Đang tải…</span>
        ) : configured !== true ? (
          partners.length > 0 ? null : (
            <span className="text-paragraph-sm text-text-soft-400">
              Chưa có ai để chia sẻ. Kết nối team trong Cài đặt → Đồng bộ nhóm, hoặc bật Cài đặt → Nâng cao → Cho phép HirS / Hir-F kết nối để chia sẻ cho công cụ đó.
            </span>
          )
        ) : shown.length === 0 ? (
          <span className="text-paragraph-sm text-text-soft-400">
            Chưa có ai để chia sẻ. Admin thêm nhân sự trong Cài đặt → Đồng bộ nhóm.
          </span>
        ) : (
          <div className="flex max-h-[340px] flex-col divide-y divide-stroke-soft-200 overflow-y-auto rounded-lg border border-stroke-soft-200">
            {shown.map((m) => {
              const lvl = (access[m.id] as Level) ?? "none";
              return (
                <div key={m.id} className="flex items-center gap-3 px-3 py-2.5">
                  <span className="grid size-8 flex-none place-items-center rounded-full bg-primary-alpha-10 text-label-xs text-primary-base">{initial(m.name)}</span>
                  <div className="flex min-w-0 flex-1 flex-col">
                    <span className="truncate text-label-sm text-text-strong-950">{m.name}</span>
                    <span className="text-paragraph-xs text-text-soft-400">{m.role === "manager" ? "Quản lý nhóm" : "Thành viên"}</span>
                  </div>
                  <div className="w-44 flex-none">
                    <Select value={lvl} onChange={(v) => set(m, v as Level)} options={optionsFor(m)} disabled={busy === m.id} />
                  </div>
                </div>
              );
            })}
          </div>
        )}
        <div className="flex justify-end">
          <Button variant="neutral" mode="ghost" size="xsmall" onClick={load}>Tải lại</Button>
        </div>
      </div>
    </DialogModal>
  );
}
