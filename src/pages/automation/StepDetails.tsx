import { useEffect, useRef, useState } from "react";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import {
  automationList,
  tableColumns,
  type TableColumn,
  branchKind,
  specFor,
  type Block,
  type Branch,
  type Project,
} from "../../entities/automation";
import { profileList } from "../../entities/profile/model/api";
import type { ProfileMeta } from "../../entities/profile/model/types";
import { MultiSelect, type MSOption } from "../../shared/ui/MultiSelect";
import { useT } from "../../shared/i18n";
import {
  psCountries,
  psRegions,
  psCities,
  psResiIsps,
} from "../../entities/proxyshard";

/** The one code in a comma list, if there is exactly one — dependent lists
 *  (region needs a country, city needs a region…) only load for a single pick. */
function single(v: unknown): string {
  const list = String(v ?? "").split(",").map((s) => s.trim()).filter(Boolean);
  return list.length === 1 ? list[0] : "";
}

/** Every saved project, loaded once and shared by the pickers below. */
function useProjects(): Project[] {
  const [list, setList] = useState<Project[]>([]);
  useEffect(() => {
    let alive = true;
    automationList()
      .then((ps) => alive && setList(ps))
      .catch(() => alive && setList([]));
    return () => {
      alive = false;
    };
  }, []);
  return list;
}

/** This machine's profiles, loaded once for the picker below. */
function useProfiles(): ProfileMeta[] {
  const [list, setList] = useState<ProfileMeta[]>([]);
  useEffect(() => {
    let alive = true;
    profileList()
      .then((ps) => alive && setList(ps))
      .catch(() => alive && setList([]));
    return () => {
      alive = false;
    };
  }, []);
  return list;
}

const FIELD =
  "h-8 flex-1 rounded-8 bg-bg-white-0 px-2 text-paragraph-sm text-text-strong-950 ring-1 ring-inset ring-stroke-soft-200 outline-none placeholder:text-text-soft-400 focus:ring-primary-base";

/** A profile chosen from a list that is already open: a search box on top and
 *  every profile below it, tall enough to see a dozen at once, one click to
 *  pick. Stores the id. A value that is not one of the listed ids (a
 *  "{{variable}}", or a profile on another machine) is kept and shown in a text
 *  box, so an imported project is not silently rewritten. */
