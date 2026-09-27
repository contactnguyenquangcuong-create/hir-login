import { useCallback, useEffect, useState } from "react";
import { Button } from "@proxyshard/shardx-ui-kit";
import { toast } from "../../shared/model/toast";
import { confirmModal } from "../../shared/model/confirm";
import { tailscaleListKeys, tailscaleRevokeKey, type TailscaleKey } from "../../entities/settings";
import { Section, Pill } from "./ui";

const fmt = (iso: string) => {
  if (!iso) return "—";
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? "—" : d.toLocaleDateString("vi-VN");
};

function statusOf(k: TailscaleKey): { label: string; tone: "success" | "warning" | "neutral" } {
  if (k.invalid || k.revoked) return { label: "Đã thu hồi", tone: "neutral" };
  const exp = new Date(k.expires).getTime();
  if (Number.isFinite(exp) && exp < Date.now()) return { label: "Hết hạn", tone: "warning" };
  return { label: "Đang dùng", tone: "success" };
}

/** Admin only, shown once an OAuth client is configured: every Tailscale key in the
 *  tailnet — including the ones Hir-Login minted for each person — with a way to
 *  revoke one without leaving the app. */
export function TailscaleKeysPanel() {
  const [keys, setKeys] = useState<TailscaleKey[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [revoking, setRevoking] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    try { setKeys(await tailscaleListKeys()); }
    catch (e) { toast.err(String(e)); setKeys(null); }
    finally { setLoading(false); }
  }, []);
  useEffect(() => { load(); }, [load]);

  const revoke = async (k: TailscaleKey) => {
    const ok = await confirmModal({
      title: `Thu hồi key "${k.description || k.id}"?`,
      message: "Máy nào đang dùng key này để vào mạng sẽ không join được nữa bằng nó. Không ảnh hưởng máy đã join rồi.",
      danger: true,
    });
    if (ok !== true) return;
    setRevoking(k.id);
    try { await tailscaleRevokeKey(k.id); toast.ok("Đã thu hồi"); await load(); }
    catch (e) { toast.err(String(e)); }
    finally { setRevoking(null); }
  };

  return (
    <Section
      title="Quản lý Auth Key"
      desc="Mọi Auth Key trong mạng Tailscale của bạn, kể cả key Hir-Login tự tạo cho từng người. Thu hồi tại đây có hiệu lực ngay."
      action={<Button variant="neutral" mode="stroke" size="xsmall" onClick={load} isLoading={loading}>Tải lại</Button>}
    >
      {keys === null && !loading && (
        <div className="px-5 py-8 text-center text-paragraph-xs text-text-soft-400">Không tải được danh sách. Kiểm tra lại Client ID/Secret ở trên.</div>
      )}
      {keys !== null && keys.length === 0 && (
        <div className="px-5 py-8 text-center text-paragraph-xs text-text-soft-400">Chưa có Auth Key nào.</div>
      )}
      {keys?.map((k) => {
        const st = statusOf(k);
        const canRevoke = !k.invalid && !k.revoked;
        return (
          <div key={k.id} className="flex flex-wrap items-center gap-3 px-5 py-3">
            <div className="flex min-w-0 flex-1 flex-col">
              <span className="truncate text-label-sm text-text-strong-950">{k.description || "(không có mô tả)"}</span>
              <span className="text-paragraph-xs text-text-soft-400">Tạo {fmt(k.created)} · Hết hạn {fmt(k.expires)}</span>
            </div>
            <Pill tone={st.tone}>{st.label}</Pill>
            <Button
              variant="neutral" mode="ghost" size="xsmall"
              disabled={!canRevoke}
              isLoading={revoking === k.id}
              onClick={() => revoke(k)}
            >
              Thu hồi
            </Button>
          </div>
        );
      })}
    </Section>
  );
}
