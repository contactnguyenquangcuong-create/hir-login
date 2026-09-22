import { useEffect, useState } from "react";
import { Modal } from "@proxyshard/shardx-ui-kit";
import {
  automationModules,
  PALETTE,
  type BlockSpec,
  type ModuleInfo,
} from "../../../entities/automation";
import { useT } from "../../../shared/i18n";

type Props = { onPick: (spec: BlockSpec) => void; onClose: () => void };

/** The library, grouped. Adding a step by hand is the ordinary way to build a
 *  project; recording from the live browser is the shortcut. */
export function BlockPicker({ onPick, onClose }: Props) {
  const t = useT();
  const [cat, setCat] = useState(PALETTE[0].id);
  const [q, setQ] = useState("");
  const [modules, setModules] = useState<ModuleInfo[]>([]);
  useEffect(() => { automationModules().then(setModules).catch(() => {}); }, []);

  // A module's blocks sit in the library beside the built-in ones. Each block
  // chooses its group: a built-in one (by label or id), a new named group, or —
  // with none set — the module's own group. Their kind carries the module id,
  // so the runner knows who to ask.
  const builtinByName = new Map<string, string>(); // lowercased name/id -> real id
  for (const c of PALETTE) {
    builtinByName.set(c.id.toLowerCase(), c.id);
    builtinByName.set(t(c.label).toLowerCase(), c.id);
  }
  const extra = new Map<string, { id: string; label: string; blocks: BlockSpec[] }>();
  for (const m of modules) {
    if (m.error || !m.blocks.length) continue;
    for (const b of m.blocks) {
      const spec: BlockSpec = {
        kind: `module:${m.id}:${b.kind}`,
        label: b.label ?? b.kind,
        about: b.about ?? t("blockPicker.fromModule", { name: m.name }),
        params: (b.params ?? []).map((prm) => ({
          name: prm.name,
          label: prm.label ?? prm.name,
          kind: (prm.kind as BlockSpec["params"][number]["kind"]) ?? "text",
          hint: prm.hint,
          default: prm.default as string | number | undefined,
          secret: prm.secret === true,
        })),
      };
      const wanted = (b.group ?? "").trim();
      const builtin = wanted ? builtinByName.get(wanted.toLowerCase()) : undefined;
      const key = builtin ?? (wanted ? `group:${wanted.toLowerCase()}` : `module:${m.id}`);
      const label = builtin ? "" : wanted || m.name; // built-in keeps its own label
      if (!extra.has(key)) extra.set(key, { id: key, label, blocks: [] });
      extra.get(key)!.blocks.push(spec);
    }
  }
  const broken = modules.filter((m) => m.error);
  // Built-in categories, with module blocks that chose them appended; then any
  // new named / per-module groups.
  const all: { id: string; label: string; blocks: BlockSpec[] }[] = [
    ...PALETTE.map((c) => {
      const add = extra.get(c.id);
      return add ? { ...c, blocks: [...c.blocks, ...add.blocks] } : c;
    }),
    ...[...extra.values()].filter((g) => !PALETTE.some((c) => c.id === g.id)),
  ];

  const query = q.trim().toLowerCase();
  const groups = query
    ? all.map((c) => ({
        ...c,
        blocks: c.blocks.filter(
          (b) =>
            // Searched on what the person sees, not on the key behind it.
            t(b.label).toLowerCase().includes(query) ||
            t(b.about).toLowerCase().includes(query),
        ),
      })).filter((c) => c.blocks.length > 0)
    : all.filter((c) => c.id === cat);

  return (
    <Modal open onClose={onClose}>
      <div className="flex h-[520px] w-[720px] max-w-full flex-col gap-3 p-5">
        <div className="flex items-center justify-between gap-3">
          <h2 className="m-0 text-title-h6 text-text-strong-950">{t("blockPicker.title")}</h2>
          <input
            className="h-8 w-[240px] rounded-8 bg-bg-white-0 px-2.5 text-paragraph-sm text-text-strong-950 ring-1 ring-inset ring-stroke-soft-200 outline-none placeholder:text-text-soft-400 focus:ring-primary-base"
            placeholder={t("blockPicker.searchPlaceholder")}
            value={q}
            onChange={(e) => setQ(e.target.value)}
            autoFocus
          />
        </div>

        <div className="flex min-h-0 flex-1 gap-3">
          {!query && (
            <div className="flex w-[150px] shrink-0 flex-col gap-1">
              {all.map((c) => (
                <button
                  key={c.id}
                  type="button"
                  className={`rounded-8 px-2.5 py-1.5 text-left text-label-sm transition-colors ${
                    c.id === cat
                      ? "bg-primary-alpha-10 text-primary-base"
                      : "text-text-sub-600 hover:bg-bg-weak-50"
                  }`}
                  onClick={() => setCat(c.id)}
                >
                  {t(c.label)}
                </button>
              ))}
            </div>
          )}

          <div className="min-h-0 flex-1 overflow-y-auto pr-1">
            {groups.map((c) => (
              <div key={c.id} className="mb-3 last:mb-0">
                {query && (
                  <div className="mb-1 text-subheading-2xs text-text-soft-400">{t(c.label)}</div>
                )}
                <div className="flex flex-col gap-1.5">
                  {c.blocks.map((b) => (
                    <button
                      key={b.kind}
                      type="button"
                      className="rounded-10 px-3 py-2 text-left ring-1 ring-inset ring-stroke-soft-200 transition-colors hover:bg-bg-weak-50 hover:ring-primary-base"
                      onClick={() => { onPick(b); onClose(); }}
                    >
                      <div className="text-label-sm text-text-strong-950">{t(b.label)}</div>
                      <div className="text-paragraph-xs text-text-soft-400">{t(b.about)}</div>
                    </button>
                  ))}
                </div>
              </div>
            ))}
            {broken.length > 0 && !query && (
              <div className="mt-2 rounded-8 bg-warning-alpha-16 px-2.5 py-1.5 text-paragraph-xs text-warning-base">
                {broken.length === 1
                  ? t("blockPicker.modulesFailedOne", { n: broken.length })
                  : t("blockPicker.modulesFailedMany", { n: broken.length })}{" "}
                {broken.map((m) => `${m.id} (${m.error})`).join(", ")}
              </div>
            )}
            {groups.length === 0 && (
              <p className="m-0 px-1 py-6 text-center text-paragraph-sm text-text-soft-400">
                {t("blockPicker.noMatches", { q })}
              </p>
            )}
          </div>
        </div>
      </div>
    </Modal>
  );
}