function ProfileField({
  block,
  name,
  hint,
  onParam,
}: {
  block: Block;
  name: string;
  hint?: string;
  onParam: (id: string, key: string, value: unknown) => void;
}) {
  const t = useT();
  const profiles = useProfiles();
  const value = String(block.params[name] ?? "");
  const [q, setQ] = useState("");
  const [typing, setTyping] = useState(false);
  // Open while choosing; once a profile is picked the list folds into one row
  // showing its name, and a click on that row opens the list again.
  const [open, setOpen] = useState(false);
  const selected = profiles.findIndex((p) => p.id === value);
  const known = selected >= 0;
  const custom = typing || (value !== "" && !known && profiles.length > 0);
  const query = q.trim().toLowerCase();
  const rows = profiles
    .map((p, i) => ({ p, n: i + 1 }))
    .filter(
      ({ p, n }) =>
        !query ||
        String(n) === query ||
        (p.name || "").toLowerCase().includes(query) ||
        p.id.toLowerCase().includes(query) ||
        (p.folder || "").toLowerCase().includes(query),
    );
  return (
    <div className="flex min-w-0 flex-1 flex-col gap-1.5">
      {custom ? (
        <div className="flex items-center gap-1.5">
          <input
            className={FIELD}
            placeholder={hint ? t(hint) : undefined}
            value={value}
            onChange={(e) => onParam(block.id, name, e.target.value)}
          />
          <button
            type="button"
            className="h-8 shrink-0 rounded-8 px-2.5 text-label-sm text-text-strong-950 ring-1 ring-inset ring-stroke-soft-200 hover:bg-bg-weak-50"
            onClick={(e) => {
              e.preventDefault();
              setTyping(false);
              if (!known) onParam(block.id, name, "");
            }}
          >
            {t("stepDetails.profileFromList")}
          </button>
        </div>
      ) : known && !open ? (
        <button
          type="button"
          title={t("stepDetails.profileChange")}
          className="flex h-9 items-center gap-2 rounded-8 bg-bg-white-0 px-2 text-left text-paragraph-sm text-text-strong-950 ring-1 ring-inset ring-stroke-soft-200 hover:ring-primary-base"
          onClick={(e) => {
            e.preventDefault();
            setQ("");
            setOpen(true);
          }}
        >
          <span className="w-5 shrink-0 text-right text-text-soft-400">{selected + 1}</span>
          <span className="min-w-0 flex-1 truncate">
            {profiles[selected].name || t("stepDetails.noName")}
            {profiles[selected].folder ? (
              <span className="text-text-soft-400"> · {profiles[selected].folder}</span>
            ) : null}
          </span>
          <span className="shrink-0 font-mono text-[10px] text-text-soft-400">{value.slice(0, 8)}</span>
          <span aria-hidden className="shrink-0 text-text-soft-400">▾</span>
        </button>
      ) : (
        <>
          <input
            className={FIELD + " flex-none"}
            placeholder={t("stepDetails.profileSearch", { n: profiles.length })}
            value={q}
            onChange={(e) => setQ(e.target.value)}
          />
          <div
            role="listbox"
            className="flex max-h-[min(52vh,440px)] min-h-[72px] flex-col gap-px overflow-y-auto rounded-8 bg-bg-weak-50 p-1 ring-1 ring-inset ring-stroke-soft-200"
          >
            {rows.length === 0 && (
              <div className="px-2 py-3 text-center text-paragraph-xs text-text-soft-400">
                {t("stepDetails.profileNone")}
              </div>
            )}
            {rows.map(({ p, n }) => {
              const on = p.id === value;
              return (
                <button
                  key={p.id}
                  type="button"
                  role="option"
                  aria-selected={on}
                  title={p.id}
                  className={
                    "flex items-center gap-2 rounded-4 px-2 py-1.5 text-left text-paragraph-sm " +
                    (on
                      ? "bg-primary-alpha-10 text-text-strong-950 ring-1 ring-inset ring-primary-base"
                      : "text-text-strong-950 hover:bg-bg-white-0")
                  }
                  onClick={(e) => {
                    e.preventDefault();
                    onParam(block.id, name, p.id);
                    setQ("");
                    setOpen(false);
                  }}
                >
                  <span className="w-5 shrink-0 text-right text-text-soft-400">{n}</span>
                  <span className="min-w-0 flex-1 truncate">
                    {p.name || t("stepDetails.noName")}
                    {p.folder ? <span className="text-text-soft-400"> · {p.folder}</span> : null}
                  </span>
                  <span className="shrink-0 font-mono text-[10px] text-text-soft-400">{p.id.slice(0, 8)}</span>
                </button>
              );
            })}
          </div>
          <button
            type="button"
            className="self-start text-paragraph-xs text-text-soft-400 hover:text-text-strong-950"
            onClick={(e) => {
              e.preventDefault();
              setTyping(true);
            }}
          >
            {t("stepDetails.profileCustom")}
          </button>
        </>
      )}
    </div>
  );
}

/** The columns of the spreadsheet the `of` param points at, as a list. It reloads
 *  when the file changes, so picking a file shows its columns straight away. A
 *  value the list does not have (a typed name, a "{{variable}}") is kept. */
