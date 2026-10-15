import { CountryFlag } from "../../../shared/ui/CountryFlag";
import type { ProxyTestSnapshot } from "../model/types";

export function ProxyCountryCell({ snap, fallback }: { snap?: ProxyTestSnapshot; fallback: string }) {
  const cc = snap?.country_code || fallback || "";
  if (!cc) return <span className="text-paragraph-xs text-text-soft-400">—</span>;
  return (
    // The flag says the country (its code is on hover); the city, when a test found one, beside it.
    <span className="inline-flex min-w-0 items-center gap-1.5" title={snap?.city ? `${cc} · ${snap.city}` : cc}>
      <CountryFlag cc={cc} />
      <span className="min-w-0 truncate text-paragraph-xs text-text-sub-600">{snap?.city || cc}</span>
    </span>
  );
}
