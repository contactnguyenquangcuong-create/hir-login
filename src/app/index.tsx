import React from "react";
import ReactDOM from "react-dom/client";
import { ThemeProvider } from "@proxyshard/shardx-ui-kit";
import "./styles/index.css";
import "./styles/app.css";
import "flag-icons/css/flag-icons.min.css";
import { App } from "./App";
import { SyncPanel } from "../widgets/SyncPanel";
import { HelperPanel } from "../widgets/HelperPanel";
import { FleetMonitor } from "../widgets/FleetMonitor";
import { initAnalytics } from "../shared/lib/analytics";
import { useLang } from "../shared/i18n";

// The always-on-top panels are second Tauri windows on this same bundle,
// addressed by hash — a 60px strip needs no vite entry of its own.
const panelParams = new URLSearchParams(
  window.location.hash.replace(/^#\/?/, ""),
);
const panelGroup = panelParams.get("syncPanel");
const helperProfile = panelParams.get("helperPanel");
const fleet = panelParams.get("fleet");

// The launcher window only — a panel is a second window on the same bundle and
// would otherwise report a second user for one person.
if (!panelGroup && !helperProfile && !fleet) void initAnalytics();

// Most strings are translated as their element is created rather than by a
// component that watches the language, so switching it has to build the tree
// again. Keying on the language is what does that.
function Root() {
  const lang = useLang((s) => s.lang);
  return (
    <ThemeProvider>
      {panelGroup ? <SyncPanel key={lang} group={panelGroup} />
       : helperProfile ? <HelperPanel key={lang} profile={helperProfile} />
       : fleet ? <FleetMonitor key={lang} />
       : <App key={lang} />}
    </ThemeProvider>
  );
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Root />
  </React.StrictMode>,
);
