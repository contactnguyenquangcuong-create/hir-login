import { BulkActionsBar, ImportProxyButton, NewProxyButton } from "../../features/manage-proxies";

export function ProxyToolbar() {
  return (
    <div className="flex items-center flex-none gap-2">
      <BulkActionsBar />
      <ImportProxyButton />
      <NewProxyButton />
    </div>
  );
}
