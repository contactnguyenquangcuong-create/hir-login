import { useEffect, useState } from "react";
import {
  automationList,
  branchKind,
  specFor,
  type Block,
  type Branch,
  type Project,
} from "../../entities/automation";
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
            {prm.kind === "project" ? (
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