function ColumnField({
  block,
  name,
  of,
  hint,
  optional,
  onParam,
}: {
  block: Block;
  name: string;
  of: string;
  hint?: string;
  optional?: boolean;
  onParam: (id: string, key: string, value: unknown) => void;
}) {
  const t = useT();
  const path = String(block.params[of] ?? "").trim();
  const value = String(block.params[name] ?? "");
  const [cols, setCols] = useState<TableColumn[]>([]);
  const [err, setErr] = useState("");
  const [typing, setTyping] = useState(false);
  useEffect(() => {
    let alive = true;
    setErr("");
    if (!path || path.includes("{{")) {
      setCols([]);
      return;
    }
    // A typed path is not a file until it stops changing.
    const timer = setTimeout(() => {
      tableColumns(path)
        .then((c) => alive && setCols(c))
        .catch((e) => {
          if (!alive) return;
          setCols([]);
          setErr(String(e));
        });
    }, 350);
    return () => {
      alive = false;
      clearTimeout(timer);
    };
  }, [path]);

  const listed = cols.some((c) => c.value === value);
  const manual = typing || cols.length === 0 || (value !== "" && !listed);
  return (
    <div className="flex min-w-0 flex-1 flex-col gap-1">
      {manual ? (
        <div className="flex items-center gap-1.5">
          <input
            className={FIELD}
            placeholder={hint ? t(hint) : undefined}
            value={value}
            onChange={(e) => onParam(block.id, name, e.target.value)}
          />
          {cols.length > 0 && (
            <button
              type="button"
              className="h-8 shrink-0 rounded-8 px-2.5 text-label-sm text-text-strong-950 ring-1 ring-inset ring-stroke-soft-200 hover:bg-bg-weak-50"
              onClick={(e) => {
                e.preventDefault();
                setTyping(false);
                if (!listed) onParam(block.id, name, "");
              }}
            >
              {t("stepDetails.columnFromList")}
            </button>
          )}
        </div>
      ) : (
        <>
          <select
            className={FIELD}
            value={value}
            onChange={(e) => onParam(block.id, name, e.target.value)}
          >
            <option value="">{optional ? t("stepDetails.columnNone") : t("stepDetails.columnFirst")}</option>
            {cols.map((c) => (
              <option key={c.value} value={c.value}>{c.label}</option>
            ))}
          </select>
          <button
            type="button"
            className="self-start text-paragraph-xs text-text-soft-400 hover:text-text-strong-950"
            onClick={(e) => {
              e.preventDefault();
              setTyping(true);
            }}
          >
            {t("stepDetails.columnCustom")}
          </button>
        </>
      )}
      {err && path && (
        <span className="text-paragraph-xs text-warning-base">{t("stepDetails.columnsFailed", { err })}</span>
      )}
    </div>
  );
}

type VarInfo = { name: string; source: string; excel: boolean };

/** Variables the project's steps define: the `into` of a step that saves a
 *  result (an Excel row, a read text, a random pick…) and the name of a "set
 *  variable" step. What a text field can pull in with {{name}}. */
function collectVars(steps: Block[], t: (k: string) => string): VarInfo[] {
  const out: VarInfo[] = [];
  const seen = new Set<string>();
  const add = (name: unknown, b: Block, excel: boolean) => {
    const n = String(name ?? "").trim();
    if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(n) || seen.has(n)) return;
    seen.add(n);
    const what = b.label || t(specFor(b.kind)?.label ?? b.kind);
    const col = excel ? String(b.params.column ?? "").trim() : "";
    out.push({ name: n, source: excel && col ? `${what} · ${col}` : what, excel });
  };
  for (const b of steps) {
    if (b.kind === "var.set") add(b.params.name, b, false);
    else {
      add(b.params.into, b, b.kind === "sheet.next");
      if (b.kind === "sheet.next" && String(b.params.comment_column ?? "").trim()) {
        add(b.params.comment_into ?? "comment", b, true);
      }
    }
  }
  // Excel columns first: that is what people reach for.
  return out.sort((a, b) => Number(b.excel) - Number(a.excel));
}

/** A "{ } Variable" button that lists what other steps saved and puts the
 *  chosen one into the field as {{name}}. */
function VarButton({
  steps,
  value,
  onPick,
}: {
  steps: Block[];
  value: string;
  onPick: (next: string) => void;
}) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const box = useRef<HTMLDivElement | null>(null);
  const vars = collectVars(steps, t);
  useEffect(() => {
    if (!open) return;
    const close = (e: MouseEvent) => {
      if (box.current && !box.current.contains(e.target as Node)) setOpen(false);
    };
    window.addEventListener("mousedown", close);
    return () => window.removeEventListener("mousedown", close);
  }, [open]);
  return (
    <div ref={box} className="relative shrink-0">
      <button
        type="button"
        title={t("stepDetails.varTitle")}
        className="h-8 rounded-8 px-2 font-mono text-[11px] text-text-strong-950 ring-1 ring-inset ring-stroke-soft-200 hover:bg-bg-weak-50"
        onClick={(e) => {
          e.preventDefault();
          setOpen((v) => !v);
        }}
      >
        {"{ }"}
      </button>
      {open && (
        <div className="absolute right-0 top-9 z-30 flex max-h-64 w-64 flex-col gap-px overflow-y-auto rounded-8 bg-bg-white-0 p-1 shadow-[var(--shadow-md)] ring-1 ring-inset ring-stroke-soft-200">
          {vars.length === 0 ? (
            <div className="px-2 py-2 text-paragraph-xs text-text-soft-400">{t("stepDetails.varNone")}</div>
          ) : (
            vars.map((v) => (
              <button
                key={v.name}
                type="button"
                className="flex flex-col rounded-4 px-2 py-1.5 text-left hover:bg-bg-weak-50"
                onClick={(e) => {
                  e.preventDefault();
                  onPick(value ? `${value}{{${v.name}}}` : `{{${v.name}}}`);
                  setOpen(false);
                }}
              >
                <span className="flex items-center gap-1.5 text-paragraph-sm text-text-strong-950">
                  <code className="font-mono text-[12px]">{`{{${v.name}}}`}</code>
                  {v.excel && (
                    <span className="rounded-4 bg-primary-alpha-10 px-1 text-[10px] text-primary-base">Excel</span>
                  )}
                </span>
                <span className="truncate text-paragraph-xs text-text-soft-400">{v.source}</span>
              </button>
            ))
          )}
        </div>
      )}
    </div>
  );
}

