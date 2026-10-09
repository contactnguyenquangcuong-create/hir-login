import { useEffect, useRef } from "react";
import { cn } from "@proxyshard/shardx-ui-kit";
import { useContextMenu } from "../../shared/hooks/useContextMenu";
import { useT } from "../../shared/i18n";
import { useProfile, useFolders } from "../../entities/profile";
import { useState } from "react";
import { useTeam, canEdit, canShare, startTeamRole } from "../../shared/model/teamRole";
import { ShareFolderModal } from "../../features/manage-profiles/ui/ShareFolderModal";

/* UI-kit "line" tab look, hand-rolled because tabs are drop targets too. */
const tabBase =
  "relative -mb-px flex flex-none cursor-pointer items-center gap-1.5 whitespace-nowrap border-0 border-b-2 bg-transparent px-3.5 py-2 text-label-xs transition-colors pointer-events-auto [&>*]:pointer-events-none";
const tabActive = "border-b-primary-base text-text-strong-950";
const tabIdle = "border-b-transparent text-text-sub-600 hover:text-text-strong-950";
const tabDrop = "bg-primary-alpha-10! text-primary-base! outline outline-1 outline-dashed outline-primary-base";
// With many folders the tabs wrap onto rows of chips inside a box of limited height, which scrolls
// and can be searched, instead of running off the right edge where most of them were hidden.
const chipBase =
  "relative flex flex-none cursor-pointer items-center gap-1.5 whitespace-nowrap rounded-full border px-3 py-1 text-label-xs transition-colors pointer-events-auto [&>*]:pointer-events-none";
const chipActive = "border-primary-base bg-primary-alpha-10 text-primary-base";
const chipIdle = "border-stroke-soft-200 bg-transparent text-text-sub-600 hover:text-text-strong-950";
/** More folders than this and the tabs turn into wrapped chips (a row of tabs ran under the
 *  toolbar beside it and the last ones were hidden); more than `SEARCHABLE_FOLDERS`, and there is
 *  a search box above them. */
const MANY_FOLDERS = 3;
const SEARCHABLE_FOLDERS = 8;
const badge = (active: boolean) =>
  cn(
    "rounded-full px-1.5 py-px text-[10px] font-semibold",
    active ? "bg-primary-alpha-10 text-primary-base" : "bg-bg-weak-50 text-text-sub-600",
  );

const readDragId = (e: React.DragEvent) =>
  e.dataTransfer.getData("application/x-shardx-profile") || e.dataTransfer.getData("text/plain");

