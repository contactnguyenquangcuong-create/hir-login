import { useMemo } from "react";
import type { FingerprintEntry } from "../model/types";
import { useFingerprint } from "./useFingerprint";

/// Library entries grouped by OS (macOS → Windows → Linux → other). `query`
/// (label/platform/chrome/gpu, case-insensitive) narrows a library that can run
/// into the thousands down to something a person can actually scroll through.
export function useFingerprintGroups(query = "") {
    const items = useFingerprint((s) => s.items);
    return useMemo<ReadonlyArray<readonly [string, FingerprintEntry[]]>>(() => {
        const q = query.trim().toLowerCase();
        const pool = q
            ? items.filter((it) => `${it.label} ${it.platform} ${it.chrome} ${it.gpu}`.toLowerCase().includes(q))
            : items;
        const order = ["macOS", "Windows", "Linux"];
        const buckets = new Map<string, FingerprintEntry[]>();
        for (const it of pool) {
            const k = it.platform || "Other";
            if (!buckets.has(k)) buckets.set(k, []);
            buckets.get(k)!.push(it);
        }
        return [
            ...order.filter((k) => buckets.has(k)).map((k) => [k, buckets.get(k)!] as const),
            ...[...buckets.keys()].filter((k) => !order.includes(k)).map((k) => [k, buckets.get(k)!] as const),
        ];
    }, [items, query]);
}
