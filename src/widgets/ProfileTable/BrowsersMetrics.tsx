import { Metric } from "../../shared/ui/Metric";
import { useProfile, useRunningCount } from "../../entities/profile";
import { useT } from "../../shared/i18n";

export function BrowsersMetrics() {
  const t = useT();
  const profileCount = useProfile((s) => s.profiles.length);
  const proxyCount = useProfile((s) => s.proxies.length);
  const fingerprintCount = useProfile((s) => s.fingerprints.length);
  const runningCount = useRunningCount();

  return (
    <div className="grid grid-cols-4 gap-[10px] mb-4">
      <Metric label={t("browsersMetrics.profiles")} value={String(profileCount)} accent />
      <Metric label={t("browsersMetrics.running")} value={String(runningCount)} pulse={runningCount > 0} />
      <Metric label={t("browsersMetrics.proxies")} value={String(proxyCount)} />
      <Metric label={t("browsersMetrics.fingerprints")} value={String(fingerprintCount)} />
    </div>
  );
}
