import { useEffect } from "react";
import { Topbar } from "../../shared/ui/Topbar";
import { useStoreChanged } from "../../shared/hooks/useStoreChanged";
import { useProfile } from "../../entities/profile";
import { useT } from "../../shared/i18n";
import { Button } from "@proxyshard/shardx-ui-kit";
import { canEdit, useTeam } from "../../shared/model/teamRole";
import { BrowsersMetrics } from "../../widgets/ProfileTable/BrowsersMetrics";
import { FolderTabs } from "../../widgets/ProfileTable/FolderTabs";
import { ProfileToolbar } from "../../widgets/ProfileTable/ProfileToolbar";
import { ProfileTable } from "../../widgets/ProfileTable/ProfileTable";
import {
  ProfileTemplatePicker,
  ProfileFolderModal,
  ProfileQuickEdit,
} from "../../features/manage-profiles";

/** Android profiles open as a phone-sized window (360 px wide, full height) that
 *  cannot be widened, which on a desktop reads as the browser being broken. They
 *  come from a bulk import that once picked fingerprints from the whole library. */
function AndroidNotice() {
  const t = useT();
  const count = useProfile((s) => s.profiles.filter((p) => p.mobile).length);
  const convertAll = useProfile((s) => s.androidToDesktopAll);
  const role = useTeam((s) => s.role);
  const configured = useTeam((s) => s.configured);
  if (count === 0) return null;
  return (
    <div className="mb-3.5 flex flex-wrap items-center gap-3 rounded-8 bg-warning-alpha-10 px-3 py-2 text-paragraph-xs text-text-strong-950 ring-1 ring-inset ring-stroke-soft-200">
      <span className="min-w-0 flex-1">{t("browsers.androidNotice", { n: count })}</span>
      {canEdit(role, configured)
        ? <Button variant="primary" mode="stroke" size="2xsmall" onClick={convertAll}>{t("browsers.androidNoticeFix")}</Button>
        : <span className="text-text-sub-600">{t("browsers.androidNoticeAsk")}</span>}
    </div>
  );
}

export function BrowsersPage() {
  const t = useT();
  const init = useProfile((s) => s.init);
  const reload = useProfile((s) => s.reload);
  const startProcessPolling = useProfile((s) => s.startProcessPolling);
  const search = useProfile((s) => s.search);
  const setSearch = useProfile((s) => s.setSearch);

  useEffect(() => { init(); }, [init]);
  // Poll real child-process status (anchors the uptime clock, refreshes totals).
  useEffect(() => startProcessPolling(), [startProcessPolling]);
  // Pick up profiles/proxies created via the automation API or MCP live.
  useStoreChanged(reload);

  return (
    <section className="flex flex-col">
      <Topbar crumbs={[t("browsers.crumbWorkspace"), t("browsers.crumbBrowsers")]} search={search} onSearch={setSearch} />

      <BrowsersMetrics />
      <AndroidNotice />

      {/* Wraps: with a selection the toolbar is wider than most windows, and as a
          non-wrapping right-hand block it squeezed the title into two lines and
          ran its own buttons off the edge of the screen. */}
      <div className="mb-3.5 flex flex-wrap items-end justify-between gap-x-4 gap-y-3">
        <div className="flex min-w-[16rem] flex-1 flex-col gap-3.5">
          <h1 className="m-0 whitespace-nowrap text-title-h5 text-text-strong-950">{t("browsers.title")}</h1>
          <FolderTabs />
        </div>
        <ProfileToolbar />
      </div>

      <ProfileTable />

      <ProfileTemplatePicker />
      <ProfileFolderModal />
      <ProfileQuickEdit />
    </section>
  );
}
