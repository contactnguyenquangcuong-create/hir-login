import { Button } from "@proxyshard/shardx-ui-kit";
import { DownloadIcon } from "../../../shared/icons";
import { useFingerprint } from "../../../entities/fingerprint";
import { useT } from "../../../shared/i18n";

export function ImportFromFolderButton() {
  const t = useT();
  const importJsonFolder = useFingerprint((s) => s.importJsonFolder);
  return (
    <Button
      variant="neutral"
      mode="stroke"
      size="small"
      leftIcon={<DownloadIcon className="size-4" />}
      onClick={importJsonFolder}
      title={t("importFromFolderButton.tooltip")}
    >
      {t("importFromFolderButton.label")}
    </Button>
  );
}
