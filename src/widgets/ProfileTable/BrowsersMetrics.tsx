import { cn } from "@proxyshard/shardx-ui-kit";
import { useProfile, useRunningCount } from "../../entities/profile";
import { useT } from "../../shared/i18n";

/// The page's counts as small chips beside its title: the numbers are glanced at, not studied, and
/// three full-width cards pushed the table — what the page is for — half a screen down.
export function BrowsersMetrics() {
  const t = useT();
  const profileCount = useProfile((s) => s.profiles.length);
  const proxyCount = useProfile((s) => s.proxies.length);
  const runningCount = useRunningCount();

  const chip = (label: string, value: number, tone?: "running") => (
    <span
      className={cn(
        "inline-flex items-center gap-1.5 rounded-full bg-bg-weak-50 px-2.5 py-1 text-label-xs text-text-sub-600 ring-1 ring-inset ring-stroke-soft-200",
        tone === "running" && value > 0 && "bg-success-alpha-10 text-success-base ring-success-alpha-24",
      )}
    >
      {tone === "running" && value > 0 && <span className="size-1.5 rounded-full bg-success-base" />}
      <span className="tabular-nums text-text-strong-950">{value}</span>
      {label}
    </span>
  );

  return (
    <div className="flex flex-wrap items-center gap-2">
      {chip(t("browsersMetrics.profiles"), profileCount)}
      {chip(t("browsersMetrics.running"), runningCount, "running")}
      {chip(t("browsersMetrics.proxies"), proxyCount)}
    </div>
  );
}
