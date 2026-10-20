import { useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { Button, DialogModal, Select } from "@proxyshard/shardx-ui-kit";
import { toast } from "../../../shared/model/toast";
import { useT } from "../../../shared/i18n";
import {
  useProfile,
  cookiesImportMany,
  cookiesListFiles,
  matchCookieFiles,
  type CookieFile,
  type CookieImportRow,
  type MatchHow,
} from "../../../entities/profile";

const baseName = (p: string) => p.replace(/^.*[/\\]/, "");

/** Cookie files → the selected profiles. Pick a folder (or files), check which goes where, import. */
export function BulkCookieImportModal({ onClose }: { onClose: () => void }) {
  const t = useT();
  const selected = useProfile((s) => s.selected);
  const profiles = useProfile((s) => s.profiles);
  const running = useProfile((s) => s.running);
  const targets = useMemo(() => profiles.filter((p) => selected.has(p.id)), [profiles, selected]);

  const [files, setFiles] = useState<CookieFile[]>([]);
  const [pick, setPick] = useState<Record<string, string>>({}); // profile id → file path ("" = skip)
  const [how, setHow] = useState<Record<string, MatchHow>>({});
  const [busy, setBusy] = useState(false);
  const [results, setResults] = useState<CookieImportRow[] | null>(null);

  // Running profiles are not offered: the browser would write its own database back over the import.
  const free = useMemo(() => targets.filter((p) => !running[p.id]), [targets, running]);

  const adopt = (list: CookieFile[]) => {
    setFiles(list);
    const m = matchCookieFiles(free, list);
    setPick(Object.fromEntries(free.map((p) => [p.id, m[p.id]?.path ?? ""])));
    setHow(Object.fromEntries(free.flatMap((p) => (m[p.id] ? [[p.id, m[p.id]!.how] as const] : []))));
    setResults(null);
  };

  const chooseFolder = async () => {
    try {
      const dir = await open({ directory: true, title: t("bulkCookies.pickFolder") });
      if (!dir || Array.isArray(dir)) return;
      const list = await cookiesListFiles(dir);
      if (list.length === 0) { toast.info(t("bulkCookies.noFiles")); return; }
      adopt(list);
    } catch (e) { toast.err(String(e)); }
  };

  const chooseFiles = async () => {
    try {
      const picked = await open({
        multiple: true,
        directory: false,
        title: t("bulkCookies.pickFiles"),
        filters: [{ name: "Cookies", extensions: ["json", "txt"] }],
      });
      if (!picked) return;
      const paths = Array.isArray(picked) ? picked : [picked];
      if (paths.length === 0) return;
      adopt(paths.map((p) => ({ path: p, name: baseName(p), size: 0 })));
    } catch (e) { toast.err(String(e)); }
  };

  const chosen = free.filter((p) => pick[p.id]);
  const options = useMemo(
    () => [{ value: "", label: t("bulkCookies.skip") }, ...files.map((f) => ({ value: f.path, label: f.name }))],
    [files, t],
  );

  const run = async () => {
    if (chosen.length === 0 || busy) return;
    setBusy(true);
    try {
      const rows = await cookiesImportMany(chosen.map((p) => ({ id: p.id, path: pick[p.id] })));
      setResults(rows);
      const ok = rows.filter((r) => !r.error);
      const bad = rows.filter((r) => r.error);
      if (ok.length > 0) toast.ok(t("bulkCookies.done", { n: ok.reduce((s, r) => s + r.count, 0), p: ok.length }));
      if (bad.length > 0) toast.err(t("bulkCookies.failed", { n: bad.length }));
    } catch (e) { toast.err(String(e)); }
    finally { setBusy(false); }
  };

  const nameOf = (id: string) => targets.find((p) => p.id === id)?.name ?? id.slice(0, 8);
  const hint = (id: string) =>
    how[id] === "order" ? t("bulkCookies.byOrder") : how[id] === "name" ? t("bulkCookies.byName") : how[id] === "id" ? t("bulkCookies.byId") : "";

  return (
    <DialogModal
      open
      onClose={onClose}
      title={t("bulkCookies.title", { n: targets.length })}
      confirmLabel={results ? t("bulkCookies.close") : t("bulkCookies.importN", { n: chosen.length })}
      onConfirm={results ? onClose : run}
      isDisabled={!results && chosen.length === 0}
      isLoading={busy}
      cancelLabel={t("bulkCookies.cancel")}
      onCancel={onClose}
      hideCancel={!!results}
      maxWidthClassName="w-[640px] max-w-[calc(100vw-2rem)]"
    >
      <div className="flex flex-col gap-3 py-4">
        <p className="m-0 text-paragraph-xs text-text-soft-400">{t("bulkCookies.intro")}</p>
        <div className="flex flex-wrap gap-2">
          <Button variant="primary" mode="stroke" size="small" onClick={chooseFolder} disabled={busy}>{t("bulkCookies.pickFolder")}</Button>
          <Button variant="neutral" mode="stroke" size="small" onClick={chooseFiles} disabled={busy}>{t("bulkCookies.pickFiles")}</Button>
        </div>

        {files.length > 0 && !results && (
          <>
            <div className="text-paragraph-xs text-text-sub-600">
              {t("bulkCookies.summary", { m: chosen.length, p: free.length, f: files.length })}
              {targets.length > free.length && ` · ${t("bulkCookies.runningSkipped", { n: targets.length - free.length })}`}
            </div>
            <div className="flex max-h-[340px] flex-col divide-y divide-stroke-soft-200 overflow-y-auto rounded-lg border border-stroke-soft-200">
              {free.map((p) => (
                <div key={p.id} className="flex items-center gap-3 px-3 py-2">
                  <div className="flex min-w-0 flex-1 flex-col">
                    <span className="truncate text-label-xs text-text-strong-950">{p.name}</span>
                    <span className="text-paragraph-xs text-text-soft-400">{hint(p.id) || (p.folder ? p.folder : "")}</span>
                  </div>
                  <div className="w-64 flex-none">
                    <Select
                      value={pick[p.id] ?? ""}
                      onChange={(v) => { setPick({ ...pick, [p.id]: String(v) }); setHow((h) => { const n = { ...h }; delete n[p.id]; return n; }); }}
                      options={options}
                      disabled={busy}
                    />
                  </div>
                </div>
              ))}
            </div>
          </>
        )}

        {results && (
          <div className="flex max-h-[340px] flex-col divide-y divide-stroke-soft-200 overflow-y-auto rounded-lg border border-stroke-soft-200">
            {results.map((r) => (
              <div key={r.id} className="flex items-center gap-3 px-3 py-2">
                <span className="min-w-0 flex-1 truncate text-label-xs text-text-strong-950">{nameOf(r.id)}</span>
                <span className={`flex-none text-paragraph-xs ${r.error ? "text-error-base" : "text-success-base"}`} title={r.error ?? undefined}>
                  {r.error ? r.error : t("bulkCookies.imported", { n: r.count })}
                </span>
              </div>
            ))}
          </div>
        )}
      </div>
    </DialogModal>
  );
}
