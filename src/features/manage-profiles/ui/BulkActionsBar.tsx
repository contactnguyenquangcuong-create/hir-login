import { useState } from "react";
import { Button } from "@proxyshard/shardx-ui-kit";
import {
  PlayIcon,
  StopIcon,
  DeleteIcon,
  SyncIcon,
  FolderIcon,
} from "../../../shared/icons";
import { useProfile, useSyncBlockReason } from "../../../entities/profile";
import { useT } from "../../../shared/i18n";
import { canEdit, useTeam } from "../../../shared/model/teamRole";
import { BulkCookieImportModal } from "./BulkCookieImportModal";

export function BulkActionsBar() {
  const t = useT();
  const count = useProfile((s) => s.selected.size);
  const bulkLaunch = useProfile((s) => s.bulkLaunch);
  const bulkLaunchSynced = useProfile((s) => s.bulkLaunchSynced);
  const bulkStop = useProfile((s) => s.bulkStop);
  const bulkDelete = useProfile((s) => s.bulkDelete);
  const bulkAndroidToDesktop = useProfile((s) => s.bulkAndroidToDesktop);
  const bulkUnbindProxy = useProfile((s) => s.bulkUnbindProxy);
  const bulkExportCookies = useProfile((s) => s.bulkExportCookies);
  const [importing, setImporting] = useState(false);
  // Only offered when the selection has a profile that actually has a proxy.
  const proxiedCount = useProfile((s) => s.profiles.filter((p) => s.selected.has(p.id) && !!p.proxy_id).length);
  // Only offered when the selection really contains a phone profile.
  const androidCount = useProfile((s) => s.profiles.filter((p) => s.selected.has(p.id) && p.mobile).length);
  const role = useTeam((s) => s.role);
  const configured = useTeam((s) => s.configured);
  const clearSelected = useProfile((s) => s.clearSelected);
  const setFolderModal = useProfile((s) => s.setFolderModal);
  const selectedIds = useProfile((s) => s.selected);
  const syncBlocked = useSyncBlockReason();

  if (count === 0) return null;

  return (
    <div className="flex flex-wrap items-center gap-2 rounded-8 bg-primary-alpha-10 py-1 pl-3 pr-1 text-label-xs text-primary-base ring-1 ring-inset ring-primary-alpha-24">
      <span>{t("bulkActionsBar.selectedCount", { n: count })}</span>
      <Button variant="neutral" mode='stroke' className="pr-4" size="2xsmall" leftIcon={<PlayIcon className="size-3.5" />} onClick={bulkLaunch}>{t("bulkActionsBar.launch")}</Button>
      {/* Meaningless for a single profile, so it only appears for a group. */}
      {count >= 2 && (
        // The tooltip sits on a wrapper: the kit gives a disabled Button
        // pointer-events-none, so a title on the button itself never shows —
        // which is exactly when the operator most needs to read it.
        <span title={syncBlocked || t("bulkActionsBar.launchSyncedHint")}>
          <Button
            variant="neutral"
            mode="stroke"
            className="pr-4"
            size="2xsmall"
            disabled={!!syncBlocked}
            leftIcon={<SyncIcon className="size-3.5" />}
            onClick={bulkLaunchSynced}
          >
            {t("bulkActionsBar.launchSynced")}
          </Button>
        </span>
      )}
      <Button variant="neutral" mode="stroke" className="pr-2" size="2xsmall" leftIcon={<StopIcon className="size-3.5" />} onClick={bulkStop}>{t("bulkActionsBar.stop")}</Button>
      {/* Offered to everyone, staff included: the server lets someone with only "use"
          access turn a proxy off (and nothing else). Always shown — hidden when nothing
          selected has a proxy it looked like the feature was missing — and it explains
          itself when there is nothing to do. */}
      <Button variant="neutral" mode="stroke" className="pr-2" size="2xsmall" onClick={bulkUnbindProxy}>
        {proxiedCount > 0 ? t("bulkActionsBar.noProxy", { n: proxiedCount }) : t("bulkActionsBar.noProxyAll")}
      </Button>
      {canEdit(role, configured) && (
        <Button
          variant="neutral"
          mode="stroke"
          className="pr-2"
          size="2xsmall"
          leftIcon={<FolderIcon className="size-3.5" />}
          onClick={() => setFolderModal({ profileId: null, profileIds: [...selectedIds] })}
        >
          Chuyển thư mục
        </Button>
      )}
      <Button variant="neutral" mode="stroke" className="pr-2" size="2xsmall" onClick={bulkExportCookies}>{t("bulkActionsBar.exportCookies")}</Button>
      <Button variant="neutral" mode="stroke" className="pr-2" size="2xsmall" onClick={() => setImporting(true)}>{t("bulkActionsBar.importCookies")}</Button>
      {androidCount > 0 && canEdit(role, configured) && (
        <Button variant="neutral" mode="stroke" className="pr-2" size="2xsmall" onClick={bulkAndroidToDesktop}>
          Đổi {androidCount} Android sang máy tính
        </Button>
      )}
      <Button variant="error" mode="stroke" className="pr-2" size="2xsmall" leftIcon={<DeleteIcon className="size-3.5" />} onClick={bulkDelete}>{t("bulkActionsBar.delete")}</Button>
      <Button variant="neutral" mode="ghost" className="pr-2" size="2xsmall" onClick={clearSelected}>{t("bulkActionsBar.clear")}</Button>
      {importing && <BulkCookieImportModal onClose={() => setImporting(false)} />}
    </div>
  );
}
