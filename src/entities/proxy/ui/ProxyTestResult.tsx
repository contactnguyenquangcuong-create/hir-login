import { openUrl } from "@tauri-apps/plugin-opener";
import Badge from "../../../shared/ui/Badge";
import { UDP_DOCS_URL } from "../../../shared/lib/utils";
import { useT } from "../../../shared/i18n";
import type { ProxyEntry, ProxyTestSnapshot } from "../model/types";

export function ProxyTestResult({ snap, kind, busy }: {
  snap?: ProxyTestSnapshot;
  kind: ProxyEntry["kind"];
  busy: boolean;
}) {
  const t = useT();
  if (busy) return <span className="text-paragraph-xs text-text-soft-400">{t("proxyTestResult.testing")}</span>;
  if (!snap) return <span className="text-paragraph-xs text-text-soft-400">{t("proxyTestResult.notTested")}</span>;

  return (
    <div className="flex flex-wrap items-center gap-1.5">
      <Badge
        color={snap.tcp_ms != null ? "success" : "error"}
        variant='filled'
        size="small"
        dot
        title={snap.tcp_ms != null ? t("proxyTestResult.tcpOkTitle", { ms: snap.tcp_ms }) : t("proxyTestResult.tcpFailedTitle")}
      >
        {snap.tcp_ms != null ? t("proxyTestResult.active") : t("proxyTestResult.failed")}
      </Badge>
      {/* UDP pill: clickable to docs explaining what the presence/absence of
          UDP means for QUIC + WebRTC. HTTP proxies never have UDP, but the
          badge still tells the user why QUIC will be force-disabled at launch. */}
      {snap.udp_ms != null && kind === "socks5" && (
        <button
          type="button"
          className="cursor-pointer flex items-center border-0 bg-transparent p-0 transition-[filter,transform] hover:brightness-110 active:translate-y-px"
          title={t("proxyTestResult.udpOkTitle", { ms: snap.udp_ms })}
          onClick={() => { openUrl(UDP_DOCS_URL).catch(() => { }); }}
        >
          <Badge color="primary" variant='filled' size="small">UDP</Badge>
        </button>
      )}
      {snap.udp_ms == null && (
        <button
          type="button"
          className="status-pill-no-udp relative flex items-center cursor-pointer rounded-full border-0 bg-transparent p-0 transition-[filter,transform] hover:brightness-110 active:translate-y-px"
          title={t("proxyTestResult.noUdpTitle")}
          onClick={() => { openUrl(UDP_DOCS_URL).catch(() => { }); }}
        >
          <Badge color="error" variant='filled' size="small">UDP</Badge>
        </button>
      )}
      {snap.tcp_ms != null && snap.ip && (
        <span
          className="mono max-w-[15ch] overflow-hidden text-ellipsis whitespace-nowrap rounded-4 bg-primary-alpha-10 px-1.5 py-px text-[11.5px] text-primary-base"
          title={snap.isp}
        >
          {snap.ip}
        </span>
      )}
    </div>
  );
}
