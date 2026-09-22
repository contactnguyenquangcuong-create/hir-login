import { useMemo } from "react";
import { t } from "../../../shared/i18n";
import type { ProxyEntry } from "../../proxy";
import { applyProfileFilters, useProfile } from "./useProfile";

/// Folder tabs derived from profile assignments + the persisted registry of
/// empty folders. Sorted; "all" is rendered separately as the first tab.
export function useFolders() {
  const profiles = useProfile((s) => s.profiles);
  const folderRegistry = useProfile((s) => s.folderRegistry);
  return useMemo(() => {
    const set = new Set<string>(folderRegistry);
    for (const p of profiles) if (p.folder) set.add(p.folder);
    return [...set].sort((a, b) => a.localeCompare(b));
  }, [profiles, folderRegistry]);
}

/// Profiles filtered by the folder tab, the search query and the filter bar.
export function useVisibleProfiles() {
  const profiles = useProfile((s) => s.profiles);
  const proxies = useProfile((s) => s.proxies);
  const search = useProfile((s) => s.search);
  const folder = useProfile((s) => s.folder);
  const filters = useProfile((s) => s.filters);
  const running = useProfile((s) => s.running);
  const sort = useProfile((s) => s.sort);
  return useMemo(
    () => applyProfileFilters(profiles, proxies, search, folder, filters, running, sort),
    [profiles, proxies, search, folder, filters, running, sort],
  );
}

/// Country codes present among bound proxies, for the country filter.
export function useProfileCountries() {
  const profiles = useProfile((s) => s.profiles);
  const proxies = useProfile((s) => s.proxies);
  return useMemo(() => {
    const byId = new Map(proxies.map((p) => [p.id, p]));
    const set = new Set<string>();
    for (const p of profiles) {
      const cc = p.proxy_id ? byId.get(p.proxy_id)?.country ?? "" : "";
      if (cc) set.add(cc.toUpperCase());
    }
    return [...set].sort();
  }, [profiles, proxies]);
}

/// proxy_id → ProxyEntry lookup for the Proxy column.
export function useProxyMap() {
  const proxies = useProfile((s) => s.proxies);
  return useMemo(
    () => Object.fromEntries(proxies.map((p) => [p.id, p])) as Record<string, ProxyEntry>,
    [proxies],
  );
}

/// Count of currently-running engines (for the Running metric).
export function useRunningCount() {
  const running = useProfile((s) => s.running);
  return useMemo(() => Object.values(running).filter(Boolean).length, [running]);
}

/// Why "Launch synced" is unavailable for the current selection; "" means it is fine.
/// A press is a tap on a phone and a click on a desktop: one stream cannot drive both.
export function useSyncBlockReason(): string {
  const profiles = useProfile((s) => s.profiles);
  const selected = useProfile((s) => s.selected);
  return useMemo(() => {
    const picked = profiles.filter((p) => selected.has(p.id));
    const m = picked.filter((p) => p.mobile).length;
    if (m > 0 && m < picked.length) {
      return t("selectors.syncMixedSelection", { m, d: picked.length - m });
    }
    return "";
  }, [profiles, selected]);
}
