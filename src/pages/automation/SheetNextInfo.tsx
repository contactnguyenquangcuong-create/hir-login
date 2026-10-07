import { useEffect, useState } from "react";
import { openPath } from "@tauri-apps/plugin-opener";
import {
  automationSheetCounts,
  automationSheetReset,
  specFor,
  type Block,
} from "../../entities/automation";
import { confirmModal } from "../../shared/lib/confirm";
import { toast } from "../../shared/lib/toast";
import { useT } from "../../shared/i18n";

/** Where `{{name}}` is used: the other steps with a parameter that contains it. */
function usedBy(name: string, self: string, steps: Block[], t: (k: string) => string): string[] {
  const safe = name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const needle = new RegExp(`\\{\\{\\s*${safe}\\s*\\}\\}`);
  const out: string[] = [];
  steps.forEach((b, i) => {
    if (b.id === self) return;
    const hit = Object.values(b.params).some((v) => typeof v === "string" && needle.test(v));
    if (hit) out.push(`${i + 1}. ${b.label || t(specFor(b.kind)?.label ?? b.kind)}`);
  });
  return out;
}

/** What a "next row from the Excel list" step does, said plainly: how far through the list
 *  it is, that the run ends by itself when the list does, which variables it hands to the
 *  steps after it and which of them use them, and where the per-row results go. */
export function SheetNextInfo({ block, steps }: { block: Block; steps: Block[] }) {
  const t = useT();
  const path = String(block.params.path ?? "").trim();
  const column = String(block.params.column ?? "").trim();
  const into = String(block.params.into ?? "").trim() || "link";
  const commentColumn = String(block.params.comment_column ?? "").trim();
  const commentInto = String(block.params.comment_into ?? "").trim() || "comment";
  const [counts, setCounts] = useState<{ total: number; left: number } | null>(null);
  const [tick, setTick] = useState(0);

  // Re-read now and then: a run going in the background takes rows while this is open.
  useEffect(() => {
    if (!path) { setCounts(null); return; }
    let alive = true;
    const read = () =>
      automationSheetCounts(path, column)
        .then((c) => alive && setCounts(c))
        .catch(() => alive && setCounts(null));
    void read();
    const timer = setInterval(read, 4000);
    return () => { alive = false; clearInterval(timer); };
  }, [path, column, tick]);

  const outputs = [
    { name: into, what: t("sheetInfo.outValue", { col: column || t("sheetInfo.firstColumn") }) },
    ...(commentColumn ? [{ name: commentInto, what: t("sheetInfo.outValue", { col: commentColumn }) }] : []),
  ];
  const resultsFile = path ? `${path}.results.csv` : "";
  const done = counts ? counts.total - counts.left : 0;

  const startOver = async () => {
    // Worded for what it does: the default red "Xoá" button reads like deleting the Excel file.
    const ok = await confirmModal({
      title: t("sheetInfo.startOver"),
      message: t("sheetInfo.resetConfirm"),
      buttons: [
        { label: t("sheetInfo.keepProgress"), value: false },
        { label: t("sheetInfo.startOver"), value: true, primary: true },
      ],
    });
    if (!ok) return;
    try {
      await automationSheetReset(path);
      setTick((n) => n + 1);
      toast.ok(t("sheetInfo.resetDone"));
    } catch (e) { toast.err(String(e)); }
  };

  return (
    <div className="flex flex-col gap-2 rounded-10 bg-bg-weak-50 p-2.5 text-paragraph-xs text-text-sub-600 ring-1 ring-inset ring-stroke-soft-200">
      <div className="text-label-xs text-text-strong-950">{t("sheetInfo.title")}</div>

      <p className="m-0">{t("sheetInfo.endsWhen")}</p>

      {counts && (
        <div className="flex flex-col gap-1">
          <div className="h-1.5 overflow-hidden rounded-full bg-stroke-soft-200">
            <div
              className="h-full rounded-full bg-primary-base"
              style={{ width: `${counts.total ? Math.round((done / counts.total) * 100) : 0}%` }}
            />
          </div>
          <div className="flex flex-wrap items-center justify-between gap-2">
            <span className="text-text-strong-950">
              {t("sheetInfo.counts", { total: counts.total, done, left: counts.left })}
            </span>
            {done > 0 && (
              <button
                type="button"
                className="text-primary-base hover:underline"
                onClick={() => void startOver()}
              >
                {t("sheetInfo.startOver")}
              </button>
            )}
          </div>
          {counts.left === 0 && counts.total > 0 && (
            <div className="rounded-8 bg-warning-alpha-16 px-2 py-1 text-warning-base">
              {t("sheetInfo.allDone")}
            </div>
          )}
        </div>
      )}

      <div className="flex flex-col gap-1.5 border-t border-stroke-soft-200 pt-2">
        <div className="text-subheading-2xs text-text-soft-400">{t("sheetInfo.wiring")}</div>
        {outputs.map((o) => {
          const users = usedBy(o.name, block.id, steps, t);
          return (
            <div key={o.name} className="flex flex-col gap-0.5">
              <div>
                <code className="rounded-6 bg-bg-white-0 px-1.5 py-0.5 text-[11px] text-text-strong-950">{`{{${o.name}}}`}</code>{" "}
                {o.what}
              </div>
              {users.length > 0 ? (
                <div className="pl-1">
                  {t("sheetInfo.usedBy")} {users.join(" · ")}
                </div>
              ) : (
                <div className="pl-1 text-warning-base">{t("sheetInfo.unused", { name: o.name })}</div>
              )}
            </div>
          );
        })}
      </div>

      {path && (
        <div className="flex flex-col gap-1 border-t border-stroke-soft-200 pt-2">
          <div className="text-subheading-2xs text-text-soft-400">{t("sheetInfo.results")}</div>
          <p className="m-0">{t("sheetInfo.resultsHelp")}</p>
          <code className="break-all text-[10px] text-text-soft-400">{resultsFile}</code>
          <button
            type="button"
            className="self-start text-primary-base hover:underline"
            onClick={() =>
              openPath(resultsFile).catch(() => toast.err(t("sheetInfo.noResultsYet")))
            }
          >
            {t("sheetInfo.openResults")}
          </button>
        </div>
      )}
    </div>
  );
}
