import { Button } from "@proxyshard/shardx-ui-kit";
import { MoreIcon, RefreshIcon } from "../../../shared/icons";
import { useProxy, type ProxyEntry } from "../../../entities/proxy";
import { useT } from "../../../shared/i18n";

/// What a row offers at a glance: test it (the thing done most), and ⋮ for details, edit and
/// delete — the same as right-click. Four icon buttons per row, one of them a solid red delete,
/// were the loudest thing on the page.
export function ProxyRowActions({ proxy, onMore }: {
  proxy: ProxyEntry;
  onMore: (e: React.MouseEvent) => void;
}) {
  const t = useT();
  const busy = useProxy((s) => !!s.proxyTesting[proxy.id]);
  const testProxy = useProxy((s) => s.testProxy);

  return (
    <div className="flex justify-end gap-1">
      <Button
        variant="neutral"
        mode="stroke"
        size="xsmall"
        className="w-[72px] flex-none"
        leftIcon={<span className={busy ? "spin-icon inline-grid place-items-center" : "inline-grid place-items-center"}><RefreshIcon className="size-3.5" /></span>}
        onClick={() => testProxy(proxy)}
        disabled={busy}
        title={t("proxyRowActions.testTitle")}
      >
        {t("proxyRowActions.test")}
      </Button>
      <Button
        variant="neutral"
        mode="stroke"
        size="xsmall"
        onlyIcon
        onMouseDown={(e) => e.stopPropagation()}
        onClick={onMore}
        title={t("proxyRowActions.more")}
        leftIcon={<MoreIcon className="size-4" />}
      />
    </div>
  );
}
