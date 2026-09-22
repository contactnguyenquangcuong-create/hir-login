import { useMemo } from "react";
import { filterProxies, useProxy } from "./useProxy";

/// Search-filtered proxies, matching name/host/port/country/notes/user + geo snapshot.
export function useFilteredProxies() {
    const proxies = useProxy((s) => s.proxies);
    const snapshots = useProxy((s) => s.snapshots);
    const search = useProxy((s) => s.search);
    return useMemo(
        () => filterProxies(proxies, snapshots, search),
        [proxies, snapshots, search],
    );
}

/// proxy_id → bound-profile count (O(n) tally; n is small).
export function useProfileCountByProxy() {
    const profiles = useProxy((s) => s.profiles);
    return useMemo(() => {
        const out: Record<string, number> = {};
        for (const p of profiles) {
            if (p.proxy_id) out[p.proxy_id] = (out[p.proxy_id] ?? 0) + 1;
        }
        return out;
    }, [profiles]);
}
