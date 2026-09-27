import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Button, Input, Select } from "@proxyshard/shardx-ui-kit";
import { CopyField } from "../../shared/ui/CopyField";
import { toast } from "../../shared/model/toast";
import { teamInviteGenerate } from "../../entities/settings";

type Level = "none" | "use" | "edit" | "delete";
type Member = { id: string; name: string; role: "manager" | "member"; disabled: boolean; folders: Record<string, string> };
type Folder = { name: string; profiles: number; access: Record<string, string> };

const call = <T,>(method: string, path: string, body?: unknown) =>
  invoke<T>("team_admin", { method, path, body: body ?? null });

const LEVELS: { value: Level; label: string }[] = [
  { value: "none", label: "Không thấy" },
  { value: "use", label: "Chỉ dùng" },
  { value: "edit", label: "Dùng + sửa" },
  { value: "delete", label: "Dùng + sửa + xoá" },
];

/** Admin-only: name each person, hand them their own key, choose what each folder lets them do. */
export function MembersPanel({ serverUrl }: { serverUrl: string }) {
  const [members, setMembers] = useState<Member[] | null>(null);
  const [folders, setFolders] = useState<Folder[]>([]);
  const [name, setName] = useState("");
  const [role, setRole] = useState<"member" | "manager">("member");
  const [issued, setIssued] = useState<{ name: string; code: string } | null>(null);
  const [folder, setFolder] = useState("");

  const load = useCallback(async () => {
    try {
      const m = await call<{ members: Member[] }>("GET", "/admin/members");
      const f = await call<{ folders: Folder[] }>("GET", "/admin/folders");
      setMembers(m.members);
      setFolders(f.folders);
      setFolder((cur) => cur || f.folders[0]?.name || "");
    } catch {
      setMembers(null);
    }
  }, []);
  useEffect(() => { load(); }, [load]);

  // Only the admin (whose key created the server) can manage people.
  if (members === null) return null;

  const show = async (who: string, token: string) => {
    setIssued({ name: who, code: await teamInviteGenerate(serverUrl, token) });
  };
  const run = async (fn: () => Promise<void>) => {
    try { await fn(); await load(); } catch (e) { toast.err(String(e)); }
  };
  const add = () => run(async () => {
    const n = name.trim();
    if (!n) return;
    const r = await call<{ token: string }>("PUT", "/admin/members", { name: n, role });
    setName("");
    await show(n, r.token);
    toast.ok(`Đã tạo mã riêng cho ${n}`);
  });
  const patch = (m: Member, p: Record<string, unknown>) => run(async () => { await call("PUT", "/admin/members", { id: m.id, ...p }); });
  const rename = (m: Member) => {
    const n = window.prompt("Tên nhân sự", m.name)?.trim();
    if (n && n !== m.name) patch(m, { name: n });
  };
  const rotate = (m: Member) => run(async () => {
    if (!window.confirm(`Cấp lại mã cho ${m.name}? Mã cũ sẽ hết hiệu lực ngay.`)) return;
    const r = await call<{ token: string }>("PUT", "/admin/members", { id: m.id, rotate: true });
    await show(m.name, r.token);
  });
  const remove = (m: Member) => run(async () => {
    if (!window.confirm(`Xoá ${m.name}? Người này mất quyền ngay.`)) return;
    await call("POST", `/admin/members/${m.id}/delete`);
  });
  const setAccess = (memberId: string, level: Level) =>
    run(async () => { await call("PUT", `/admin/folders/${encodeURIComponent(folder)}/access`, { memberId, level }); });

  const current = folders.find((f) => f.name === folder);
  const people = members.filter((m) => m.role === "member");

  return (
    <div className="flex flex-col gap-3 rounded-lg bg-bg-weak-50 p-3">
      <span className="text-label-xs text-text-sub-600">Nhân sự & quyền theo thư mục</span>
      <p className="m-0 text-paragraph-xs text-text-soft-400">
        Mỗi người một mã riêng, gọi theo tên. Quản lý được thêm/sửa/xoá mọi thứ; nhân sự chỉ làm được trong thư mục bạn cho phép.
      </p>

      <div className="flex items-end gap-2">
        <Input inputSize="small" label="Tên nhân sự" value={name} onChange={(e) => setName(e.target.value)} placeholder="VD: Lan - Sale 1" className="flex-1" />
        <Select
          value={role}
          onChange={(v) => setRole(v as "member" | "manager")}
          options={[{ value: "member", label: "Nhân sự" }, { value: "manager", label: "Quản lý" }]}
          className="w-32"
        />
        <Button variant="primary" size="small" onClick={add} disabled={!name.trim()}>Tạo mã</Button>
      </div>

      {issued && (
        <div className="flex flex-col gap-1.5 rounded-md border border-stroke-soft-200 p-2">
          <span className="text-paragraph-xs text-text-sub-600">Mã của <b>{issued.name}</b> — chỉ hiện lần này, gửi cho đúng người:</span>
          <CopyField value={issued.code} />
        </div>
      )}

      <div className="flex flex-col gap-1">
        {members.length === 0 && <span className="text-paragraph-xs text-text-soft-400">Chưa có nhân sự nào.</span>}
        {members.map((m) => (
          <div key={m.id} className="flex flex-wrap items-center gap-2 rounded-md border border-stroke-soft-200 px-2 py-1.5">
            <span className={`text-paragraph-sm ${m.disabled ? "text-text-soft-400 line-through" : "text-text-strong-950"}`}>{m.name}</span>
            <span className="rounded bg-bg-soft-200 px-1.5 text-paragraph-xs text-text-sub-600">{m.role === "manager" ? "Quản lý" : "Nhân sự"}</span>
            {m.disabled && <span className="text-paragraph-xs text-warning-base">Đã khoá</span>}
            <span className="flex-1" />
            <Button variant="neutral" mode="ghost" size="xsmall" onClick={() => rename(m)}>Đổi tên</Button>
            <Button variant="neutral" mode="ghost" size="xsmall" onClick={() => rotate(m)}>Cấp lại mã</Button>
            <Button variant="neutral" mode="ghost" size="xsmall" onClick={() => patch(m, { disabled: !m.disabled })}>{m.disabled ? "Mở khoá" : "Khoá"}</Button>
            <Button variant="neutral" mode="ghost" size="xsmall" onClick={() => remove(m)}>Xoá</Button>
          </div>
        ))}
      </div>

      {people.length > 0 && (
        <div className="flex flex-col gap-2">
          <span className="text-label-xs text-text-sub-600">Quyền theo thư mục</span>
          {folders.length === 0 ? (
            <span className="text-paragraph-xs text-text-soft-400">Chưa có thư mục nào được đồng bộ. Đưa profile vào thư mục rồi quay lại đây.</span>
          ) : (
            <>
              <Select
                value={folder}
                onChange={(v) => setFolder(v as string)}
                options={folders.map((f) => ({ value: f.name, label: `${f.name} (${f.profiles} profile)` }))}
              />
              {people.map((m) => (
                <div key={m.id} className="flex items-center gap-2">
                  <span className="flex-1 text-paragraph-sm">{m.name}</span>
                  <Select
                    value={(current?.access[m.id] as Level) ?? "none"}
                    onChange={(v) => setAccess(m.id, v as Level)}
                    options={LEVELS}
                    className="w-48"
                  />
                </div>
              ))}
              <p className="m-0 text-paragraph-xs text-text-soft-400">Người không có quyền sẽ không thấy thư mục này. Thêm profile mới chỉ admin/quản lý làm được.</p>
            </>
          )}
        </div>
      )}
    </div>
  );
}