export function FolderTabs() {
  const t = useT();
  const profiles = useProfile((s) => s.profiles);
  const folder = useProfile((s) => s.folder);
  const dropTarget = useProfile((s) => s.dropTarget);
  const setFolder = useProfile((s) => s.setFolder);
  const setDropTarget = useProfile((s) => s.setDropTarget);
  const setProfileFolder = useProfile((s) => s.setProfileFolder);
  const deleteFolder = useProfile((s) => s.deleteFolder);
  const folders = useFolders();
  const ctx = useContextMenu();
  const role = useTeam((s) => s.role);
  const configured = useTeam((s) => s.configured);
  const [sharing, setSharing] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const folderNames = useTeam((s) => s.folderNames);
  const registry = useProfile((s) => s.folderRegistry);
  const forgetFolder = useProfile((s) => s.forgetFolder);
  // A folder the server no longer shows this person (deleted, or access removed) goes from the
  // list here as well; only empty ones, so no profile is ever hidden by this.
  useEffect(() => {
    if (folderNames === null) return;
    for (const f of registry) {
      if (!folderNames.includes(f) && !profiles.some((p) => p.folder === f)) forgetFolder(f);
    }
  }, [folderNames, registry, profiles, forgetFolder]);
  useEffect(() => { startTeamRole(); }, []);

  // Native non-passive wheel handler turns vertical scroll into horizontal tab scroll.
  const folderTabsRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const el = folderTabsRef.current;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      if (el.scrollWidth <= el.clientWidth || e.deltaY === 0) return;
      e.preventDefault();
      el.scrollLeft += e.deltaY;
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  }, []);

  // Fall back to "all" when the active folder tab becomes empty.
  useEffect(() => {
    if (folder !== "all" && !folders.includes(folder)) setFolder("all");
  }, [folders, folder, setFolder]);

  const many = folders.length > MANY_FOLDERS;
  const searchable = folders.length > SEARCHABLE_FOLDERS;
  const needle = searchable ? query.trim().toLowerCase() : "";
  // The open folder stays in the list even when the search would hide it.
  const shown = !needle ? folders : folders.filter((f) => f === folder || f.toLowerCase().includes(needle));
  const base = many ? chipBase : tabBase;
  const on = many ? chipActive : tabActive;
  const off = many ? chipIdle : tabIdle;
  const drop = many ? "border-primary-base! bg-primary-alpha-10! text-primary-base! border-dashed" : tabDrop;

  const dropProps = (key: string, folderName: string) => ({
    // Unconditional preventDefault on dragover is the *only* way HTML5 marks
    // the element as a valid drop target — the preventDefault itself must
    // fire on every event or `drop` never lands.
    onDragOver: (e: React.DragEvent) => {
      e.preventDefault();
      e.dataTransfer.dropEffect = "move";
      if (dropTarget !== key) setDropTarget(key);
    },
    // Ignore enter-into-child events: relatedTarget will be a descendant
    // of the button, so the drag is still over us.
    onDragLeave: (e: React.DragEvent<HTMLElement>) => {
      if (!e.currentTarget.contains(e.relatedTarget as Node)) setDropTarget(null);
    },
    onDrop: (e: React.DragEvent) => {
      e.preventDefault();
      setDropTarget(null);
      const id = readDragId(e);
      if (id) setProfileFolder(id, folderName); // "" = unassign folder
    },
  });

  return (
    <div className="flex min-w-0 flex-col gap-1.5">
      {searchable && (
        <div className="flex items-center gap-2">
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={`Tìm trong ${folders.length} thư mục…`}
            className="h-8 w-56 rounded-8 bg-bg-white-0 px-2.5 text-paragraph-sm text-text-strong-950 ring-1 ring-inset ring-stroke-soft-200 outline-none placeholder:text-text-soft-400 focus:ring-primary-base"
          />
          {needle && (
            <span className="text-paragraph-xs text-text-soft-400">
              {folders.filter((f) => f.toLowerCase().includes(needle)).length}/{folders.length}
            </span>
          )}
        </div>
      )}
      <div
        className={cn(
          "flex min-w-0 border-b border-stroke-soft-200",
          many
            ? "max-h-[88px] flex-wrap content-start gap-1.5 overflow-y-auto border-b-0 pb-1"
            : "overflow-x-auto [scrollbar-width:none] [&::-webkit-scrollbar]:hidden",
        )}
        ref={folderTabsRef}
      >
        <button
          className={cn(base, folder === "all" ? on : off, dropTarget === "__all__" && drop)}
          onClick={() => setFolder("all")}
          {...dropProps("__all__", "")}
        >
          {t("folderTabs.allTab")}<span className={badge(folder === "all")}>{profiles.length}</span>
        </button>
        {shown.map((f) => (
          <button
            key={f}
            className={cn(base, folder === f ? on : off, dropTarget === f && drop)}
            onClick={() => setFolder(f)}
            title={t("folderTabs.folderTabHint")}
            onContextMenu={(e) =>
              ctx.open(e, [
                ...(canShare(role, configured) ? [{ label: "Chia sẻ quyền…", onClick: () => setSharing(f) }] : []),
                ...(canEdit(role, configured) ? [{ label: t("folderTabs.deleteFolder"), onClick: () => deleteFolder(f), danger: true }] : []),
              ])
            }
            {...dropProps(f, f)}
          >
            {f}
            <span className={badge(folder === f)}>
              {profiles.filter((p) => p.folder === f).length}
            </span>
          </button>
        ))}
        {needle && shown.length === 0 && (
          <span className="px-2 py-1 text-paragraph-xs text-text-soft-400">Không có thư mục nào khớp “{query}”.</span>
        )}
        {canShare(role, configured) && folder !== "all" && (
          <button
            className="ml-auto flex-none cursor-pointer whitespace-nowrap border-0 bg-transparent px-3 py-2 text-label-xs text-text-sub-600 hover:text-primary-base"
            title="Chọn ai được dùng thư mục này"
            onClick={() => setSharing(folder)}
          >
            Chia sẻ quyền
          </button>
        )}
      </div>
      {sharing && <ShareFolderModal folder={sharing} onClose={() => setSharing(null)} />}
      {ctx.node}
    </div>
  );
}
