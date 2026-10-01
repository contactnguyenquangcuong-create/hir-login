import { useCallback, useEffect, useState } from "react";
import { Button, Input, Select } from "@proxyshard/shardx-ui-kit";
import { CopyField } from "../../shared/ui/CopyField";
import { toast } from "../../shared/model/toast";
import { confirmModal } from "../../shared/model/confirm";
import { teamCall, useTeam } from "../../shared/model/teamRole";
import { teamInviteGenerate, teamInviteGenerateWithAuth, tailscaleCreateKey, tailscaleListKeys, tailscaleRevokeKey } from "../../entities/settings";
import { Section, Block, Pill, avatar } from "./ui";

type Role = "admin" | "manager" | "member";
type Member = { id: string; name: string; role: Role; disabled: boolean };

const ROLES = [
  { value: "member", label: "Thành viên" },
  { value: "manager", label: "Quản lý nhóm" },
  { value: "admin", label: "Quản trị" },
];
const ROLE_PILL: Record<Role, { label: string; tone: "warning" | "primary" | "neutral" }> = {
  admin: { label: "Quản trị", tone: "warning" },
  manager: { label: "Quản lý nhóm", tone: "primary" },
  member: { label: "Thành viên", tone: "neutral" },
};

const LEGEND = [
  ["Quản trị", "Toàn quyền như người giữ server — kể cả thêm/sửa/xoá nhân sự khác. Mỗi người một mã riêng, không cần nhớ chung một token."],
  ["Quản lý nhóm", "Thêm, sửa, xoá trong thư mục được giao hoặc tự tạo."],
  ["Thành viên", "Chỉ dùng profile trong thư mục được chia sẻ."],
];

/** Admin only: the team's people, each with a name and a key of their own. Folders are shared from
 *  the folders themselves. `oauthReady` comes from the parent (TeamTab) rather than being checked
 *  here too — the two used to drift out of sync: this panel could still say "no Tailscale key yet"
 *  right after the admin had just configured one two sections up. */
