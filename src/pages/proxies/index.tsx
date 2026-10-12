import { useEffect } from "react";
import { Topbar } from "../../shared/ui/Topbar";
import { useStoreChanged } from "../../shared/hooks/useStoreChanged";
import { useProxy } from "../../entities/proxy";
import { storeBus } from "../../shared/lib/storeBus";
import { ProxyEditor, ProxyBulkImporter, ProxyInfoPopover, ProxyDistributeModal } from "../../features/manage-proxies";
import { ProxyTable } from "../../widgets/ProxyTable/ProxyTable";
import { ProxyToolbar } from "../../widgets/ProxyTable/ProxyToolbar";
import { useT } from "../../shared/i18n";

export function ProxiesPage() {
  const t = useT();
  const init = useProxy((s) => s.init);
  const reload = useProxy((s) => s.reload);
  const search = useProxy((s) => s.search);
  const setSearch = useProxy((s) => s.setSearch);
  const editing = useProxy((s) => s.editing);
  const setEditing = useProxy((s) => s.setEditing);
  const bulkOpen = useProxy((s) => s.bulkOpen);
  const setBulkOpen = useProxy((s) => s.setBulkOpen);
  const infoFor = useProxy((s) => s.infoFor);
  const setInfoFor = useProxy((s) => s.setInfoFor);
  const snapshots = useProxy((s) => s.snapshots);
  const distributeOpen = useProxy((s) => s.distributeOpen);
  const setDistributeOpen = useProxy((s) => s.setDistributeOpen);

  useEffect(() => { init(); }, [init]);
  // Pick up proxies/profiles added via the automation API or MCP live.
  useStoreChanged(reload);

  return (
    <section className="flex flex-col">
      <Topbar crumbs={[t("proxies.crumbWorkspace"), t("proxies.crumbProxies")]} search={search} onSearch={setSearch} />
      <div className="mb-3.5 flex items-end justify-between gap-4">
        <h1 className="m-0 text-title-h5 text-text-strong-950">{t("proxies.title")}</h1>
        <ProxyToolbar />
      </div>
      <ProxyTable />
      {editing && (
        <ProxyEditor
          initial={editing}
          onClose={() => { setEditing(null); reload(); }}
          onSaved={() => storeBus.emit("proxies")}
        />
      )}
      {bulkOpen && (
        <ProxyBulkImporter
          onClose={() => { setBulkOpen(false); reload(); storeBus.emit("proxies"); }}
        />
      )}
      {distributeOpen && <ProxyDistributeModal onClose={() => setDistributeOpen(false)} />}
      {infoFor && (
        <ProxyInfoPopover
          proxy={infoFor.proxy}
          anchor={infoFor.anchor}
          latest={snapshots[infoFor.proxy.id]}
          onClose={() => setInfoFor(null)}
        />
      )}
    </section>
  );
}