/** A path with a Browse button — the system file picker, so nobody has to type
 *  or paste a long path. Typing still works. */
function FileField({
  block,
  name,
  hint,
  filters,
  onParam,
}: {
  block: Block;
  name: string;
  hint?: string;
  filters?: { name: string; extensions: string[] }[];
  onParam: (id: string, key: string, value: unknown) => void;
}) {
  const t = useT();
  const browse = async () => {
    try {
      const picked = await openFileDialog({ multiple: false, directory: false, filters });
      if (typeof picked === "string" && picked) onParam(block.id, name, picked);
    } catch {
      /* the dialog was dismissed or is unavailable; the box still takes a typed path */
    }
  };
  return (
    <>
      <input
        className={FIELD}
        placeholder={hint ? t(hint) : undefined}
        value={String(block.params[name] ?? "")}
        onChange={(e) => onParam(block.id, name, e.target.value)}
      />
      <button
        type="button"
        className="h-8 shrink-0 rounded-8 px-2.5 text-label-sm text-text-strong-950 ring-1 ring-inset ring-stroke-soft-200 hover:bg-bg-weak-50"
        onClick={(e) => {
          e.preventDefault();
          void browse();
        }}
      >
        {t("stepDetails.pickFile")}
      </button>
    </>
  );
}

/** Picks a saved project. Stores its id, and its name beside it: an id does not
 *  survive an export and a name is not unique, so a call keeps both. */
function ProjectField({
  block,
  name,
  onParam,
}: {
  block: Block;
  name: string;
  onParam: (id: string, key: string, value: unknown) => void;
}) {
  const t = useT();
  const projects = useProjects();
  const value = String(block.params[name] ?? "");
  return (
    <select
      className="h-8 flex-1 rounded-8 bg-bg-white-0 px-2 text-paragraph-sm text-text-strong-950 ring-1 ring-inset ring-stroke-soft-200 outline-none focus:ring-primary-base"
      value={value}
      onChange={(e) => {
        onParam(block.id, name, e.target.value);
        onParam(
          block.id,
          `${name}Name`,
          projects.find((p) => p.id === e.target.value)?.name ?? "",
        );
      }}
    >
      <option value="">{t("stepDetails.pickFlow")}</option>
      {projects.map((p) => (
        <option key={p.id} value={p.id}>{p.name}</option>
      ))}
    </select>
  );
}

/** Picks an entry point inside whichever project a sibling param names. */
function ProjectStepField({
  block,
  name,
  of,
  onParam,
}: {
  block: Block;
  name: string;
  of: string;
  onParam: (id: string, key: string, value: unknown) => void;
}) {
  const t = useT();
  const projects = useProjects();
  const chosen = projects.find((p) => p.id === String(block.params[of] ?? ""));
  const entries = (chosen?.blocks ?? []).filter((b) => b.kind === "flow.entry");
  return (
    <select
      className="h-8 flex-1 rounded-8 bg-bg-white-0 px-2 text-paragraph-sm text-text-strong-950 ring-1 ring-inset ring-stroke-soft-200 outline-none focus:ring-primary-base disabled:opacity-50"
      disabled={!chosen}
      value={String(block.params[name] ?? "")}
      onChange={(e) => onParam(block.id, name, e.target.value)}
    >
      <option value="">{t("stepDetails.entryDefault")}</option>
      {entries.map((b) => (
        <option key={b.id} value={b.id}>
          {String(b.params.name ?? "") || b.label || b.id.slice(0, 6)}
        </option>
      ))}
    </select>
  );
}

