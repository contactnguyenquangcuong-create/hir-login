import { useEffect, useState } from "react";
import { Button } from "@proxyshard/shardx-ui-kit";
import { Topbar } from "../../shared/ui/Topbar";
import {
  AddIcon,
  CopyIcon,
  UploadIcon,
  DeleteIcon,
  NavAutomationIcon,
} from "../../shared/icons";
import { fmtTs } from "../../shared/lib/utils";
import { useStoreChanged } from "../../shared/hooks/useStoreChanged";
import { automationImport, useAutomation, type Bundle } from "../../entities/automation";
import { toast } from "../../shared/lib/toast";
import { ProjectEditor } from "./ProjectEditor";
import { ModulesCard } from "../../features/automation-modules";
import { useT } from "../../shared/i18n";

export function AutomationPage() {
  const t = useT();
  const init = useAutomation((s) => s.init);
  const status = useAutomation((s) => s.status);
  const available = useAutomation((s) => s.available);
  const projects = useAutomation((s) => s.projects);
  const openId = useAutomation((s) => s.openId);
  const busy = useAutomation((s) => s.busy);
  const open = useAutomation((s) => s.open);
  const create = useAutomation((s) => s.create);
  const remove = useAutomation((s) => s.remove);
  const duplicate = useAutomation((s) => s.duplicate);
  const reload = useAutomation((s) => s.reload);

  const [name, setName] = useState("");
  const crumbs = [t("automation.crumbWorkspace"), t("automation.crumbAutomation")];

  useEffect(() => { init(); }, [init]);
  // A project created or removed through the HTTP API or MCP writes straight
  // to disk; without this the list only catches up on restart.
  useStoreChanged(reload);

  if (status === "ready" && !available) {
    return (
      <section className="flex flex-col">
        <Topbar crumbs={crumbs} search="" onSearch={() => {}} />
        <div className="rounded-12 bg-bg-white-0 p-8 text-center shadow-[var(--shadow-xs)] ring-1 ring-inset ring-stroke-soft-200">
          <p className="m-0 text-paragraph-sm text-text-soft-400">
            {t("automation.notBuilt")}
          </p>
        </div>
      </section>
    );
  }

  if (openId) return <ProjectEditor />;

  const submit = async () => {
    const n = name.trim();
    if (!n) return;
    setName("");
    await create(n);
  };

  return (
    <section className="flex flex-col">
      <Topbar crumbs={crumbs} search="" onSearch={() => {}} />

      <div className="mb-3.5 flex items-end justify-between gap-4">
        <div>
          <h1 className="m-0 text-title-h5 text-text-strong-950">{t("automation.title")}</h1>
          <p className="m-0 mt-1 max-w-[70ch] text-paragraph-xs text-text-soft-400">
            {t("automation.intro")}
          </p>
        </div>
      </div>

      <div className="mb-3.5 flex items-center gap-2">
        <input
          className="h-9 w-[280px] rounded-10 bg-bg-white-0 px-3 text-paragraph-sm text-text-strong-950 ring-1 ring-inset ring-stroke-soft-200 outline-none placeholder:text-text-soft-400 focus:ring-primary-base"
          placeholder={t("automation.namePlaceholder")}
          value={name}
          onChange={(e) => setName(e.target.value)}
          onKeyDown={(e) => { if (e.key === "Enter") submit(); }}
        />
        <Button
          variant="primary" mode="filled" size="small"
          leftIcon={<AddIcon className="size-4" />}
          disabled={!name.trim()}
          onClick={submit}
        >
          {t("automation.create")}
        </Button>
        <label className="cursor-pointer">
          <input
            type="file"
            accept=".json"
            className="hidden"
            onChange={async (e) => {
              const file = e.target.files?.[0];
              e.target.value = "";
              if (!file) return;
              try {
                const bundle = JSON.parse(await file.text()) as Bundle;
                const p = await automationImport(bundle);
                await init();
                open(p.id);
                const n = bundle.needs?.length ?? 0;
                toast.ok(
                  n === 0
                    ? t("automation.imported")
                    : n === 1
                      ? t("automation.importedSecret", { n })
                      : t("automation.importedSecrets", { n }),
                );
              } catch (err) { toast.err(String(err)); }
            }}
          />
          <span className="inline-flex h-8 items-center gap-1.5 rounded-10 px-3 text-label-sm text-text-sub-600 ring-1 ring-inset ring-stroke-soft-200 hover:bg-bg-weak-50">
            <UploadIcon className="size-4" />
            {t("automation.import")}
          </span>
        </label>
      </div>

      <div className="overflow-hidden rounded-12 bg-bg-white-0 shadow-[var(--shadow-xs)] ring-1 ring-inset ring-stroke-soft-200">
        {projects.length > 0 && (
          <div className="grid grid-cols-[1fr_100px_160px_140px] items-center gap-3 border-b border-stroke-soft-200 bg-bg-weak-50 px-4 py-2 text-subheading-2xs text-text-soft-400">
            <div>{t("automation.colName")}</div>
            <div>{t("automation.colSteps")}</div>
            <div>{t("automation.colUpdated")}</div>
            <div />
          </div>
        )}

        {projects.length === 0 ? (
          <div className="flex flex-col items-center gap-2 px-4 py-12 text-center">
            <span className="text-icon-soft-400"><NavAutomationIcon className="size-7" /></span>
            <p className="m-0 text-paragraph-sm text-text-soft-400">
              {t("automation.emptyState")}
            </p>
          </div>
        ) : (
          projects.map((p) => (
            <div
              key={p.id}
              className="grid cursor-pointer grid-cols-[1fr_100px_160px_140px] items-center gap-3 border-b border-stroke-soft-200 px-4 py-2.5 last:border-b-0 hover:bg-bg-weak-50"
              onClick={() => open(p.id)}
            >
              <div className="min-w-0">
                <div className="truncate text-label-sm text-text-strong-950">{p.name}</div>
                {p.notes && (
                  <div className="truncate text-paragraph-xs text-text-soft-400">{p.notes}</div>
                )}
              </div>
              <div className="text-paragraph-sm text-text-sub-600">{p.blocks.length}</div>
              <div className="text-paragraph-xs text-text-soft-400">{fmtTs(`@${p.updated_at}`)}</div>
              <div
                className="flex items-center justify-end gap-1.5"
                onClick={(e) => e.stopPropagation()}
              >
                <Button
                  variant="neutral" mode="ghost" size="xsmall"
                  disabled={busy === p.id}
                  leftIcon={<CopyIcon className="size-4" />}
                  onClick={() => duplicate(p)}
                />
                <Button
                  variant="error" mode="ghost" size="xsmall"
                  disabled={busy === p.id}
                  leftIcon={<DeleteIcon className="size-4" />}
                  onClick={() => remove(p)}
                />
              </div>
            </div>
          ))
        )}
      </div>

      <ModulesCard />
    </section>
  );
}
