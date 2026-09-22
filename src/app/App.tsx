import { useEffect } from "react";
import { TitleBar } from "../widgets/TitleBar/TitleBar";
import { Sidebar } from "../widgets/Sidebar/Sidebar";
import { FirstRunGate } from "../widgets/FirstRunGate/FirstRunGate";
import { ToastHost } from "../widgets/ToastHost/ToastHost";
import { ConfirmHost } from "../widgets/ConfirmHost/ConfirmHost";
import { StarModal } from "../widgets/StarModal/StarModal";
import { HelperWatcher } from "../widgets/HelperWatcher";
import { WhatsNewGate } from "../widgets/WhatsNewGate";
import { BrowsersPage } from "../pages/browsers";
import { ProxiesPage } from "../pages/proxies";
import { ProxyShardPage } from "../pages/proxyshard";
import { FingerprintsPage } from "../pages/fingerprints";
import { ExtensionsPage } from "../pages/extensions";
import { BookmarksPage } from "../pages/bookmarks";
import { AutomationPage } from "../pages/automation";
import { TrashPage } from "../pages/trash";
import { SettingsPage } from "../pages/settings";
import { PatchLogPage } from "../pages/patchlog";
import { useNav } from "../shared/model/navigation";
import { useLauncherWarning } from "../shared/hooks/useLauncherWarning";
import { trackSection } from "../shared/lib/analytics";

export function App() {
  const section = useNav((s) => s.section);

  useEffect(() => { void trackSection(section); }, [section]);
  useLauncherWarning();

  return (
    <>
      <TitleBar />
      <HelperWatcher />
      <WhatsNewGate />
      <FirstRunGate>
        <div
          className="grid overflow-hidden bg-bg-weak-50 [grid-template-columns:240px_1fr] [@media(min-width:1700px)]:[grid-template-columns:280px_1fr]"
          style={{ height: "100vh", paddingTop: "var(--titlebar-h)" }}
        >
          <Sidebar />
          <main className="overflow-y-auto px-7 py-6">
            {section === "browsers" && <BrowsersPage />}
            {section === "proxies" && <ProxiesPage />}
            {section === "proxyshard" && <ProxyShardPage />}
            {section === "fingerprints" && <FingerprintsPage />}
            {section === "extensions" && <ExtensionsPage />}
            {section === "bookmarks" && <BookmarksPage />}
            {section === "automation" && <AutomationPage />}
            {section === "trash" && <TrashPage />}
            {section === "patchlog" && <PatchLogPage />}
            {section === "settings" && <SettingsPage />}
          </main>
          <ToastHost />
          <ConfirmHost />
          <StarModal />
        </div>
      </FirstRunGate>
    </>
  );
}