/** A searchable multi-select for a residential location dimension, with options
 *  loaded from ProxyShard and each level depending on the one above it. */
function ResiLocField({
  source,
  block,
  onChange,
}: {
  source: "country" | "region" | "city" | "isp";
  block: Block;
  onChange: (v: string) => void;
}) {
  const t = useT();
  const plan = String(block.params.plan ?? "standart");
  const country = single(block.params.country);
  const region = single(block.params.region);
  const city = single(block.params.city);
  const [opts, setOpts] = useState<MSOption[]>([]);
  const [loading, setLoading] = useState(false);

  // The parent picks this level needs. Empty when it cannot load yet.
  const gate =
    source === "country" ? plan
    : source === "region" ? (country ? `${plan}/${country}` : "")
    : source === "city" ? (country && region ? `${plan}/${country}/${region}` : "")
    : (plan === "premium" && country && region && city ? `${plan}/${country}/${region}/${city}` : "");

  useEffect(() => {
    if (!gate) { setOpts([]); return; }
    let alive = true;
    setLoading(true);
    const req =
      source === "country" ? psCountries(plan)
      : source === "region" ? psRegions(plan, country)
      : source === "city" ? psCities(plan, country, region)
      : psResiIsps(plan, country, region, city);
    req
      .then((r: { results?: { code: string; name: string }[] }) => {
        if (!alive) return;
        setOpts((r.results ?? []).map((l) => ({ value: l.code, label: l.name || l.code })));
      })
      .catch(() => alive && setOpts([]))
      .finally(() => alive && setLoading(false));
    return () => { alive = false; };
  }, [gate]); // eslint-disable-line react-hooks/exhaustive-deps

  const need =
    source === "region" && !country ? t("stepDetails.needCountry")
    : source === "city" && !region ? t("stepDetails.needRegion")
    : source === "isp" && plan !== "premium" ? t("stepDetails.ispPremiumOnly")
    : source === "isp" && !city ? t("stepDetails.needCity")
    : "";

  return (
    <MultiSelect
      value={String(block.params[source] ?? "")}
      onChange={onChange}
      options={opts}
      loading={loading}
      disabled={!!need}
      placeholder={need || t("stepDetails.anyPlaceholder")}
      // Only the country picks several (a random one each run). The deeper
      // levels are a single choice, and only when exactly one country is set.
      single={source !== "country"}
    />
  );
}

type Props = {
  block: Block | null;
  steps: Block[];
  onParam: (blockId: string, name: string, value: unknown) => void;
  onSecret: (blockId: string, name: string) => void;
  onBranch: (blockId: string, which: "on_done" | "on_fail", b: Branch) => void;
  onToggle: (blockId: string) => void;
};

