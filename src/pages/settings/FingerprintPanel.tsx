import { useEffect } from "react";
import { useFingerprint } from "../../entities/fingerprint";
import { useTeam } from "../../shared/model/teamRole";
import { FingerprintImporter } from "../../features/manage-fingerprints";
import { FingerprintToolbar } from "../../widgets/FingerprintLibrary/FingerprintToolbar";
import { FingerprintLibrary } from "../../widgets/FingerprintLibrary/FingerprintLibrary";
import { Section, Block } from "./ui";

/** Admin-only: the shared GPU/device fingerprint library — not something staff
 *  need to see day to day, but someone has to be able to add or remove entries. */
export function FingerprintPanel() {
  const role = useTeam((s) => s.role);
  const init = useFingerprint((s) => s.init);
  const reload = useFingerprint((s) => s.reload);
  const importerOpen = useFingerprint((s) => s.importerOpen);
  const setImporterOpen = useFingerprint((s) => s.setImporterOpen);
  useEffect(() => { if (role === "admin") init(); }, [role, init]);
  if (role !== "admin") return null;

  return (
    <Section title="Thư viện Fingerprint" desc="Vân tay GPU/thiết bị dùng khi tạo profile. Nhập thêm hoặc bớt tại đây; nhân sự không thấy mục này." action={<FingerprintToolbar />}>
      <Block><FingerprintLibrary /></Block>
      {importerOpen && <FingerprintImporter onClose={() => { setImporterOpen(false); reload(); }} />}
    </Section>
  );
}
