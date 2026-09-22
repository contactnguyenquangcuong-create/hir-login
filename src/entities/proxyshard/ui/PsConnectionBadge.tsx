import Badge from "../../../shared/ui/Badge";
import { useT } from "../../../shared/i18n";
import type { PsMe } from "../model/types";
import type { PsStatus } from "../lib/usePsAccount";

export function PsConnectionBadge({ status, me, err, hasKey }: {
  status: PsStatus;
  me: PsMe | null;
  err: string;
  hasKey: boolean;
}) {
  const t = useT();
  return (
    <div className="mt-2.5 min-h-[22px]">
      {status === "checking" && <span className="text-paragraph-xs text-text-soft-400">{t("psConnectionBadge.validating")}</span>}
      {status === "ok" && me && (
        <Badge color="success" variant="filled" size="small" dot>{t("psConnectionBadge.connected", { email: me.email })}</Badge>
      )}
      {status === "err" && (
        <Badge color="error" variant="filled" size="small" dot title={err}>{t("psConnectionBadge.notConnected", { err })}</Badge>
      )}
      {status === "idle" && !hasKey && <span className="text-paragraph-xs text-text-soft-400">{t("psConnectionBadge.noKeyYet")}</span>}
    </div>
  );
}
