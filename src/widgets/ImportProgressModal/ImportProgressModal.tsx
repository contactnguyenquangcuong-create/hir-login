import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { useProfile } from "../../entities/profile";
import { useT } from "../../shared/i18n";

type ImportProgress = {
  done: number;
  total: number;
  imported: number;
  skipped: number;
  failed: number;
  current: string;
};

/** Blocking progress overlay shown while importing a folder of profiles.
 *  Prevents double-clicking import while showing a spinning % ring, done/total,
 *  skipped duplicate count, and the profile currently being copied. */
export function ImportProgressModal() {
  const open = useProfile((s) => s.importingBundle);
  const t = useT();
  const [p, setP] = useState<ImportProgress | null>(null);

  useEffect(() => {
    if (!open) { setP(null); return; }
    let off: (() => void) | undefined;
    let gone = false;
    void listen<ImportProgress>("profile:import-progress", (e) => {
      if (!gone) setP(e.payload);
    }).then((u) => { if (gone) u(); else off = u; });
    return () => { gone = true; off?.(); };
  }, [open]);

  if (!open) return null;

  const pct = p && p.total > 0 ? Math.min(100, Math.round((p.done / p.total) * 100)) : null;
  const r = 28;
  const circ = 2 * Math.PI * r;
  const filled = pct !== null ? (pct / 100) * circ : 0;

  return (
    <div role="status" aria-live="polite" className="fixed inset-0 z-[1000] flex items-center justify-center bg-black/45">
      <div className="flex w-[360px] flex-col items-center gap-3 rounded-16 bg-bg-white-0 p-6 shadow-xl ring-1 ring-stroke-soft-200">
        <div className="relative flex h-[72px] w-[72px] items-center justify-center">
          <svg width="72" height="72" viewBox="0 0 72 72" className="absolute inset-0 -rotate-90" aria-hidden>
            <circle cx="36" cy="36" r={r} fill="none" strokeWidth="5" className="stroke-bg-soft-200" />
            {pct !== null && (
              <circle
                cx="36"
                cy="36"
                r={r}
                fill="none"
                strokeWidth="5"
                strokeLinecap="round"
                strokeDasharray={`${filled} ${circ}`}
                className="stroke-primary-base transition-[stroke-dasharray] duration-200"
              />
            )}
          </svg>
          <svg width="72" height="72" viewBox="0 0 72 72" className="absolute inset-0 animate-spin" aria-hidden>
            <circle
              cx="36"
              cy="36"
              r={r}
              fill="none"
              strokeWidth="5"
              strokeLinecap="round"
              strokeDasharray="24 152"
              className="stroke-primary-base/70"
            />
          </svg>
          <span className="text-label-md font-semibold tabular-nums text-text-strong-950">
            {pct !== null ? `${pct}%` : "…"}
          </span>
        </div>

        <div className="text-label-md font-medium text-text-strong-950">
          {t("importProgress.title")}
        </div>

        <div className="text-center text-paragraph-sm tabular-nums text-text-sub-600">
          {p && p.total > 0
            ? t("importProgress.count", { done: p.done, total: p.total, pct: pct ?? 0 })
            : t("importProgress.scanning")}
        </div>

        {p && p.total > 0 && (
          <div className="flex flex-wrap items-center justify-center gap-x-3 gap-y-1 text-paragraph-xs tabular-nums text-text-soft-400">
            <span>{t("importProgress.imported", { n: p.imported })}</span>
            {p.skipped > 0 && <span>· {t("importProgress.skipped", { n: p.skipped })}</span>}
            {p.failed > 0 && <span className="text-error-base">· {t("importProgress.failed", { n: p.failed })}</span>}
          </div>
        )}

        {p?.current && (
          <div className="max-w-full truncate rounded-8 bg-bg-weak-50 px-2.5 py-1 text-paragraph-xs text-text-sub-600" title={p.current}>
            {p.current}
          </div>
        )}

        <div className="text-center text-paragraph-xs text-text-soft-400">
          {t("importProgress.hint")}
        </div>
      </div>
    </div>
  );
}
