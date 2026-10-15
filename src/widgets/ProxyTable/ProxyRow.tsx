import { useRef } from "react";
import { Checkbox } from "@proxyshard/shardx-ui-kit";
import type { ContextItem } from "../../shared/types";
import { useT } from "../../shared/i18n";
import {
  useProxy,
  type ProxyEntry,
} from "../../entities/proxy";
import { ProxyRowActions, RenameProxyCell } from "../../features/manage-proxies";

import { ProxyTypeBadge, ProxyCountryCell, ProxyTestResult } from "../../entities/proxy";

export function ProxyRow({ proxy, profileCount, onMenu }: {
  proxy: ProxyEntry;
  profileCount: number;
  onMenu: (e: React.MouseEvent, items: ContextItem[]) => void;
}) {
  const t = useT();
  const snap = useProxy((s) => s.snapshots[proxy.id]);
  const busy = useProxy((s) => !!s.proxyTesting[proxy.id]);
  const isSel = useProxy((s) => s.proxySel.has(proxy.id));
  const selectProxy = useProxy((s) => s.selectProxy);
  const selectRangeTo = useProxy((s) => s.selectRangeTo);
  const testProxy = useProxy((s) => s.testProxy);
  const removeProxy = useProxy((s) => s.removeProxy);
  const setEditing = useProxy((s) => s.setEditing);
  const setInfoFor = useProxy((s) => s.setInfoFor);

  // Handled in mousedown only: a click on the checkbox's <label> reaches the
  // row twice, and applying the range twice would undo it.
  const shiftPress = useRef(false);

  // One list for right-click and the ⋮ button.
  const menu = (e: React.MouseEvent): ContextItem[] => [
    { label: t("proxyRow.menuTest"), onClick: () => testProxy(proxy) },
    { label: t("proxyRow.menuViewDetails"), onClick: () => setInfoFor({ proxy, anchor: { x: e.clientX, y: e.clientY } }) },
    { label: t("proxyRow.menuEdit"), onClick: () => setEditing(proxy) },
    { sep: true, label: "", onClick: () => { } },
    { label: t("proxyRow.menuDelete"), onClick: () => removeProxy(proxy.id), danger: true },
  ];
  const address = `${proxy.host}:${proxy.port}`;

  return (
    <div
      className="relative border-t border-stroke-soft-200 first:border-t-0"
      onMouseDown={(e) => {
        const t = e.target as HTMLElement;
        shiftPress.current = e.button === 0 && e.shiftKey && !t.closest("button, a");
        if (!shiftPress.current) return;
        // Stops the text selection a shift-drag begins, and the label
        // activation that would reach the checkbox.
        e.preventDefault();
        selectRangeTo(proxy.id);
      }}
      onClick={(e) => { if (shiftPress.current) e.preventDefault(); }}
      onContextMenu={(e) => onMenu(e, menu(e))}
    >
      <div className="p-cols transition-colors hover:bg-bg-weak-50">
        <div>
          <Checkbox
            checked={isSel}
            onChange={() => { if (!shiftPress.current) selectProxy(!isSel, [proxy]); }}
          />
        </div>
        {/* The name, and under it the address — only when the name is not the address already,
            which it is for most proxies (it had a column of its own that repeated the name). */}
        <div className="min-w-0">
          <RenameProxyCell proxy={proxy} />
          {proxy.name !== address && (
            <span
              className="mono mt-0.5 block max-w-full cursor-pointer truncate text-[10.5px] text-text-soft-400 transition-colors hover:text-primary-base"
              onClick={() => { if (!shiftPress.current) setEditing(proxy); }}
              title={t("proxyRow.editProxyTitle")}
            >
              {address}
            </span>
          )}
        </div>
        <div><ProxyTypeBadge kind={proxy.kind} /></div>
        <div className="min-w-0"><ProxyCountryCell snap={snap} fallback={proxy.country} /></div>
        <div className="cell-count text-paragraph-xs text-text-sub-600" title={t("proxyRow.boundProfilesTitle", { n: profileCount })}>
          {profileCount > 0 ? profileCount : <span className="text-text-soft-400">0</span>}
        </div>
        <div className="min-w-0">
          <ProxyTestResult snap={snap} kind={proxy.kind} busy={busy} />
        </div>
        <ProxyRowActions proxy={proxy} onMore={(e) => { e.stopPropagation(); onMenu(e, menu(e)); }} />
      </div>
    </div>
  );
}
