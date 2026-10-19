import { useEffect, useMemo, useState } from "react";
import { Checkbox, Pagination } from "@proxyshard/shardx-ui-kit";
import { ShardLogo } from "../../shared/icons";
import { useT } from "../../shared/i18n";
import { useContextMenu } from "../../shared/hooks/useContextMenu";
import {
  useProfile,
  useVisibleProfiles,
  useProxyMap,
  nextSort,
  parseSort,
  type SortKey,
} from "../../entities/profile";
import {
  ProfileInlineEditor,
  FromTemplateButton,
  NewProfileButton,
} from "../../features/manage-profiles";
import { ProfileRow } from "./ProfileRow";

const PAGE_SIZE = 20;

/** A column title that sorts: click for the order, again for the reverse, a third time to clear.
 *  The arrow shows the order in force (up = small/old/A first, down = big/new/Z first). */
function SortHead({ k, label, className }: { k: SortKey; label: string; className?: string }) {
  const t = useT();
  const sort = useProfile((s) => s.sort);
  const setSort = useProfile((s) => s.setSort);
  const cur = parseSort(sort);
  const dir = cur && cur.key === k ? cur.dir : null;
  return (
    <div className={className}>
      <button
        type="button"
        className={`group inline-flex cursor-pointer items-center gap-1 border-0 bg-transparent p-0 text-inherit uppercase hover:text-text-strong-950 ${dir ? "text-primary-base" : ""}`}
        title={t("profileTable.sortHint", { col: label })}
        aria-sort={dir === "asc" ? "ascending" : dir === "desc" ? "descending" : "none"}
        onClick={() => setSort(nextSort(sort, k))}
      >
        <span>{label}</span>
        <svg viewBox="0 0 10 14" className={`h-3 w-2 flex-none ${dir ? "" : "opacity-0 group-hover:opacity-50"}`} fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round">
          <path d="M5 12V2M2 5l3-3 3 3" className={dir === "desc" ? "opacity-25" : ""} />
          <path d="M5 2v10M2 9l3 3 3-3" className={dir === "asc" ? "opacity-25" : ""} />
        </svg>
      </button>
    </div>
  );
}

export function ProfileTable() {
  const t = useT();
  const selected = useProfile((s) => s.selected);
  const selectProfiles = useProfile((s) => s.selectProfiles);
  const expanded = useProfile((s) => s.expanded);
  const folder = useProfile((s) => s.folder);
  const search = useProfile((s) => s.search);
  const running = useProfile((s) => s.running);

  const visible = useVisibleProfiles();
  const proxyMap = useProxyMap();
  const ctx = useContextMenu();

  // Pagination of the (filtered) profile list.
  const [page, setPage] = useState(1);
  const pageCount = Math.max(1, Math.ceil(visible.length / PAGE_SIZE));
  // Reset to page 1 when the filter changes; clamp if the list shrank.
  useEffect(() => { setPage(1); }, [folder, search]);
  useEffect(() => { if (page > pageCount) setPage(pageCount); }, [pageCount, page]);
  const paged = useMemo(
    () => visible.slice((page - 1) * PAGE_SIZE, page * PAGE_SIZE),
    [visible, page],
  );

  // Re-render every second so the uptime label ticks without re-fetching the
  // process list (which polls every 2s in the store).
  const [, setUptimeTick] = useState(0);
  useEffect(() => {
    if (Object.keys(running).length === 0) return;
    const h = setInterval(() => setUptimeTick((t) => t + 1), 1000);
    return () => clearInterval(h);
  }, [running]);

  // Scroll the expanded editor into view after the expand animation.
  useEffect(() => {
    if (!expanded || expanded === "__new__") return;
    const t = setTimeout(() => {
      const el = document.querySelector<HTMLElement>(".row-wrap.row-expanded .inline-editor");
      el?.scrollIntoView({ behavior: "smooth", block: "center" });
    }, 60);
    return () => clearTimeout(t);
  }, [expanded]);

  // While a profile is being edited or made, only that is on the page: the list below it is
  // hidden and comes back when the form is saved or cancelled.
  const editing = expanded !== null;
  const rows = editing ? visible.filter((p) => p.id === expanded) : paged;

  const allPageSelected = paged.length > 0 && paged.every((p) => selected.has(p.id));
  const anyPageSelected = paged.some((p) => selected.has(p.id));

  return (
    <>
      <div className="overflow-clip rounded-lg bg-bg-white-0 shadow-[var(--shadow-xs)] ring-1 ring-inset ring-stroke-soft-200">
        {/* The column titles stay at the top of the page while the rows scroll under them. A sticky
            box is placed from the inner edge of the page's padding (1.5rem), hence the offset. */}
        {!editing && (
        <div className="t-cols t-head sticky top-[-1.5rem] z-10 border-b border-stroke-soft-200 bg-bg-weak-50 text-subheading-2xs text-text-soft-400">
          <div></div>
          <div>
            <Checkbox
              title={t("profileTable.selectAllOnPage")}
              // Header checkbox toggles only visible page rows; other pages preserved.
              checked={allPageSelected}
              indeterminate={anyPageSelected && !allPageSelected}
              onChange={(e) => selectProfiles(e.target.checked, paged)}
            />
          </div>
          <SortHead k="name" label={t("profileTable.colName")} />
          <SortHead k="status" label={t("profileTable.colStatus")} />
          <SortHead k="proxy" label={t("profileTable.colProxy")} />
          <SortHead k="notes" label={t("profileTable.colNotes")} />
          <SortHead k="runtime" label={t("profileTable.colTime")} className="head-time" />
          <SortHead k="created" label={t("profileTable.colCreated")} className="head-created" />
          <SortHead k="launched" label={t("profileTable.colLastRun")} className="head-lastrun" />
          <div></div>
        </div>
        )}
        {expanded === "__new__" && (
          <div className="row-expanded row-new relative border-t border-stroke-soft-200 first:border-t-0">
            <ProfileInlineEditor />
          </div>
        )}
        {rows.map((p) => (
          <ProfileRow
            key={p.id}
            profile={p}
            proxy={p.proxy_id ? proxyMap[p.proxy_id] ?? null : null}
            onMenu={ctx.open}
          />
        ))}
        {visible.length === 0 && !expanded && (
          <div className="flex flex-col items-center gap-2.5 px-6 py-14 text-center">
            <div className="grid size-14 place-items-center rounded-[14px] bg-primary-alpha-10 text-primary-base ring-1 ring-inset ring-primary-alpha-24">
              <ShardLogo />
            </div>
            <h3 className="m-0 text-label-sm text-text-strong-950">{t("profileTable.emptyTitle")}</h3>
            <p className="m-0 max-w-[420px] text-paragraph-sm text-text-sub-600">
              {t("profileTable.emptyHint")}
            </p>
            <div className="mt-2 flex gap-2">
              <FromTemplateButton />
              <NewProfileButton />
            </div>
          </div>
        )}
      </div>
      {pageCount > 1 && !editing && (
        <div className="flex items-center justify-center py-3 pb-1">
          <Pagination
            page={page}
            totalPages={pageCount}
            asLinks={false}
            onPageChange={setPage}
            infoLabel={(p, total) => t("profileTable.pageInfo", { page: p, total, count: visible.length })}
          />
        </div>
      )}
      {ctx.node}
    </>
  );
}
