import { Metric } from "../../shared/ui/Metric";
import { fmtCents } from "../../shared/lib/utils";
import { usePsAccount, usePsConnected } from "../../entities/proxyshard";
import { useT } from "../../shared/i18n";

export function PsAccountMetrics() {
  const t = useT();
  const me = usePsAccount((s) => s.me);
  const connected = usePsConnected();

  return (
    <div className="grid grid-cols-4 gap-[10px] mb-4">
      <Metric label={t("psAccountMetrics.account")} value={connected ? t("psAccountMetrics.connected") : "—"} accent={connected} pulse={connected} />
      <Metric label={t("psAccountMetrics.balance")} value={me ? fmtCents(me.wallet_balance) : "—"} />
      <Metric label={t("psAccountMetrics.activeOrders")} value={me ? String(me.active_orders) : "—"} />
    </div>
  );
}
