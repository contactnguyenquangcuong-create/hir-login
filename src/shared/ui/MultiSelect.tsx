import { useEffect, useMemo, useRef, useState } from "react";
import { useT } from "../i18n";

export type MSOption = { value: string; label: string };

/** A searchable multi-select. The value is a comma-separated string of the
 *  chosen option values ("" means "any"); several picked means the runner
 *  chooses one at random each run. Kept tiny and dependency-free so it drops
 *  into the step panel next to the plain fields. */
export function MultiSelect({
  value,
  onChange,
  options,
  loading,
  disabled,
  placeholder,
  emptyLabel: emptyLabelProp,
  single,
}: {
  value: string;
  onChange: (v: string) => void;
  options: MSOption[];
  loading?: boolean;
  disabled?: boolean;
  placeholder?: string;
  emptyLabel?: string;
  /** One choice only — picking replaces the value and closes. */
  single?: boolean;
}) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const [q, setQ] = useState("");
  const box = useRef<HTMLDivElement | null>(null);
  const emptyLabel = emptyLabelProp ?? t("multiSelect.anyLabel");

  const chosen = useMemo(
    () => value.split(",").map((s) => s.trim()).filter(Boolean),
    [value],
  );
  const set = (list: string[]) => onChange(list.join(","));
  const toggle = (v: string) => {
    if (single) {
      set(chosen.includes(v) ? [] : [v]);
      setOpen(false);
      return;
    }
    set(chosen.includes(v) ? chosen.filter((x) => x !== v) : [...chosen, v]);
  };

  useEffect(() => {
    if (!open) return;
    const close = (e: MouseEvent) => {
      if (box.current && !box.current.contains(e.target as Node)) setOpen(false);
    };
    window.addEventListener("mousedown", close);
    return () => window.removeEventListener("mousedown", close);
  }, [open]);

  const labelOf = (v: string) => options.find((o) => o.value === v)?.label ?? v;
  const query = q.trim().toLowerCase();
  const shown = query
    ? options.filter(
        (o) => o.label.toLowerCase().includes(query) || o.value.toLowerCase().includes(query),
      )
    : options;

  return (
    <div ref={box} className="relative flex-1">
      <button
        type="button"
        disabled={disabled}
        onClick={() => setOpen((v) => !v)}
        className="flex h-8 w-full items-center gap-1 overflow-hidden rounded-8 bg-bg-white-0 px-2 text-left text-paragraph-sm text-text-strong-950 ring-1 ring-inset ring-stroke-soft-200 outline-none focus:ring-primary-base disabled:opacity-50"
      >
        {chosen.length === 0 ? (
          <span className="text-text-soft-400">{placeholder ?? emptyLabel}</span>
        ) : (
          <span className="truncate">
            {chosen.length <= 3
              ? chosen.map(labelOf).join(", ")
              : t("multiSelect.selectedCount", { n: chosen.length })}
          </span>
        )}
        <span className="ml-auto shrink-0 text-text-soft-400">▾</span>
      </button>

      {open && (
        <div className="absolute z-30 mt-1 max-h-64 w-full overflow-hidden rounded-8 bg-bg-white-0 shadow-[var(--shadow-md)] ring-1 ring-stroke-soft-200">
          <input
            autoFocus
            value={q}
            onChange={(e) => setQ(e.target.value)}
            placeholder={t("multiSelect.searchPlaceholder")}
            className="h-8 w-full border-b border-stroke-soft-200 bg-bg-white-0 px-2 text-paragraph-sm text-text-strong-950 outline-none placeholder:text-text-soft-400"
          />
          <div className="max-h-52 overflow-y-auto py-1">
            <button
              type="button"
              onClick={() => { set([]); }}
              className="flex w-full items-center gap-2 px-2 py-1 text-left text-paragraph-sm hover:bg-bg-weak-50"
            >
              <span className={`size-3.5 rounded-4 ring-1 ${chosen.length === 0 ? "bg-primary-base ring-primary-base" : "ring-stroke-soft-200"}`} />
              <span className="text-text-sub-600">{emptyLabel}</span>
            </button>
            {loading && <div className="px-2 py-2 text-paragraph-xs text-text-soft-400">{t("multiSelect.loading")}</div>}
            {!loading && shown.length === 0 && (
              <div className="px-2 py-2 text-paragraph-xs text-text-soft-400">{t("multiSelect.noMatches")}</div>
            )}
            {shown.map((o) => {
              const on = chosen.includes(o.value);
              return (
                <button
                  key={o.value}
                  type="button"
                  onClick={() => toggle(o.value)}
                  className="flex w-full items-center gap-2 px-2 py-1 text-left text-paragraph-sm hover:bg-bg-weak-50"
                >
                  <span className={`size-3.5 rounded-4 ring-1 ${on ? "bg-primary-base ring-primary-base" : "ring-stroke-soft-200"}`} />
                  <span className="truncate text-text-strong-950">{o.label}</span>
                  {o.value !== o.label && (
                    <span className="ml-auto shrink-0 text-[10px] text-text-soft-400">{o.value}</span>
                  )}
                </button>
              );
            })}
          </div>
        </div>
      )}
    </div>
  );
}
