import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";

/** What the window shows while a machine that has just joined a team takes every profile of it
 *  down, once. Without it the profiles arrive one by one in the background and a person can open
 *  one that is not here yet. A quarter of a circle turns while the count says how far it is. */
export function FirstDownload({ open }: { open: boolean }) {
  const [p, setP] = useState<{ done: number; total: number } | null>(null);

  useEffect(() => {
    if (!open) { setP(null); return; }
    let off: (() => void) | undefined;
    let gone = false;
    void listen<{ done: number; total: number }>("team:pull-progress", (e) => {
      if (!gone) setP(e.payload.total > 0 ? e.payload : null);
    }).then((u) => { if (gone) u(); else off = u; });
    return () => { gone = true; off?.(); };
  }, [open]);

  if (!open) return null;
  const pct = p && p.total > 0 ? Math.min(100, Math.round((p.done / p.total) * 100)) : null;
  return (
    <div role="status" aria-live="polite" className="fixed inset-0 z-[1000] flex items-center justify-center bg-black/40">
      <div className="flex w-[320px] flex-col items-center gap-3 rounded-16 bg-bg-white-0 p-6 shadow-xl ring-1 ring-stroke-soft-200">
        <svg width="56" height="56" viewBox="0 0 56 56" className="animate-spin" aria-hidden>
          <circle cx="28" cy="28" r="22" fill="none" strokeWidth="5" className="stroke-bg-soft-200" />
          <circle cx="28" cy="28" r="22" fill="none" strokeWidth="5" strokeLinecap="round" strokeDasharray="34 138" className="stroke-primary-base" />
        </svg>
        <div className="text-label-md text-text-strong-950">Đang tải toàn bộ profile của nhóm</div>
        <div className="text-center text-paragraph-sm text-text-sub-600">
          {p ? `${p.done}/${p.total} profile${pct !== null ? ` · ${pct}%` : ""}` : "Đang lấy danh sách…"}
        </div>
        <div className="text-center text-paragraph-xs text-text-soft-400">Chỉ làm một lần. Đợi xong rồi mới mở profile để không sót cái nào.</div>
      </div>
    </div>
  );
}
