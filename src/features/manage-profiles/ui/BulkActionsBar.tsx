import { Button } from "@proxyshard/shardx-ui-kit";
import {
  PlayIcon,
  StopIcon,
  UploadIcon,
  DeleteIcon,
  SyncIcon,
} from "../../../shared/icons";
import { useProfile, useSyncBlockReason } from "../../../entities/profile";
import { useT } from "../../../shared/i18n";

export function BulkActionsBar() {
  const t = useT();
  const count = useProfile((s) => s.selected.size);
  const bulkLaunch = useProfile((s) => s.bulkLaunch);
  const bulkLaunchSynced = useProfile((s) => s.bulkLaunchSynced);
  const bulkStop = useProfile((s) => s.bulkStop);
  const bulkExport = useProfile((s) => s.bulkExport);
  const bulkDelete = useProfile((s) => s.bulkDelete);
  const clearSelected = useProfile((s) => s.clearSelected);
  const syncBlocked = useSyncBlockReason();

  if (count === 0) return null;

  return (
    <div className="flex items-center gap-2 rounded-8 bg-primary-alpha-10 py-1 pl-3 pr-1 text-label-xs text-primary-base ring-1 ring-inset ring-primary-alpha-24">
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
      <Button variant="neutral" mode="stroke" className="pl-2" size="2xsmall" leftIcon={<UploadIcon className="size-3.5" />} onClick={bulkExport}>{t("bulkActionsBar.export")}</Button>
      <Button variant="error" mode="stroke" className="pr-2" size="2xsmall" leftIcon={<DeleteIcon className="size-3.5" />} onClick={bulkDelete}>{t("bulkActionsBar.delete")}</Button>
      <Button variant="neutral" mode="ghost" className="pr-2" size="2xsmall" onClick={clearSelected}>{t("bulkActionsBar.clear")}</Button>
    </div>
  );
}
