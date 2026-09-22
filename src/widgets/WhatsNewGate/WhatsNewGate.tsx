import { useEffect } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { useNav } from "../../shared/model/navigation";

const SEEN_KEY = "shardx-patchlog-seen-version";

/** Opens the patch log on the first run after an update. Renders nothing. */
export function WhatsNewGate() {
  const setSection = useNav((s) => s.setSection);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      let version: string;
      try {
        version = await getVersion();
      } catch {
        // Plain-browser dev: no Tauri IPC, and nothing to announce.
        return;
      }
      if (cancelled) return;
      const seen = localStorage.getItem(SEEN_KEY);
      // A fresh install has nothing to catch up on — record the version and
      // leave the operator on the profile list.
      localStorage.setItem(SEEN_KEY, version);
      if (seen && seen !== version) {
        setSection("patchlog");
      }
    })();
    return () => { cancelled = true; };
  }, [setSection]);

  return null;
}
