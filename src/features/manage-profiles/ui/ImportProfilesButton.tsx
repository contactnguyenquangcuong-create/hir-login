import { Button } from "@proxyshard/shardx-ui-kit";
import { DownloadIcon } from "../../../shared/icons";
import { useProfile } from "../../../entities/profile";
import { useT } from "../../../shared/i18n";

export function ImportProfilesButton() {
  const t = useT();
  const importProfilesFromFolder = useProfile((s) => s.importProfilesFromFolder);
  return (
    <Button
      variant="neutral"
      mode="stroke"
      size="small"
      leftIcon={<DownloadIcon className="size-4" />}
      onClick={importProfilesFromFolder}
      title={t("importProfilesButton.tooltip")}
    >
      {t("importProfilesButton.label")}
    </Button>
  );
}
