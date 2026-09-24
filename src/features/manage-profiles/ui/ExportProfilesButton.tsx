import { Button } from "@proxyshard/shardx-ui-kit";
import { UploadIcon } from "../../../shared/icons";
import { useProfile } from "../../../entities/profile";
import { useT } from "../../../shared/i18n";

export function ExportProfilesButton() {
  const t = useT();
  const exportProfilesToFolder = useProfile((s) => s.exportProfilesToFolder);
  return (
    <Button
      variant="neutral"
      mode="stroke"
      size="small"
      leftIcon={<UploadIcon className="size-4" />}
      onClick={exportProfilesToFolder}
      title={t("exportProfilesButton.tooltip")}
    >
      {t("exportProfilesButton.label")}
    </Button>
  );
}
