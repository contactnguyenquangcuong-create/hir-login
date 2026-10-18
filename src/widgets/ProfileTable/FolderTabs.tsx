import { useEffect, useMemo, useState } from "react";
import { cn } from "@proxyshard/shardx-ui-kit";
import { useContextMenu } from "../../shared/hooks/useContextMenu";
import { useT } from "../../shared/i18n";
import { useProfile, useFolders, useHirsShare } from "../../entities/profile";
import { toast } from "../../shared/model/toast";
import { useTeam, canEdit, canShare, startTeamRole } from "../../shared/model/teamRole";
import { FolderListButton, inlineFolders } from "../../shared/ui/FolderListButton";
import { ShareFolderModal } from "../../features/manage-profiles/ui/ShareFolderModal";

/* UI-kit "line" tab look, hand-rolled because tabs are drop targets too. */
const tabBase =
  "relative -mb-px flex flex-none cursor-pointer items-center gap-1.5 whitespace-nowrap border-0 border-b-2 bg-transparent px-3.5 py-2 text-label-xs transition-colors pointer-events-auto [&>*]:pointer-events-none";
const tabActive = "border-b-primary-base text-text-strong-950";
const tabIdle = "border-b-transparent text-text-sub-600 hover:text-text-strong-950";
const tabDrop = "bg-primary-alpha-10! text-primary-base! outline outline-1 outline-dashed outline-primary-base";
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
  // HirS is shared with like a person, in the Chia sẻ dialog; only shown while it may connect.
  const hirsAllowed = useHirsShare((s) => s.allowed);
  const hirsAccess = useHirsShare((s) => s.access);
  const hirs = hirsAllowed ? Object.keys(hirsAccess) : [];
  const loadHirs = useHirsShare((s) => s.load);
  const setHirs = useHirsShare((s) => s.set);
  useEffect(() => { startTeamRole(); loadHirs(); }, [loadHirs]);
  const flipHirs = async (f: string) => {
    const on = !hirs.includes(f);
    try { await setHirs(f, on ? "use" : "none"); toast.ok(t(on ? "folderTabs.hirsOn" : "folderTabs.hirsOff", { folder: f })); }
    catch (e) { toast.err(String(e)); }
  };

  // One pass over the profiles instead of one per folder: with a thousand folders the per-tab
  // `filter` was a thousand passes on every render.
  const counts = useMemo(() => {
    const m = new Map<string, number>();
    for (const p of profiles) if (p.folder) m.set(p.folder, (m.get(p.folder) ?? 0) + 1);
    return m;
  }, [profiles]);

  // Fall back to "all" when the active folder tab becomes empty.
  useEffect(() => {
    if (folder !== "all" && !folders.includes(folder)) setFolder("all");
  }, [folders, folder, setFolder]);

  // The tabs in the row: the first few, and the open folder wherever it is in the list.
  const inline = useMemo(() => inlineFolders(folders, folder === "all" ? "" : folder), [folders, folder]);
  const hidden = folders.length - inline.length;

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

  const folderMenu = (e: React.MouseEvent, f: string) =>
    ctx.open(e, [
      ...(canShare(role, configured) ? [{ label: "Chia sẻ quyền…", onClick: () => setSharing(f) }] : []),
      // A team member has no Chia sẻ dialog, yet still decides what the HirS on their own machine
      // may touch.
      ...(!canShare(role, configured) && hirsAllowed
        ? [{ label: hirs.includes(f) ? `✓ ${t("folderTabs.unshareWithHirs")}` : t("folderTabs.shareWithHirs"), onClick: () => flipHirs(f) }]
        : []),
      ...(canEdit(role, configured) ? [{ label: t("folderTabs.deleteFolder"), onClick: () => deleteFolder(f), danger: true }] : []),
    ]);

  return (
    <div className="flex min-w-0 items-center overflow-x-auto border-b border-stroke-soft-200 [scrollbar-width:none] [&::-webkit-scrollbar]:hidden">
      <button
        className={cn(tabBase, folder === "all" ? tabActive : tabIdle, dropTarget === "__all__" && tabDrop)}
        onClick={() => setFolder("all")}
        {...dropProps("__all__", "")}
      >
        {t("folderTabs.allTab")}<span className={badge(folder === "all")}>{profiles.length}</span>
      </button>
      {inline.map((f) => (
        <button
          key={f}
          className={cn(tabBase, "max-w-[160px]", folder === f ? tabActive : tabIdle, dropTarget === f && tabDrop)}
          onClick={() => setFolder(f)}
          title={`${f} — ${t("folderTabs.folderTabHint")}${hirs.includes(f) ? `\n${t("folderTabs.hirsShared")}` : ""}`}
          onContextMenu={(e) => folderMenu(e, f)}
          {...dropProps(f, f)}
        >
          <span className="truncate">{f}</span>
          {hirs.includes(f) && <span className="size-1.5 flex-none rounded-full bg-primary-base" />}
          <span className={badge(folder === f)}>{counts.get(f) ?? 0}</span>
        </button>
      ))}
      {/* After the folders it holds the rest of: "the other folders", at the end of the row. */}
      {hidden > 0 && (
        <FolderListButton
          folders={folders}
          value={folder}
          onPick={setFolder}
          counts={counts}
          label={t("folderTabs.moreFolders")}
          count={hidden}
          className={cn(tabBase, "border-b-transparent text-primary-base hover:text-primary-base")}
        />
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
      {sharing && <ShareFolderModal folder={sharing} onClose={() => setSharing(null)} />}
      {ctx.node}
    </div>
  );
}
