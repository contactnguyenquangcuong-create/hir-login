import { useEffect, useMemo, useRef, useState } from "react";
import { Button, Checkbox } from "../../shared/ui";
import { CloseIcon, SearchIcon } from "../../shared/icons";
import { useT } from "../../shared/i18n";
import type { ProfileMeta } from "../../entities/profile";
import type { RunSettings } from "../../entities/automation";

type Props = {
  profiles: ProfileMeta[];
  run: RunSettings;
  onChange: (next: Partial<RunSettings>) => void;
  onClose: () => void;
};

const NUM =
  "h-8 w-[68px] rounded-8 bg-bg-white-0 px-2 text-center text-paragraph-sm text-text-strong-950 ring-1 ring-inset ring-stroke-soft-200 outline-none focus:ring-primary-base";

/** How many chosen profiles are spelled out as chips; the rest are counted. */
const CHIPS = 8;

/** "from … to … seconds" as two small boxes. The second never drops below the first. */
function Range({
  label,
  hint,
  min,
  max,
  onChange,
}: {
  label: string;
  hint: string;
  min: number;
  max: number;
  onChange: (min: number, max: number) => void;
}) {
  const t = useT();
  const num = (v: string) => Math.max(0, Number(v) || 0);
  return (
    <div className="flex min-w-[260px] flex-1 flex-col gap-1">
      <span className="text-label-sm text-text-strong-950">{label}</span>
      <span className="text-paragraph-xs text-text-soft-400">{hint}</span>
      <span className="mt-1 flex items-center gap-2 text-paragraph-sm text-text-sub-600">
        {t("projectEditor.multiFrom")}
        <input
          type="number" min={0} step={1} className={NUM} value={min}
          onChange={(e) => { const a = num(e.target.value); onChange(a, Math.max(max, a)); }}
        />
        {t("projectEditor.multiTo")}
        <input
          type="number" min={0} step={1} className={NUM} value={max}
          onChange={(e) => { const b = num(e.target.value); onChange(Math.min(min, b), b); }}
        />
        {t("projectEditor.multiSeconds")}
      </span>
    </div>
  );
}

/** Picks the profiles a run drives together, and how their starts and steps are spread out.
 *
 *  Built for a long list: nothing is spelled out until it is asked for. The box is a search
 *  field; clicking it drops a list that scrolls inside itself and floats over what is below,
 *  so a thousand profiles do not make the page a thousand rows long. What is chosen shows
 *  as a few removable chips and a count. */
