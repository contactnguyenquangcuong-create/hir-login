import type { Picked } from "../../entities/automation";
import { useT } from "../../shared/i18n";

export type MenuAction = { kind: string; label: string; needsText?: boolean };

/** What a right-click offers for the element under it. Ordered by how often an
 *  operator wants each. */
const actions = (t: (key: string) => string): MenuAction[] => [
  { kind: "click", label: t("actionMenu.click") },
  { kind: "type", label: t("actionMenu.type"), needsText: true },
  { kind: "doubleClick", label: t("actionMenu.doubleClick") },
  { kind: "rightClick", label: t("actionMenu.rightClick") },
  { kind: "hover", label: t("actionMenu.hover") },
  { kind: "clear", label: t("actionMenu.clear") },
  { kind: "waitFor", label: t("actionMenu.waitFor") },
  { kind: "ifExists", label: t("actionMenu.ifExists") },
  { kind: "readText", label: t("actionMenu.readText"), needsText: true },
];

type Props = {
  at: { x: number; y: number };
  target: Picked | null;
  onChoose: (a: MenuAction) => void;
  onClose: () => void;
};

export function ActionMenu({ at, target, onChoose, onClose }: Props) {
  const t = useT();
  const what = target?.label
    ? `"${target.label}"`
    : target?.tag
      ? `<${target.tag}>`
      : t("actionMenu.thisPoint");

  return (
    <>
      <div className="fixed inset-0 z-40" onClick={onClose} onContextMenu={(e) => { e.preventDefault(); onClose(); }} />
      <div
        className="fixed z-50 w-[220px] overflow-hidden rounded-10 bg-bg-white-0 py-1 shadow-[var(--shadow-md)] ring-1 ring-stroke-soft-200"
        style={{ left: at.x, top: at.y }}
      >
        <div className="truncate px-3 py-1.5 text-paragraph-xs text-text-soft-400">
          {target?.selector ? what : t("actionMenu.byPosition", { what })}
        </div>
        {actions(t).map((a) => (
          <button
            key={a.kind}
            type="button"
            className="block w-full px-3 py-1.5 text-left text-paragraph-sm text-text-strong-950 hover:bg-bg-weak-50"
            onClick={() => { onChoose(a); onClose(); }}
          >
            {a.label}
          </button>
        ))}
      </div>
    </>
  );
}
