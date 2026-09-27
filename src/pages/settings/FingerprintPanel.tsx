import { useEffect, useState } from "react";
import { Button, Input } from "@proxyshard/shardx-ui-kit";
import { useFingerprint } from "../../entities/fingerprint";
import { useTeam } from "../../shared/model/teamRole";
import { FingerprintImporter } from "../../features/manage-fingerprints";
import { FingerprintToolbar } from "../../widgets/FingerprintLibrary/FingerprintToolbar";
import { FingerprintLibrary } from "../../widgets/FingerprintLibrary/FingerprintLibrary";
import { Section, Block, Pill } from "./ui";

/** Admin-only: the shared GPU/device fingerprint library — not something staff
 *  need to see day to day, but someone has to be able to add or remove entries.
 *  Collapsed by default and capped in height: this library runs into the
 *  thousands, and left open inline it used to make the whole Settings page an
 *  endless scroll. */
export function FingerprintPanel() {
  const role = useTeam((s) => s.role);
  const init = useFingerprint((s) => s.init);
  const reload = useFingerprint((s) => s.reload);
  const importerOpen = useFingerprint((s) => s.importerOpen);
  const setImporterOpen = useFingerprint((s) => s.setImporterOpen);
  const count = useFingerprint((s) => s.items.length);
  const [open, setOpen] = useState(false);
  const [q, setQ] = useState("");
  useEffect(() => { if (role === "admin") init(); }, [role, init]);
  if (role !== "admin") return null;

  return (
    <Section
      title="Thư viện Fingerprint"
      desc="Vân tay GPU/thiết bị dùng khi tạo profile. Nhân sự không thấy mục này."
      action={<FingerprintToolbar />}
    >
      <Block>
        <div className="flex flex-wrap items-center gap-2">
          <Pill>{count} vân tay</Pill>
          <Button variant="neutral" mode="stroke" size="xsmall" onClick={() => setOpen((v) => !v)}>
            {open ? "Ẩn danh sách" : "Xem danh sách"}
          </Button>
        </div>
        {open && (
          <div className="flex flex-col gap-3">
            <Input inputSize="small" value={q} onChange={(e) => setQ(e.target.value)} placeholder="Tìm theo tên, hệ điều hành, GPU…" />
            <div className="max-h-[420px] overflow-y-auto rounded-lg bg-bg-weak-50 p-3">
              <FingerprintLibrary query={q} />
            </div>
          </div>
        )}
      </Block>
      {importerOpen && <FingerprintImporter onClose={() => { setImporterOpen(false); reload(); }} />}
    </Section>
  );
}
