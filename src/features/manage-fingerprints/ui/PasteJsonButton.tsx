import { Button } from "@proxyshard/shardx-ui-kit";
import { AddIcon } from "../../../shared/icons";
import { useFingerprint } from "../../../entities/fingerprint";
import { useT } from "../../../shared/i18n";

export function PasteJsonButton() {
  const t = useT();
  const setImporterOpen = useFingerprint((s) => s.setImporterOpen);
  return (
    <Button variant="primary" mode="filled" size="small" leftIcon={<AddIcon className="size-4" />} onClick={() => setImporterOpen(true)}>
      {t("pasteJsonButton.label")}
    </Button>
  );
}
