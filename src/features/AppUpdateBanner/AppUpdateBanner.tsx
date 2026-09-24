import { useEffect } from "react";
import { Button } from "@proxyshard/shardx-ui-kit";
import { DownloadIcon } from "../../shared/icons";
import { useAppUpdate } from "../../shared/model/appUpdate";
import { useT } from "../../shared/i18n";

/// Checked once per app launch; renders nothing until an update is actually
/// available (or being installed) so it never crowds the sidebar otherwise.
export function AppUpdateBanner() {
  const t = useT();
  const status = useAppUpdate((s) => s.status);
  const version = useAppUpdate((s) => s.version);
  const progress = useAppUpdate((s) => s.progress);
  const runCheck = useAppUpdate((s) => s.check);
  const install = useAppUpdate((s) => s.install);

  useEffect(() => { void runCheck(); }, [runCheck]);

  if (status === "idle" || status === "checking" || status === "error") return null;

  return (
    <div className="mb-2.5 flex flex-col gap-1.5 rounded-xl bg-primary-alpha-10 p-2.5 ring-1 ring-inset ring-primary-alpha-10">
      <div className="flex items-center gap-1.5 text-label-xs font-medium text-primary-base">
        <DownloadIcon className="size-3.5 shrink-0" />
        {status === "downloading"
          ? t("appUpdate.downloading", { pct: Math.round(progress * 100) })
          : t("appUpdate.available", { v: version ?? "" })}
      </div>
      {status === "available" && (
        <Button variant="primary" mode="lighter" size="xsmall" className="w-full" onClick={install}>
          {t("appUpdate.updateNow")}
        </Button>
      )}
    </div>
  );
}
