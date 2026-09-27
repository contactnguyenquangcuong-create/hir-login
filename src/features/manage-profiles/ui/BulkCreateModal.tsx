import { useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { Button, DialogModal } from "@proxyshard/shardx-ui-kit";
import { toast } from "../../../shared/lib/toast";
import { useProfile } from "../../../entities/profile";

type ParseRow = { row: number; name: string; folder: string; notes: string; proxy: string; color: string; error: string | null };
type CreateItem = { index: number; ok: boolean; id: string | null; error: string | null };

function downloadTemplate() {
  const header = "name,folder,notes,proxy,color\n";
  const ex1 = 'FB 01,Ads,via US,socks5://user:pass@1.2.3.4:1080,#8b5cf6\n';
  const ex2 = 'FB 02,,ghi chú,http://5.6.7.8:8080,#22c55e\n';
  const csv = header + ex1 + ex2;
  const blob = new Blob([csv], { type: "text/csv;charset=utf-8" });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url; a.download = "hir-bulk-template.csv"; a.click();
  URL.revokeObjectURL(url);
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
    const payload = rows.filter((r) => !r.error).map((r) => ({ name: r.name, folder: r.folder, notes: r.notes, proxy: r.proxy, color: r.color }));
    setCreating(true);
    try {
      const res = await invoke<CreateItem[]>("profile_bulk_create", { rows: payload });
      setResults(res);
      const ok = res.filter((x) => x.ok).length;
      const fail = res.filter((x) => !x.ok).length;
      if (ok > 0) { reload(); toast.ok(`Đã tạo ${ok} profile${fail ? `, ${fail} lỗi` : ""}`); }
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
        <p className="m-0 text-paragraph-xs text-text-soft-400">
          Cột: <b>name</b> (bắt buộc), <b>folder</b>, <b>notes</b>, <b>proxy</b> (vd socks5://user:pass@host:port), <b>color</b> (#rrggbb). Fingerprint random.
          <button className="ml-2 underline text-primary-base" onClick={downloadTemplate}>Tải file mẫu .csv</button>
        </p>
        <div className="flex gap-2 items-center">
          <Button variant="neutral" mode="stroke" size="small" onClick={pickFile}>Chọn file .xlsx / .csv</Button>
          {path && <span className="truncate text-paragraph-xs text-text-soft-400 ml-2">{path}</span>}
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
                    <th className="px-2 py-1">#</th><th className="px-2 py-1">name</th><th className="px-2 py-1">folder</th><th className="px-2 py-1">proxy</th><th className="px-2 py-1">color</th><th className="px-2 py-1">lỗi</th>
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
        {!rows && <p className="m-0 text-paragraph-xs text-text-soft-400">Chưa chọn file. Dùng file mẫu để xem định dạng.</p>}
      </div>
    </DialogModal>
  );
}
