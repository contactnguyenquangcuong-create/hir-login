import { useCallback, useEffect, useState } from "react";
import { Button, Input, Select } from "@proxyshard/shardx-ui-kit";
import { CopyField } from "../../shared/ui/CopyField";
import { toast } from "../../shared/model/toast";
import { confirmModal } from "../../shared/model/confirm";
import { teamCall, useTeam } from "../../shared/model/teamRole";
import { teamInviteGenerate, teamInviteGenerateWithAuth, tailscaleCreateKey } from "../../entities/settings";
import { Section, Block, Pill, avatar } from "./ui";

type Role = "manager" | "member";
type Member = { id: string; name: string; role: Role; disabled: boolean };

const ROLES = [
  { value: "member", label: "Thành viên" },
  { value: "manager", label: "Quản lý nhóm" },
];

const LEGEND = [
  ["Quản trị", "Người giữ server, toàn quyền."],
  ["Quản lý nhóm", "Thêm, sửa, xoá trong thư mục được giao hoặc tự tạo."],
  ["Thành viên", "Chỉ dùng profile trong thư mục được chia sẻ."],
];

/** Admin only: the team's people, each with a name and a key of their own. Folders are shared from
 *  the folders themselves. `oauthReady` comes from the parent (TeamTab) rather than being checked
 *  here too — the two used to drift out of sync: this panel could still say "no Tailscale key yet"
 *  right after the admin had just configured one two sections up. */
