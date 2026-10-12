import { useEffect, useMemo, useState } from "react";
import { Checkbox, Pagination } from "@proxyshard/shardx-ui-kit";
import { RouteIcon } from "../../shared/icons";
import { useContextMenu } from "../../shared/hooks/useContextMenu";
import { useT } from "../../shared/i18n";
import { useProxy, useFilteredProxies, useProfileCountByProxy } from "../../entities/proxy";
import { NewProxyButton } from "../../features/manage-proxies";
import { ProxyRow } from "./ProxyRow";

const PROXY_PAGE_SIZE = 20;

export function ProxyTable() {
  const t = useT();
  const totalProxies = useProxy((s) => s.proxies.length);
  const selectProxy = useProxy((s) => s.selectProxy);
  const proxySel = useProxy((s) => s.proxySel);
  const search = useProxy((s) => s.search);

  const filteredProxies = useFilteredProxies();
  const profileCountByProxy = useProfileCountByProxy();
  const ctx = useContextMenu();

  const [proxyPage, setProxyPage] = useState(1);
  const proxyPageCount = Math.max(1, Math.ceil(filteredProxies.length / PROXY_PAGE_SIZE));
  useEffect(() => {
    if (proxyPage > proxyPageCount) setProxyPage(proxyPageCount);
  }, [proxyPageCount, proxyPage]);
  // Reset to page 1 when the search narrows the list to fewer pages.
  useEffect(() => { setProxyPage(1); }, [search]);
  const pagedProxies = useMemo(
    () => filteredProxies.slice((proxyPage - 1) * PROXY_PAGE_SIZE, proxyPage * PROXY_PAGE_SIZE),
    [filteredProxies, proxyPage],
  );

  const allPageSelected = pagedProxies.length > 0 && pagedProxies.every((p) => proxySel.has(p.id));
  const anyPageSelected = pagedProxies.some((p) => proxySel.has(p.id));

  return (
    <>
      <div className="overflow-hidden rounded-12 bg-bg-white-0 shadow-[var(--shadow-xs)] ring-1 ring-inset ring-stroke-soft-200">
        <div className="p-cols w-full justify-between border-b border-stroke-soft-200 bg-bg-weak-50 text-subheading-2xs text-text-soft-400">
          <div>
            <Checkbox
              title={t("proxyTable.selectAllOnPage")}
              checked={allPageSelected}
              indeterminate={anyPageSelected && !allPageSelected}
              onChange={(e) => selectProxy(e.target.checked, pagedProxies)}
            />
          </div>
          <div>{t("proxyTable.colName")}</div>
          <div>{t("proxyTable.colType")}</div>
          <div>{t("proxyTable.colHostPort")}</div>
          <div>{t("proxyTable.colCountry")}</div>
          <div>{t("proxyTable.colProfiles")}</div>
          <div>{t("proxyTable.colTestResult")}</div>
          <div></div>
        </div>
        {pagedProxies.map((p) => (
          <ProxyRow
            key={p.id}
            proxy={p}
            profileCount={profileCountByProxy[p.id] ?? 0}
            onMenu={ctx.open}
          />
        ))}
        {totalProxies === 0 && (
          <div className="flex flex-col items-center gap-2.5 px-6 py-14 text-center">
            <div className="grid size-14 place-items-center rounded-[14px] bg-primary-alpha-10 text-primary-base ring-1 ring-inset ring-primary-alpha-24">
              <RouteIcon className="size-6" />
            </div>
            <h3 className="m-0 text-label-sm text-text-strong-950">{t("proxyTable.emptyTitle")}</h3>
            <p className="m-0 max-w-[420px] text-paragraph-sm text-text-sub-600">
              {t("proxyTable.emptyHint")}
            </p>
            <div className="mt-2 flex gap-2">
              <NewProxyButton />
            </div>
          </div>
        )}
      </div>
      {proxyPageCount > 1 && (
        <div className="flex items-center justify-center py-3 pb-1">
          <Pagination
            page={proxyPage}
            totalPages={proxyPageCount}
            asLinks={false}
            onPageChange={setProxyPage}
            infoLabel={(p, total) => t("proxyTable.pageInfo", { page: p, total, count: totalProxies })}
          />
        </div>
      )}
      {ctx.node}
    </>
  );
}
