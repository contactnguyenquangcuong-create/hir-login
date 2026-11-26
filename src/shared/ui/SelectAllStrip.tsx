import { useT } from "../i18n";

type Props = {
  /** Rows on the page being shown. */
  pageCount: number;
  /** Rows in the whole (filtered) list, every page. */
  total: number;
  /** How many of those `total` rows are ticked. */
  selected: number;
  allPageSelected: boolean;
  onSelectPage: () => void;
  onSelectAll: () => void;
  onClearAll: () => void;
};

const link =
  "cursor-pointer border-0 bg-transparent p-0 text-label-xs text-primary-base underline-offset-2 hover:underline";

/** A thin line under a paged table's column titles: the header box only ticks the
 *  page in view, so this is where the whole list (every page) is ticked or cleared. */
export function SelectAllStrip({
  pageCount, total, selected, allPageSelected, onSelectPage, onSelectAll, onClearAll,
}: Props) {
  const t = useT();
  const allSelected = total > 0 && selected >= total;
  return (
    <div className="flex flex-wrap items-center gap-x-3 gap-y-1 border-b border-stroke-soft-200 bg-primary-alpha-10 px-4 py-1.5 text-paragraph-xs text-text-sub-600">
      <span>
        {selected > 0
          ? t("selectStrip.selected", { n: selected, total })
          : t("selectStrip.none", { total })}
      </span>
      {!allPageSelected && pageCount > 0 && (
        <button type="button" className={link} onClick={onSelectPage}>
          {t("selectStrip.selectPage", { n: pageCount })}
        </button>
      )}
      {!allSelected && (
        <button type="button" className={link} onClick={onSelectAll}>
          {t("selectStrip.selectAll", { n: total })}
        </button>
      )}
      {selected > 0 && (
        <button type="button" className={link} onClick={onClearAll}>
          {t("selectStrip.clearAll")}
        </button>
      )}
    </div>
  );
}
