import { Breadcrumb, Input } from "@proxyshard/shardx-ui-kit";
import { SearchIcon } from "../icons";
import { useT } from "../i18n";

/// Page header — UI-kit Breadcrumb + search Input.
export function Topbar({ crumbs, search, onSearch }: { crumbs: string[]; search: string; onSearch: (v: string) => void }) {
  const t = useT();
  return (
    <div className="mb-4 flex items-center justify-between gap-4">
      <Breadcrumb items={crumbs.map((c) => ({ label: c }))} />
      <div className="w-[320px]">
      <Input
          inputSize="small"
          placeholder={t("topbar.searchPlaceholder")}
          leftIcon={<SearchIcon className="size-4" />}
          value={search}
          onChange={(e) => onSearch(e.target.value)}
        />
      </div>
    </div>
  );
}
