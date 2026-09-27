import { useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import { Button, DialogModal } from "@proxyshard/shardx-ui-kit";
import { toast } from "../../../shared/lib/toast";
import { useProfile } from "../../../entities/profile";

type ParseRow = { row: number; name: string; folder: string; notes: string; proxy: string; color: string; kind: string; error: string | null };
type CreateItem = { index: number; ok: boolean; id: string | null; error: string | null };

async function downloadTemplate() {
  try {
    const dest = await save({ defaultPath: "hir-login-mau-tao-profile.xlsx", filters: [{ name: "Excel", extensions: ["xlsx"] }] });
    if (!dest) return;
    await invoke("bulk_template_save", { path: dest });
    toast.ok("Đã lưu file mẫu");
  } catch (e) { toast.err(String(e)); }
}

export function BulkCreateButton() {
  const [openM, setOpenM] = useState(false);
  return (
    <>
      <Button variant="neutral" mode="stroke" size="small" onClick={() => setOpenM(true)}>Bulk Excel</Button>
      {openM && <BulkCreateModal onClose={() => setOpenM(false)} />}
    </>
  );
}

function BulkCreateModal({ onClose }: { onClose: () => void }) {
  const reload = useProfile((s) => s.reload);
  const rememberFolder = useProfile((s) => s.rememberFolder);
  const [rows, setRows] = useState<ParseRow[] | null>(null);
  const [path, setPath] = useState("");
  const [creating, setCreating] = useState(false);
  const [results, setResults] = useState<CreateItem[] | null>(null);

  const validCount = useMemo(() => rows ? rows.filter((r) => !r.error).length : 0, [rows]);
  const errCount = useMemo(() => rows ? rows.filter((r) => !!r.error).length : 0, [rows]);

  const pickFile = async () => {
    const sel = await open({ multiple: false, filters: [{ name: "Excel/CSV", extensions: ["xlsx", "csv"] }] });
    if (typeof sel !== "string") return;
    await loadPath(sel);
  };

  const loadPath = async (p: string) => {
    setPath(p);
    setResults(null);
    try {
      const parsed = await invoke<ParseRow[]>("bulk_parse_file", { path: p });
      setRows(parsed);
      if (parsed.length === 0) toast.err("File trống hoặc không đọc được");
    } catch (e) { toast.err(String(e)); }
  };

  const create = async () => {
    if (!rows || validCount === 0) return;
    const payload = rows.filter((r) => !r.error).map((r) => ({ name: r.name, folder: r.folder, notes: r.notes, proxy: r.proxy, color: r.color, kind: r.kind }));
    setCreating(true);
    try {
      const res = await invoke<CreateItem[]>("profile_bulk_create", { rows: payload });
      setResults(res);
      const ok = res.filter((x) => x.ok).length;
      const fail = res.filter((x) => !x.ok).length;
      if (ok > 0) { for (const f of new Set(payload.map((r) => r.folder).filter(Boolean))) rememberFolder(f); reload(); toast.ok(`Đã tạo ${ok} profile${fail ? `, ${fail} lỗi` : ""}`); }
      if (fail > 0 && ok === 0) toast.err(`${fail} dòng lỗi`);
    } catch (e) { toast.err(String(e)); }
    finally { setCreating(false); }
  };

  return (
    <DialogModal
      open
      onClose={onClose}
      title="Tạo hàng loạt từ Excel / CSV"
      maxWidthClassName="max-w-[860px]"
      cancelLabel="Đóng"
      onCancel={onClose}
      confirmLabel={creating ? "Đang tạo..." : `Tạo ${validCount} profile`}
      onConfirm={create}
      isDisabled={!rows || validCount === 0 || creating}
      isLoading={creating}
    >
      <div className="flex flex-col gap-3 py-3">
        <div className="flex flex-col gap-3 rounded-xl bg-bg-weak-50 p-4 ring-1 ring-inset ring-stroke-soft-200">
          <div className="flex flex-col gap-1">
            <span className="text-label-sm text-text-strong-950">Bước 1: tải file mẫu và điền</span>
            <span className="text-paragraph-xs text-text-soft-400">
              Các cột: <b>Tên</b> (bắt buộc), <b>Thư mục</b>, <b>Ghi chú</b>, <b>Proxy</b>, <b>Loại proxy</b> (http/https/socks5 — chỉ cần khi ô Proxy không có tiền tố như <code>http://</code>), <b>Màu</b> (#rrggbb). Thư mục chưa có sẽ được tạo. Fingerprint được chọn ngẫu nhiên.
            </span>
          </div>
          <div><Button variant="neutral" mode="stroke" size="small" onClick={downloadTemplate}>Tải file Excel mẫu (.xlsx)</Button></div>
        </div>
        <div className="flex flex-col gap-2 rounded-xl bg-bg-weak-50 p-4 ring-1 ring-inset ring-stroke-soft-200">
          <span className="text-label-sm text-text-strong-950">Bước 2: chọn file đã điền (.xlsx hoặc .csv)</span>
          <div className="flex items-center gap-2">
            <Button variant="primary" mode="stroke" size="small" onClick={pickFile}>Chọn file</Button>
            {path && <span className="truncate text-paragraph-xs text-text-soft-400" title={path}>{path}</span>}
          </div>
        </div>

        {rows && (
          <div className="rounded-10 ring-1 ring-stroke-soft-200 overflow-hidden">
            <div className="flex gap-2 px-3 py-2 bg-bg-weak-50 text-paragraph-xs">
              <span>Tổng {rows.length}</span>
              <span className="text-emerald-600">hợp lệ {validCount}</span>
              {errCount > 0 && <span className="text-red-600">lỗi {errCount}</span>}
            </div>
            <div className="max-h-[340px] overflow-auto">
              <table className="w-full text-left text-paragraph-xs">
                <thead className="sticky top-0 bg-bg-weak-50">
                  <tr>
                    <th className="px-2 py-1">#</th><th className="px-2 py-1">Tên</th><th className="px-2 py-1">Thư mục</th><th className="px-2 py-1">Proxy</th><th className="px-2 py-1">Loại</th><th className="px-2 py-1">Màu</th><th className="px-2 py-1">Kết quả</th>
                  </tr>
                </thead>
                <tbody>
                  {rows.map((r, i) => {
                    const res = results?.find((x) => x.index === rows.filter((x) => !x.error).indexOf(r));
                    return (
                      <tr key={i} className={r.error ? "bg-red-50" : res?.ok === false ? "bg-amber-50" : ""}>
                        <td className="px-2 py-1">{r.row}</td>
                        <td className="px-2 py-1">{r.name}</td>
                        <td className="px-2 py-1">{r.folder}</td>
                        <td className="px-2 py-1 truncate max-w-[180px]">{r.proxy}</td>
                        <td className="px-2 py-1">{r.proxy ? (r.kind || "socks5") : ""}</td>
                        <td className="px-2 py-1">{r.color}</td>
                        <td className="px-2 py-1 text-red-600">{r.error ?? res?.error ?? (res?.ok ? "OK" : "")}</td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
          </div>
        )}
      </div>
    </DialogModal>
  );
}