export function MultiProfilePanel({ profiles, run, onChange, onClose }: Props) {
  const t = useT();
  const [q, setQ] = useState("");
  const [open, setOpen] = useState(false);
  const [onlyChosen, setOnlyChosen] = useState(false);
  const box = useRef<HTMLDivElement | null>(null);
  const targets = run.targets ?? [];
  const chosen = useMemo(() => new Set(targets), [targets]);

  // A click anywhere outside the search box folds the list away.
  useEffect(() => {
    if (!open) return;
    const away = (e: MouseEvent) => {
      if (box.current && !box.current.contains(e.target as Node)) setOpen(false);
    };
    window.addEventListener("mousedown", away);
    return () => window.removeEventListener("mousedown", away);
  }, [open]);

  const rows = useMemo(() => {
    const query = q.trim().toLowerCase();
    return profiles
      .map((p, i) => ({ p, n: i + 1 }))
      .filter(({ p, n }) => {
        if (onlyChosen && !chosen.has(p.id)) return false;
        if (!query) return true;
        return (
          String(n) === query ||
          (p.name || "").toLowerCase().includes(query) ||
          (p.folder || "").toLowerCase().includes(query) ||
          p.id.toLowerCase().includes(query)
        );
      });
  }, [profiles, q, onlyChosen, chosen]);

  const set = (ids: string[]) => onChange({ targets: ids });
  const toggle = (id: string) => set(chosen.has(id) ? targets.filter((x) => x !== id) : [...targets, id]);
  const addShown = () => {
    const next = new Set(chosen);
    rows.forEach(({ p }) => next.add(p.id));
    set([...next]);
  };
  const removeShown = () => {
    const shown = new Set(rows.map(({ p }) => p.id));
    set(targets.filter((id) => !shown.has(id)));
  };
  const filtered = q.trim() !== "" || onlyChosen;
  const shownChosen = rows.filter(({ p }) => chosen.has(p.id)).length;

  const byId = useMemo(() => new Map(profiles.map((p, i) => [p.id, { p, n: i + 1 }])), [profiles]);
  const chips = targets.slice(0, CHIPS).map((id) => byId.get(id)).filter(Boolean) as { p: ProfileMeta; n: number }[];

  return (
    <section className="rounded-12 bg-bg-white-0 shadow-[var(--shadow-md)] ring-1 ring-inset ring-stroke-soft-200">
      <header className="flex items-start justify-between gap-3 border-b border-stroke-soft-200 px-4 py-3">
        <div className="min-w-0">
          <h3 className="m-0 text-label-md text-text-strong-950">{t("projectEditor.multiTitle")}</h3>
          <p className="m-0 mt-0.5 text-paragraph-xs text-text-soft-400">{t("projectEditor.multiHelp")}</p>
        </div>
        <div className="flex shrink-0 items-center gap-2">
          <span className="rounded-full bg-primary-alpha-10 px-2.5 py-1 text-label-xs text-primary-base">
            {t("projectEditor.multiCount", { n: chosen.size, total: profiles.length })}
          </span>
          <button
            type="button"
            aria-label={t("projectEditor.multiClose")}
            title={t("projectEditor.multiClose")}
            className="grid size-7 place-items-center rounded-8 text-text-soft-400 hover:bg-bg-weak-50 hover:text-text-strong-950"
            onClick={onClose}
          >
            <CloseIcon className="size-4" />
          </button>
        </div>
      </header>

      <div className="flex flex-col gap-4 px-4 py-3">
        {/* The search field and, under it, the list it opens. */}
        <div ref={box} className="relative">
          <label className="relative block">
            <SearchIcon className="pointer-events-none absolute left-2.5 top-1/2 size-4 -translate-y-1/2 text-text-soft-400" />
            <input
              value={q}
              onChange={(e) => { setQ(e.target.value); setOpen(true); }}
              onFocus={() => setOpen(true)}
              onKeyDown={(e) => { if (e.key === "Escape") setOpen(false); }}
              placeholder={t("projectEditor.multiSearch")}
              className="h-10 w-full rounded-10 bg-bg-white-0 pl-8 pr-3 text-paragraph-sm text-text-strong-950 ring-1 ring-inset ring-stroke-soft-200 outline-none placeholder:text-text-soft-400 focus:ring-primary-base"
            />
          </label>

          {open && (
            <div className="absolute inset-x-0 top-full z-50 mt-1.5 overflow-hidden rounded-12 bg-bg-white-0 shadow-[var(--shadow-md)] ring-1 ring-inset ring-stroke-soft-200">
              <div className="flex flex-wrap items-center gap-2 border-b border-stroke-soft-200 px-3 py-2">
                <Button variant="neutral" mode="stroke" size="xsmall" onClick={addShown} disabled={rows.length === 0}>
                  {filtered ? t("projectEditor.multiAddShown", { n: rows.length }) : t("projectEditor.multiAll")}
                </Button>
                <Button variant="neutral" mode="stroke" size="xsmall" onClick={removeShown} disabled={shownChosen === 0}>
                  {filtered ? t("projectEditor.multiRemoveShown") : t("projectEditor.multiNone")}
                </Button>
                <label className="ml-auto flex cursor-pointer items-center gap-2 text-paragraph-xs text-text-sub-600">
                  <Checkbox checked={onlyChosen} onChange={() => setOnlyChosen((v) => !v)} />
                  {t("projectEditor.multiOnlyChosen")}
                </label>
              </div>
              <div role="listbox" aria-multiselectable className="max-h-[280px] overflow-y-auto p-1">
                {rows.length === 0 && (
                  <div className="px-3 py-6 text-center text-paragraph-sm text-text-soft-400">
                    {profiles.length === 0 ? t("projectEditor.multiNoProfiles") : t("stepDetails.profileNone")}
                  </div>
                )}
                {rows.map(({ p, n }) => {
                  const on = chosen.has(p.id);
                  return (
                    <div
                      key={p.id}
                      role="option"
                      aria-selected={on}
                      title={p.id}
                      onClick={() => toggle(p.id)}
                      // Rows off screen are not laid out: a thousand of them stay quick.
                      style={{ contentVisibility: "auto", containIntrinsicSize: "auto 36px" }}
                      className={
                        "flex h-9 cursor-pointer items-center gap-3 rounded-8 px-2.5 text-paragraph-sm " +
                        (on ? "bg-primary-alpha-10 text-text-strong-950" : "text-text-strong-950 hover:bg-bg-weak-50")
                      }
                    >
                      <span onClick={(e) => e.stopPropagation()} className="flex items-center">
                        <Checkbox checked={on} onChange={() => toggle(p.id)} />
                      </span>
                      <span className="w-9 shrink-0 text-right tabular-nums text-text-soft-400">{n}</span>
                      <span className="min-w-0 flex-1 truncate">{p.name || t("stepDetails.noName")}</span>
                      {p.folder ? (
                        <span className="hidden max-w-[160px] shrink-0 truncate rounded-6 bg-bg-weak-50 px-2 py-0.5 text-paragraph-xs text-text-sub-600 sm:inline">
                          {p.folder}
                        </span>
                      ) : null}
                      <span className="w-[68px] shrink-0 text-right font-mono text-[11px] text-text-soft-400">{p.id.slice(0, 8)}</span>
                    </div>
                  );
                })}
              </div>
            </div>
          )}
        </div>

        {/* What is chosen, in a line or two. */}
        <div className="flex min-h-[28px] flex-wrap items-center gap-1.5">
          {chosen.size === 0 ? (
            <span className="text-paragraph-xs text-text-soft-400">{t("projectEditor.multiNoneChosen")}</span>
          ) : (
            <>
              {chips.map(({ p, n }) => (
                <span
                  key={p.id}
                  title={p.id}
                  className="flex items-center gap-1.5 rounded-full bg-primary-alpha-10 py-0.5 pl-2.5 pr-1 text-paragraph-xs text-text-strong-950"
                >
                  <span className="text-text-soft-400">{n}</span>
                  {p.name || t("stepDetails.noName")}
                  <button
                    type="button"
                    aria-label={t("projectEditor.multiRemove")}
                    className="grid size-4 place-items-center rounded-full text-text-soft-400 hover:bg-bg-white-0 hover:text-text-strong-950"
                    onClick={() => toggle(p.id)}
                  >
                    <CloseIcon className="size-3" />
                  </button>
                </span>
              ))}
              {chosen.size > CHIPS && (
                <button
                  type="button"
                  className="rounded-full px-2 py-0.5 text-paragraph-xs text-primary-base hover:bg-primary-alpha-10"
                  onClick={() => { setOnlyChosen(true); setOpen(true); }}
                >
                  {t("projectEditor.multiMore", { n: chosen.size - CHIPS })}
                </button>
              )}
            </>
          )}
        </div>

        <div className="flex flex-wrap gap-x-8 gap-y-3 border-t border-stroke-soft-200 pt-3">
          <Range
            label={t("projectEditor.multiStartGap")}
            hint={t("projectEditor.multiStartGapHint")}
            min={run.start_gap_min ?? 0}
            max={run.start_gap_max ?? 0}
            onChange={(a, b) => onChange({ start_gap_min: a, start_gap_max: b })}
          />
          <Range
            label={t("projectEditor.multiStepDelay")}
            hint={t("projectEditor.multiStepDelayHint")}
            min={run.step_delay_min ?? 0}
            max={run.step_delay_max ?? 0}
            onChange={(a, b) => onChange({ step_delay_min: a, step_delay_max: b })}
          />
        </div>
      </div>
    </section>
  );
}
