import { CountryFlag } from "../../../shared/ui/CountryFlag";
import type { ProxyTestSnapshot } from "../model/types";

export function ProxyCountryCell({ snap, fallback }: { snap?: ProxyTestSnapshot; fallback: string }) {
  const cc = snap?.country_code || fallback || "";
  if (!cc) return <span className="text-paragraph-xs text-text-soft-400">—</span>;
  return (
    <span className="wrap-anywhere inline-flex min-w-0 flex-wrap items-center gap-1.5 gap-y-[2px]">
      <CountryFlag cc={cc} />
      <span className="inline-block rounded-4 bg-bg-weak-50 px-1.5 py-0.5 text-[10.5px] font-bold tracking-[0.5px] text-text-sub-600">{cc}</span>
      {snap?.city && <span className="text-paragraph-xs text-text-soft-400">{snap.city}</span>}
    </span>
  );
}
