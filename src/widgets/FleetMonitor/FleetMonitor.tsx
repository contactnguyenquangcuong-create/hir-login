import { useEffect, useState } from "react";
import { automationFleet, type RunState, type WorkerState } from "../../entities/automation";
import { useT } from "../../shared/i18n";

const TONE: Record<WorkerState["status"], string> = {
  queued: "text-text-soft-400",
  starting: "text-warning-base",
  running: "text-success-base",
  done: "text-text-sub-600",
  failed: "text-error-base",
  stopped: "text-text-soft-400",
};

/** A second window: every browser in every run, one row each. */
export function FleetMonitor() {
  const t = useT();
  const [runs, setRuns] = useState<RunState[]>([]);

  useEffect(() => {
    let alive = true;
    const tick = () => {
      automationFleet()
        .then((r) => { if (alive) setRuns(r); })
        .catch(() => {});
    };
    tick();
    const t = setInterval(tick, 700);
    return () => { alive = false; clearInterval(t); };
  }, []);

  const busy = runs.filter((r) => r.running);

  return (
    <div className="flex h-screen flex-col gap-2 bg-bg-white-0 p-3">
      <div className="text-label-sm text-text-strong-950">
        Fleet {busy.length > 0 && <span className="text-text-soft-400">· {busy.length} running</span>}
      </div>

      {runs.length === 0 ? (
        <p className="m-0 mt-6 text-center text-paragraph-sm text-text-soft-400">
          {t("fleetMonitor.emptyState")}
        </p>
      ) : (
        <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto">
          {runs.map((r) => (
            <div key={r.project_id} className="rounded-10 ring-1 ring-inset ring-stroke-soft-200">
              <div className="flex items-center justify-between border-b border-stroke-soft-200 px-2.5 py-1.5">
                <span className="truncate text-label-xs text-text-strong-950">{r.project_name}</span>
                <span className="text-paragraph-xs text-text-soft-400">
                  {r.running ? "running" : "finished"}
                </span>
              </div>
              {r.workers.map((w) => (
                <div
                  key={w.profile_id}
                  className="grid grid-cols-[1fr_64px_76px] items-center gap-2 px-2.5 py-1.5"
                >
                  <div className="min-w-0">
                    <div className="truncate text-paragraph-xs text-text-strong-950">
                      {w.profile_name}
                    </div>
                    {w.note && (
                      <div className="truncate text-paragraph-xs text-text-soft-400">{w.note}</div>
                    )}
                  </div>
                  <div className="text-paragraph-xs text-text-soft-400">
                    {w.steps_total > 0 ? `${w.step}/${w.steps_total}` : "—"}
                  </div>
                  <div className={`text-right text-paragraph-xs ${TONE[w.status]}`}>
                    {w.status}
                    {w.pass > 0 && <span className="text-text-soft-400"> ·{w.pass}</span>}
                  </div>
                </div>
              ))}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