export function MembersPanel({ serverUrl, oauthReady, onOpenOauthSetup }: { serverUrl: string; oauthReady: boolean; onOpenOauthSetup: () => void }) {
  const role = useTeam((s) => s.role);
  const [members, setMembers] = useState<Member[]>([]);
  const [name, setName] = useState("");
  const [newRole, setNewRole] = useState<Role>("member");
  const [issued, setIssued] = useState<{ name: string; code: string } | null>(null);
  const [busy, setBusy] = useState(false);

  const load = useCallback(async () => {
    try { setMembers((await teamCall<{ members: Member[] }>("GET", "/admin/members")).members); } catch { /* not the admin */ }
  }, []);
  useEffect(() => { if (role === "admin") load(); }, [role, load]);
  if (role !== "admin") return null;

  const run = async (fn: () => Promise<void>) => {
    setBusy(true);
    try { await fn(); await load(); } catch (e) { toast.err(String(e)); }
    finally { setBusy(false); }
  };
  // A brand-new machine needs two things to be useful with one code: Tailscale
  // network access, and this person's Hir-Login permission level. When an OAuth
  // client is configured, mint them their own Tailscale key (named after them,
  // so it can be found and revoked on its own in Tailscale's Keys page) instead
  // of leaving the code Hir-Login-only.
  const show = async (who: string, token: string) => {
    let code: string;
    if (oauthReady) {
      const key = await tailscaleCreateKey(`Hir-Login: ${who}`);
      code = await teamInviteGenerateWithAuth(serverUrl, token, key);
    } else {
      code = await teamInviteGenerate(serverUrl, token);
    }
    setIssued({ name: who, code });
  };

  const add = () => run(async () => {
    const n = name.trim();
    if (!n) return;
    const r = await teamCall<{ token: string }>("PUT", "/admin/members", { name: n, role: newRole });
    setName("");
    await show(n, r.token);
  });
  const patch = (m: Member, p: Record<string, unknown>) => run(async () => { await teamCall("PUT", "/admin/members", { id: m.id, ...p }); });
  const [renaming, setRenaming] = useState<{ id: string; text: string } | null>(null);
  const commitRename = (m: Member) => {
    const n = renaming?.text.trim();
    setRenaming(null);
    if (n && n !== m.name) patch(m, { name: n });
  };
  const rotate = (m: Member) => run(async () => {
    if ((await confirmModal({ title: `Cấp lại mã cho ${m.name}?`, message: "Mã cũ hết hiệu lực ngay, người này phải dùng mã mới." })) !== true) return;
    const r = await teamCall<{ token: string }>("PUT", "/admin/members", { id: m.id, rotate: true });
    await show(m.name, r.token);
  });
  const remove = (m: Member) => run(async () => {
    if ((await confirmModal({ title: `Xoá ${m.name}?`, message: "Người này mất quyền ngay và mã của họ không dùng được nữa.", danger: true })) !== true) return;
    await teamCall("POST", `/admin/members/${m.id}/delete`);
  });

  return (
    <Section
      title="Nhân sự"
      desc={
        oauthReady ? (
          'Mỗi người một mã riêng, kèm sẵn quyền vào mạng Tailscale — dùng được ngay trên máy mới. Chia sẻ thư mục cho họ ở trang Trình duyệt: mở thư mục, bấm "Chia sẻ quyền".'
        ) : (
          <>
            Mỗi người một mã riêng, gọi theo tên. Mã này <b>chưa kèm quyền vào Tailscale</b> — máy nhận mã cần đã ở trong mạng từ trước.{" "}
            <button type="button" className="border-0 bg-transparent p-0 text-primary-base underline" onClick={onOpenOauthSetup}>
              Bật để mã tự kèm quyền mạng
            </button>
            . Chia sẻ thư mục cho họ ở trang Trình duyệt: mở thư mục, bấm "Chia sẻ quyền".
          </>
        )
      }
    >
      <Block>
        <div className="grid grid-cols-1 gap-2 sm:grid-cols-3">
          {LEGEND.map(([t, d]) => (
            <div key={t} className="rounded-lg bg-bg-weak-50 p-3">
              <div className="text-label-xs text-text-strong-950">{t}</div>
              <div className="mt-0.5 text-paragraph-xs text-text-soft-400">{d}</div>
            </div>
          ))}
        </div>
      </Block>

      <Block>
        <form className="flex flex-wrap items-end gap-2" onSubmit={(e) => { e.preventDefault(); add(); }}>
          <div className="min-w-[220px] flex-1">
            <Input inputSize="small" label="Thêm nhân sự" value={name} onChange={(e) => setName(e.target.value)} placeholder="Tên, ví dụ: Lan - Sale 1" />
          </div>
          <div className="w-44"><Select size="small" value={newRole} onChange={(v) => setNewRole(v as Role)} options={ROLES} /></div>
          <Button type="submit" variant="primary" size="small" disabled={!name.trim()} isLoading={busy}>Tạo mã</Button>
        </form>
        {issued && (
          <div className="flex flex-col gap-2 rounded-lg bg-success-alpha-10 p-3 ring-1 ring-inset ring-success-alpha-16">
            <span className="text-paragraph-xs text-text-sub-600">
              Mã của <b className="text-text-strong-950">{issued.name}</b>. Chỉ hiện một lần, hãy gửi đúng người:
            </span>
            <CopyField value={issued.code} />
            <div><Button variant="neutral" mode="ghost" size="xsmall" onClick={() => setIssued(null)}>Ẩn mã</Button></div>
          </div>
        )}
      </Block>

      {members.length === 0 ? (
        <div className="px-5 py-8 text-center text-paragraph-xs text-text-soft-400">Chưa có nhân sự nào. Thêm người đầu tiên ở trên.</div>
      ) : (
        members.map((m) => (
          <div key={m.id} className="flex flex-wrap items-center gap-3 px-5 py-3">
            {avatar(m.name)}
            <div className="flex min-w-0 flex-1 items-center gap-2">
              {renaming?.id === m.id ? (
                <input
                  autoFocus
                  value={renaming.text}
                  onChange={(e) => setRenaming({ id: m.id, text: e.target.value })}
                  onBlur={() => commitRename(m)}
                  onKeyDown={(e) => { if (e.key === "Enter") commitRename(m); if (e.key === "Escape") setRenaming(null); }}
                  className="w-48 rounded-md border border-stroke-soft-200 bg-bg-white-0 px-2 py-1 text-label-sm text-text-strong-950"
                />
              ) : (
                <span className={`truncate text-label-sm ${m.disabled ? "text-text-soft-400 line-through" : "text-text-strong-950"}`}>{m.name}</span>
              )}
              <Pill tone={m.role === "manager" ? "primary" : "neutral"}>{m.role === "manager" ? "Quản lý nhóm" : "Thành viên"}</Pill>
              {m.disabled && <Pill tone="warning">Đã khoá</Pill>}
            </div>
            <div className="flex flex-none flex-wrap justify-end gap-1">
              <Button variant="neutral" mode="ghost" size="xsmall" onClick={() => setRenaming({ id: m.id, text: m.name })}>Đổi tên</Button>
              <Button variant="neutral" mode="ghost" size="xsmall" onClick={() => patch(m, { role: m.role === "manager" ? "member" : "manager" })}>
                {m.role === "manager" ? "Hạ xuống thành viên" : "Lên quản lý nhóm"}
              </Button>
              <Button variant="neutral" mode="ghost" size="xsmall" onClick={() => rotate(m)} isLoading={busy}>Cấp lại mã</Button>
              <Button variant="neutral" mode="ghost" size="xsmall" onClick={() => patch(m, { disabled: !m.disabled })}>{m.disabled ? "Mở khoá" : "Khoá"}</Button>
              <Button variant="neutral" mode="ghost" size="xsmall" onClick={() => remove(m)}>Xoá</Button>
            </div>
          </div>
        ))
      )}
    </Section>
  );
}