function BranchField({
  title,
  value,
  steps,
  self,
  onChange,
}: {
  title: string;
  value: Branch;
  steps: Block[];
  self: string;
  onChange: (b: Branch) => void;
}) {
  const t = useT();
  const kind = branchKind(value);
  return (
    <div className="flex flex-col gap-1">
      <span className="text-subheading-2xs text-text-soft-400">{title}</span>
      <select
        className="h-8 rounded-8 bg-bg-white-0 px-2 text-paragraph-sm text-text-strong-950 ring-1 ring-inset ring-stroke-soft-200 outline-none focus:ring-primary-base"
        value={kind}
        onChange={(e) => {
          const k = e.target.value;
          if (k === "goto") {
            const first = steps.find((x) => x.id !== self);
            onChange({ goto: first?.id ?? "" });
          } else if (k === "retry") onChange({ retry: 2 });
          else onChange(k as Branch);
        }}
      >
        <option value="next">{t("stepDetails.branchNext")}</option>
        <option value="goto">{t("stepDetails.branchGoto")}</option>
        <option value="retry">{t("stepDetails.branchRetry")}</option>
        <option value="endpass">{t("stepDetails.branchEndPass")}</option>
        <option value="stop">{t("stepDetails.branchStop")}</option>
      </select>

      {kind === "goto" &&
        !steps.some((x) => x.id === (value as { goto: string }).goto) && (
          <div className="rounded-8 bg-warning-alpha-16 px-2 py-1 text-[11px] text-warning-base">
            {t("stepDetails.gotoMissing")}
          </div>
        )}

      {kind === "goto" && (
        <select
          className="h-8 rounded-8 bg-bg-white-0 px-2 text-paragraph-sm text-text-strong-950 ring-1 ring-inset ring-stroke-soft-200 outline-none focus:ring-primary-base"
          value={(value as { goto: string }).goto}
          onChange={(e) => onChange({ goto: e.target.value })}
        >
          {steps.map((x, i) => (
            <option key={x.id} value={x.id}>
              {i + 1}. {x.label || x.kind}
            </option>
          ))}
        </select>
      )}

      {kind === "retry" && (
        <input
          type="number" min={1} max={50}
          className="h-8 rounded-8 bg-bg-white-0 px-2 text-paragraph-sm text-text-strong-950 ring-1 ring-inset ring-stroke-soft-200 outline-none focus:ring-primary-base"
          value={(value as { retry: number }).retry}
          onChange={(e) => onChange({ retry: Math.max(1, Number(e.target.value) || 1) })}
        />
      )}
    </div>
  );
}

/** The selected block's settings. Kept out of the canvas so a long parameter
 *  list never changes where the cards are. */