export function MembersPanel({ getServerUrl, oauthReady, onOpenOauthSetup }: { getServerUrl: () => Promise<string>; oauthReady: boolean; onOpenOauthSetup: () => void }) {
  const role = useTeam((s) => s.role);
  // Only whoever holds the server's own token may rank (promote/demote/disable/
  // delete) an admin — a named admin here is a full equal everywhere else, but
  // not over a peer's rank. The server enforces this either way; disabling the
  // controls here just avoids offering an action that would 403.
  const isServerAdmin = useTeam((s) => s.isServerAdmin);
  const meId = useTeam((s) => s.id);
  const [members, setMembers] = useState<Member[]>([]);
  const [name, setName] = useState("");
  const [newRole, setNewRole] = useState<Role>("member");
  const [issued, setIssued] = useState<{ name: string; code: string } | null>(null);
  const [busy, setBusy] = useState(false);
  // Must stay above the early `return null` below: a hook declared after it
  // was skipped whenever `role` wasn't yet "admin" (e.g. right after joining
  // or disconnecting a team), so this component called a different number of
  // hooks from one render to the next — React error #300, "Rendered fewer
  // hooks than expected" — and crashed into the app's ErrorBoundary.
  const [renaming, setRenaming] = useState<{ id: string; text: string } | null>(null);

  const load = useCallback(async () => {
    try { setMembers((await teamCall<{ members: Member[] }>("GET", "/admin/members")).members); } catch { /* not the admin */ }
  }, []);
  useEffect(() => { if (role === "admin") load(); }, [role, load]);
  if (role !== "admin") return null;

  // `add` creates the member first, then mints a Tailscale key and invite code
  // for them — if that second step fails, the member still exists server-side.
  // Refreshing the list only on success used to leave it stale in that case:
  // the admin would see "no members yet", assume the whole thing failed, and
  // retry with the same name — silently creating a second member each time.
  const run = async (fn: () => Promise<void>) => {
    setBusy(true);
    try { await fn(); } catch (e) { toast.err(String(e)); }
    finally { await load(); setBusy(false); }
  };
  // A brand-new machine needs two things to be useful with one code: Tailscale
  // network access, and this person's Hir-Login permission level. When an OAuth
  // client is configured, mint them their own Tailscale key (named after them,
  // so it can be found and revoked on its own in Tailscale's Keys page) instead
  // of leaving the code Hir-Login-only.
  const show = async (who: string, token: string) => {
    const serverUrl = await getServerUrl();
    let code: string;
    if (oauthReady) {
      const key = await tailscaleCreateKey(`Hir-Login - ${who}`);
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
  // Hir-Login names each Tailscale key it mints "Hir-Login - <tên>" (see `show`
  // above), so the key(s) a deleted person holds are found by that same name —
  // there is no key id stored against the member to look up directly. A person
  // renamed after their key was minted keeps old key(s) under the old name;
  // those are missed here and need revoking by hand in Tailscale's own console.
  const revokeKeysFor = async (who: string) => {
    if (!oauthReady) return;
    try {
      const mine = (await tailscaleListKeys()).filter((k) => k.description === `Hir-Login - ${who}` && !k.invalid && !k.revoked);
      await Promise.all(mine.map((k) => tailscaleRevokeKey(k.id)));
    } catch (e) {
      // The person is already removed from Hir-Login either way; only the
      // network-level cleanup is what failed, worth a separate heads-up.
      toast.err(`Đã xoá ${who} nhưng chưa thu hồi được Auth Key của họ: ${String(e)}`);
    }
  };
  const remove = (m: Member) => run(async () => {
    if ((await confirmModal({ title: `Xoá ${m.name}?`, message: "Người này mất quyền ngay và mã của họ không dùng được nữa.", danger: true })) !== true) return;
    await teamCall("POST", `/admin/members/${m.id}/delete`);
    await revokeKeysFor(m.name);
  });

  return (
    <Section
      title="Nhân sự"
      desc={
        oauthReady ? (
          "Mỗi người một mã riêng, dùng được ngay trên máy mới."
        ) : (
          <>
            Mã chưa kèm quyền vào Tailscale.{" "}
            <button type="button" className="border-0 bg-transparent p-0 text-primary-base underline" onClick={onOpenOauthSetup}>
              Bật để tự kèm
            </button>
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
          <div className="w-44"><Select size="small" value={newRole} onChange={(v) => setNewRole(v as Role)} options={isServerAdmin ? ROLES : ROLES.filter((r) => r.value !== "admin")} /></div>
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
        members.map((m) => {
          const rankLocked = m.role === "admin" && !isServerAdmin;
          const isSelf = m.id === meId;
          return (
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
                <Pill tone={(ROLE_PILL[m.role] ?? ROLE_PILL.member).tone}>{(ROLE_PILL[m.role] ?? ROLE_PILL.member).label}</Pill>
                {m.disabled && <Pill tone="warning">Đã khoá</Pill>}
                {rankLocked && <span className="text-paragraph-xs text-text-soft-400" title="Chỉ người giữ server mới đổi được cấp bậc của một quản trị viên khác">Chỉ chủ server đổi được</span>}
              </div>
              <div className="flex flex-none flex-wrap items-center justify-end gap-1">
                <Button variant="neutral" mode="ghost" size="xsmall" onClick={() => setRenaming({ id: m.id, text: m.name })}>Đổi tên</Button>
                <div className="w-36"><Select size="small" value={m.role} onChange={(v) => patch(m, { role: v })} disabled={rankLocked} options={rankLocked || isServerAdmin ? ROLES : ROLES.filter((r) => r.value !== "admin")} /></div>
                <Button
                  variant="neutral" mode="ghost" size="xsmall"
                  disabled={isSelf}
                  title={isSelf ? "Nhờ người cấp trên cấp mã mới cho bạn — tự cấp lại sẽ khoá luôn phiên đang dùng" : undefined}
                  onClick={() => rotate(m)} isLoading={busy}
                >
                  Cấp lại mã
                </Button>
                <Button variant="neutral" mode="ghost" size="xsmall" disabled={rankLocked} onClick={() => patch(m, { disabled: !m.disabled })}>{m.disabled ? "Mở khoá" : "Khoá"}</Button>
                <Button variant="neutral" mode="ghost" size="xsmall" disabled={rankLocked} onClick={() => remove(m)}>Xoá</Button>
              </div>
            </div>
          );
        })
      )}
    </Section>
  );
}