export function StepDetails({ block, steps, onParam, onSecret, onBranch, onToggle }: Props) {
  const t = useT();
  if (!block) {
    return (
      <p className="m-0 py-8 text-center text-paragraph-xs text-text-soft-400">
        {t("stepDetails.emptyState")}
      </p>
    );
  }
  const spec = specFor(block.kind);

  return (
    <div className="flex flex-col gap-2">
      <div>
        <div className="text-label-sm text-text-strong-950">
          {block.label || (spec?.label ? t(spec.label) : "") || block.kind}
        </div>
        {spec?.about && (
          <div className="text-paragraph-xs text-text-soft-400">{t(spec.about)}</div>
        )}
      </div>

      {/* What this step actually aims at, said plainly. A step recorded by
          position is worth knowing about: it stops being right the moment the
          window is a different size. */}
      {(() => {
        const sel = block.params.selector;
        const hasXY = block.params.x !== undefined && block.params.y !== undefined;
        if (!sel && !hasXY) return null;
        return sel ? (
          <div className="rounded-8 bg-bg-weak-50 px-2 py-1.5">
            <div className="text-subheading-2xs text-text-soft-400">{t("stepDetails.targets")}</div>
            <code className="block break-all text-[11px] text-text-strong-950">
              {String(sel)}
            </code>
          </div>
        ) : (
          <div className="rounded-8 bg-warning-alpha-16 px-2 py-1.5">
            <div className="text-subheading-2xs text-warning-base">
              {t("stepDetails.byPosition", {
                x: Math.round(Number(block.params.x)),
                y: Math.round(Number(block.params.y)),
              })}
            </div>
            <div className="text-[11px] text-text-soft-400">
              {t("stepDetails.noSelectorHelp")}
            </div>
            {Array.isArray(block.params._tried) && block.params._tried.length > 0 && (
              <ul className="mt-1 list-none space-y-0.5 p-0 font-mono text-[10px] text-text-soft-400">
                {(block.params._tried as string[]).map((t, i) => (
                  <li key={i} className="break-all">{t}</li>
                ))}
              </ul>
            )}
          </div>
        );
      })()}

      <button
        type="button"
        className="self-start text-paragraph-xs text-text-soft-400 hover:text-text-strong-950"
        onClick={() => onToggle(block.id)}
      >
        {block.enabled ? t("stepDetails.enabledToggle") : t("stepDetails.skippedToggle")}
      </button>

      {(spec?.params ?? []).map((prm) => (
        <label key={prm.name} className="flex flex-col gap-1">
          <span className="text-subheading-2xs text-text-soft-400">{t(prm.label)}</span>
          <div className="flex items-center gap-1.5">
            {prm.kind === "column" ? (
              <ColumnField block={block} name={prm.name} of={prm.of ?? "path"} hint={prm.hint} optional={prm.optional} onParam={onParam} />
            ) : prm.kind === "file" ? (
              <FileField block={block} name={prm.name} hint={prm.hint} filters={prm.filters} onParam={onParam} />
            ) : prm.kind === "profile" ? (
              <ProfileField block={block} name={prm.name} hint={prm.hint} onParam={onParam} />
            ) : prm.kind === "project" ? (
              <ProjectField block={block} name={prm.name} onParam={onParam} />
            ) : prm.kind === "projectStep" ? (
              <ProjectStepField
                block={block}
                name={prm.name}
                of={prm.of ?? "project"}
                onParam={onParam}
              />
            ) : prm.kind === "resiloc" ? (
              <ResiLocField
                source={prm.name as "country" | "region" | "city" | "isp"}
                block={block}
                onChange={(v) => onParam(block.id, prm.name, v)}
              />
            ) : prm.kind === "textarea" ? (
              <textarea
                className="min-h-20 flex-1 rounded-8 bg-bg-white-0 p-2 font-mono text-[11px] text-text-strong-950 ring-1 ring-inset ring-stroke-soft-200 outline-none placeholder:text-text-soft-400 focus:ring-primary-base"
                placeholder={prm.hint ? t(prm.hint) : undefined}
                value={String(block.params[prm.name] ?? prm.default ?? "")}
                onChange={(e) => onParam(block.id, prm.name, e.target.value)}
              />
            ) : prm.kind === "select" && prm.options ? (
              <select
                className="h-8 flex-1 rounded-8 bg-bg-white-0 px-2 text-paragraph-sm text-text-strong-950 ring-1 ring-inset ring-stroke-soft-200 outline-none focus:ring-primary-base"
                value={String(block.params[prm.name] ?? prm.default ?? "")}
                onChange={(e) => onParam(block.id, prm.name, e.target.value)}
              >
                {prm.options.map((o) => (
                  <option key={o} value={o}>{o === "" ? t("stepDetails.anyOption") : o}</option>
                ))}
              </select>
            ) : (
              <input
                type={prm.kind === "number" ? "number" : "text"}
                className="h-8 flex-1 rounded-8 bg-bg-white-0 px-2 text-paragraph-sm text-text-strong-950 ring-1 ring-inset ring-stroke-soft-200 outline-none placeholder:text-text-soft-400 focus:ring-primary-base"
                placeholder={prm.hint ? t(prm.hint) : undefined}
                value={String(block.params[prm.name] ?? prm.default ?? "")}
                onChange={(e) =>
                  onParam(
                    block.id,
                    prm.name,
                    prm.kind === "number" ? Number(e.target.value) : e.target.value,
                  )
                }
              />
            )}
            {(prm.kind === "text" || prm.kind === "url" || prm.kind === "textarea") &&
              prm.name !== "into" &&
              prm.name !== "comment_into" &&
              !(block.kind === "var.set" && prm.name === "name") && (
                <VarButton
                  steps={steps}
                  value={String(block.params[prm.name] ?? prm.default ?? "")}
                  onPick={(next) => onParam(block.id, prm.name, next)}
                />
              )}
            {prm.secret && (
              <button
                type="button"
                title={
                  block.secrets.includes(prm.name)
                    ? t("stepDetails.secretOn")
                    : t("stepDetails.secretOff")
                }
                className={`rounded-8 px-1.5 py-1 text-[10px] ring-1 ring-inset ${
                  block.secrets.includes(prm.name)
                    ? "bg-warning-alpha-16 text-warning-base ring-warning-base"
                    : "text-text-soft-400 ring-stroke-soft-200 hover:text-text-strong-950"
                }`}
                onClick={() => onSecret(block.id, prm.name)}
              >
                {t("stepDetails.secretButton")}
              </button>
            )}
          </div>
        </label>
      ))}

      <div className="mt-1 flex flex-col gap-2 border-t border-stroke-soft-200 pt-2">
        <BranchField
          title={t("stepDetails.onDoneTitle")}
          value={block.on_done}
          steps={steps}
          self={block.id}
          onChange={(v) => onBranch(block.id, "on_done", v)}
        />
        <BranchField
          title={t("stepDetails.onFailTitle")}
          value={block.on_fail}
          steps={steps}
          self={block.id}
          onChange={(v) => onBranch(block.id, "on_fail", v)}
        />
      </div>
    </div>
  );
}
