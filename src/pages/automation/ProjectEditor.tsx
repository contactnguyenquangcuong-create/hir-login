import { useEffect, useState } from "react";
import { Button } from "@proxyshard/shardx-ui-kit";
import { Topbar } from "../../shared/ui/Topbar";
import { AddIcon, CloseIcon, DownloadIcon, PlayIcon, StopIcon } from "../../shared/icons";
import {
  specFor,
  automationDisplay,
  automationExport,
  automationFleetWindow,
  automationRun,
  automationRunStatus,
  automationRunStop,
  scriptRun,
  useAutomation,
  type Block,
  type BlockSpec,
  type Branch,
  type DisplayInfo,
  type RunState,
} from "../../entities/automation";
import type { MenuAction } from "../../widgets/LiveView/ActionMenu";
import { BlockCanvas } from "../../widgets/BlockCanvas";
import { StepDetails } from "./StepDetails";
import { toast } from "../../shared/lib/toast";
import { BlockPicker } from "../../features/automation-blocks";
import { profileList } from "../../entities/profile/model/api";
import type { ProfileMeta } from "../../entities/profile/model/types";
import type { Picked } from "../../entities/automation";
import { LiveView } from "../../widgets/LiveView";
import { TrafficPanel } from "../../widgets/TrafficPanel";
import type { TrafficRule } from "../../entities/automation";
import { useT } from "../../shared/i18n";

export function ProjectEditor() {
  const t = useT();
  const project = useAutomation((s) => s.current());
  const open = useAutomation((s) => s.open);
  const patch = useAutomation((s) => s.patch);
  const addBlock = useAutomation((s) => s.addBlock);
  const placeBlock = useAutomation((s) => s.placeBlock);
  const connect = useAutomation((s) => s.connect);
  const undo = useAutomation((s) => s.undo);
  const redo = useAutomation((s) => s.redo);

  const [profiles, setProfiles] = useState<ProfileMeta[]>([]);
  const [target, setTarget] = useState<string | null>(null);
  const [picking, setPicking] = useState(false);
  const [pickAt, setPickAt] = useState<{ x: number; y: number } | null>(null);
  const [showLive, setShowLive] = useState(true);
  const [recording, setRecording] = useState(false);
  const [openStep, setOpenStep] = useState<string | null>(null);
  const [rightTab, setRightTab] = useState<"step" | "traffic">("step");
  const [consoleSrc, setConsoleSrc] = useState("");
  const [consoleWorld, setConsoleWorld] = useState<"isolated" | "main">("isolated");
  const [consoleOut, setConsoleOut] = useState("");
  const [run, setRun] = useState<RunState | null>(null);
  const [display, setDisplay] = useState<DisplayInfo | null>(null);
  useEffect(() => { automationDisplay().then(setDisplay).catch(() => {}); }, []);

  // Undo covers every change to the project, deleting a step included. Skipped
  // while a text field has focus, where the browser's own undo is what the
  // operator means.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const key = e.key.toLowerCase();
      if (!(e.metaKey || e.ctrlKey) || (key !== "z" && key !== "y")) return;
      const el = document.activeElement;
      const tag = el?.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA" || (el as HTMLElement)?.isContentEditable) return;
      e.preventDefault();
      if (key === "y" || e.shiftKey) void redo();
      else void undo();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [undo, redo]);

  // Poll only while something is going; a finished run keeps its last state on
  // screen so the log can still be read.
  useEffect(() => {
    if (!project) return;
    const tick = () => { automationRunStatus(project.id).then(setRun).catch(() => {}); };
    tick();
    const t = setInterval(tick, 800);
    return () => clearInterval(t);
  }, [project?.id]);
  useEffect(() => { profileList().then(setProfiles).catch(() => {}); }, []);

  if (!project) {
    open(null);
    return null;
  }

  // A right-click on the live view records a step against the point clicked.
  // Which element that point is over is resolved next, natively, through the
  // DOM domain — never by running script in the page.
  // Somewhere free: down the right of whatever is already there, so a step
  // added while the canvas is scrolled elsewhere is still findable.
  const freeSpot = () => {
    const bs = project.blocks;
    if (bs.length === 0) return { x: 80, y: 80 };
    const last = bs[bs.length - 1];
    return { x: last.x, y: last.y + 100 };
  };

  // Every step gets its block's defaults, however it was made. Recorded steps
  // used to be built by hand and so carried none — a recorded click had no
  // wait at all, while the same block added from the library had five seconds.
  const newBlock = (
    kind: string,
    params: Record<string, unknown>,
    label: string,
    at?: { x: number; y: number },
  ): Block => {
    const defaults: Record<string, unknown> = {};
    for (const prm of specFor(kind)?.params ?? []) {
      if (prm.default !== undefined) defaults[prm.name] = prm.default;
    }
    return {
      id: crypto.randomUUID(),
      kind,
      label,
      params: { ...defaults, ...params },
      enabled: true,
      secrets: [],
      ...(at ?? freeSpot()),
      on_done: "next",
      on_fail: "stop",
    };
  };

  // A right-click on the live view: the operator chose what to do with the
  // element the browser resolved under the point.
  const onAction = async (a: MenuAction, target: Picked | null) => {
    const params: Record<string, unknown> = target?.selector
      ? { selector: target.selector }
      : {
          x: Math.round(target?.x ?? 0),
          y: Math.round(target?.y ?? 0),
          // Kept on the step so the panel can say WHY it fell back.
          _tried: target?.tried ?? [],
        };
    const what = target?.label
      ? `"${target.label}"`
      : target?.tag
        ? `<${target.tag}>`
        : t("projectEditor.point");
    await addBlock(project.id, newBlock(a.kind, params, `${a.label.replace(/…$/, "")} ${what}`));
  };

  // Recording: the operator did something on the page and it becomes a step.
  const onRecorded = async (
    kind: string,
    target: Picked | null,
    extra?: Record<string, unknown>,
  ) => {
    const params: Record<string, unknown> = { ...(extra ?? {}) };
    if (target?.selector) params.selector = target.selector;
    else if (target) {
      params.x = Math.round(target.x);
      params.y = Math.round(target.y);
      params._tried = target.tried ?? [];
    }
    const what = target?.label
      ? `"${target.label}"`
      : target?.tag
        ? `<${target.tag}>`
        : String(extra?.key ?? "");
    const name =
      kind === "press"
        ? t("projectEditor.stepPress", { what })
        : kind === "scroll"
          ? Number(extra?.deltaY ?? 0) >= 0
            ? t("projectEditor.stepScrollDown")
            : t("projectEditor.stepScrollUp")
          : kind === "goto"
            ? t("projectEditor.stepOpen", { url: String(extra?.url ?? "").slice(0, 48) })
            : what
              ? t("projectEditor.stepClick", { what })
              : t("projectEditor.stepClickPoint");
    await addBlock(project.id, newBlock(kind, params, name));
  };

  const addFromLibrary = (spec: BlockSpec) => {
    const b = newBlock(spec.kind, {}, "", pickAt ?? undefined);
    setPickAt(null);
    addBlock(project.id, b);
    setOpenStep(b.id);
  };

  // The run follows the visual stack (see BlockCanvas / the runner sort), so a
  // drop only needs to place the card and make room: its position decides where
  // it sits in the flow. Its "done" link becomes "next" so it flows into the
  // card now beneath it — a stale goto from an earlier layout would otherwise
  // send the run somewhere else.
  const onDrop = (dragged: string, at: { under?: string; before?: string }) => {
    const CH = 60; // must match BlockCanvas CARD_H
    const blocks = project.blocks;
    const d = blocks.find((b) => b.id === dragged);
    if (!d) return;
    const anchorId = at.before ?? at.under ?? null;
    const anchor = anchorId ? blocks.find((b) => b.id === anchorId) : null;

    let x = d.x;
    let y = d.y;
    if (anchor) {
      x = anchor.x;
      y = at.before ? anchor.y : anchor.y + CH;
    }

    patch(project.id, {
      blocks: blocks.map((b) => {
        if (b.id === dragged) return { ...b, x, y, on_done: "next" as Branch };
        // Same column, at or below the new slot: slide down to make room.
        if (Math.abs(b.x - x) < 6 && b.y >= y) return { ...b, y: b.y + CH };
        return b;
      }),
    });
  };

  const toggleSecret = (blockId: string, name: string) =>
    patch(project.id, {
      blocks: project.blocks.map((x) =>
        x.id === blockId
          ? {
              ...x,
              secrets: x.secrets.includes(name)
                ? x.secrets.filter((n) => n !== name)
                : [...x.secrets, name],
            }
          : x,
      ),
    });

  const exportProject = async () => {
    try {
      const bundle = await automationExport(project.id);
      const blob = new Blob([JSON.stringify(bundle, null, 2)], { type: "application/json" });
      const a = document.createElement("a");
      a.href = URL.createObjectURL(blob);
      a.download = `${project.name.replace(/[^\w.-]+/g, "-")}.shardx-project.json`;
      a.click();
      URL.revokeObjectURL(a.href);
      const n = bundle.needs.length;
      toast.ok(
        n === 0
          ? t("projectEditor.exported")
          : n === 1
            ? t("projectEditor.exportedOneSecret")
            : t("projectEditor.exportedSecrets", { n }),
      );
    } catch (e) { toast.err(String(e)); }
  };

  const setBranch = (blockId: string, which: "on_done" | "on_fail", b: Branch) =>
    patch(project.id, {
      blocks: project.blocks.map((x) => (x.id === blockId ? { ...x, [which]: b } : x)),
    });

  const setParam = (blockId: string, name: string, value: unknown) =>
    patch(project.id, {
      blocks: project.blocks.map((b) =>
        b.id === blockId ? { ...b, params: { ...b.params, [name]: value } } : b,
      ),
    });

  const setRunOpts = (next: Partial<typeof project.run>) =>
    patch(project.id, { run: { ...project.run, ...next } });

  const setRules = (rules: TrafficRule[]) => patch(project.id, { rules });

  const runConsole = async () => {
    if (!target || !consoleSrc.trim()) return;
    try {
      const r = await scriptRun(target, consoleSrc, consoleWorld);
      setConsoleOut(r.result);
    } catch (e) {
      setConsoleOut("⚠ " + String(e));
    }
  };


  const removeBlock = (id: string) =>
    patch(project.id, { blocks: project.blocks.filter((b) => b.id !== id) });

  const toggleBlock = (id: string) =>
    patch(project.id, {
      blocks: project.blocks.map((b) => (b.id === id ? { ...b, enabled: !b.enabled } : b)),
    });

  return (
    <section className="flex h-full min-h-0 flex-col">
      <Topbar
        crumbs={[t("projectEditor.crumbWorkspace"), t("projectEditor.crumbAutomation"), project.name]}
        search=""
        onSearch={() => {}}
      />

      <div className="mb-3.5 flex items-end justify-between gap-4">
        <div className="min-w-0">
          <input
            className="m-0 w-full max-w-[36ch] truncate rounded-8 bg-transparent px-1 py-0.5 text-title-h5 text-text-strong-950 outline-none ring-1 ring-inset ring-transparent hover:ring-stroke-soft-200 focus:ring-primary-base"
            value={project.name}
            onChange={(e) => patch(project.id, { name: e.target.value })}
          />
          <input
            className="mt-1 w-full max-w-[70ch] rounded-8 bg-transparent px-1 py-0.5 text-paragraph-xs text-text-soft-400 outline-none ring-1 ring-inset ring-transparent hover:ring-stroke-soft-200 focus:ring-primary-base"
            placeholder={t("projectEditor.notesPlaceholder")}
            value={project.notes}
            onChange={(e) => patch(project.id, { notes: e.target.value })}
          />
        </div>
        <Button
          variant="neutral" mode="stroke" size="small"
          leftIcon={<CloseIcon className="size-4" />}
          onClick={() => open(null)}
        >
          {t("projectEditor.close")}
        </Button>
      </div>

      <div className="mb-3.5 flex flex-wrap items-end gap-4 rounded-12 bg-bg-white-0 px-4 py-3 shadow-[var(--shadow-xs)] ring-1 ring-inset ring-stroke-soft-200">
        <label className="flex flex-col gap-1">
          <span className="text-subheading-2xs text-text-soft-400">{t("projectEditor.threads")}</span>
          <input
            type="number" min={1} max={64}
            className="h-8 w-[90px] rounded-8 bg-bg-white-0 px-2 text-paragraph-sm text-text-strong-950 ring-1 ring-inset ring-stroke-soft-200 outline-none focus:ring-primary-base"
            value={project.run.threads}
            onChange={(e) => setRunOpts({ threads: Math.max(1, Number(e.target.value) || 1) })}
          />
        </label>
        <label className="flex flex-col gap-1">
          <span className="text-subheading-2xs text-text-soft-400">{t("projectEditor.loops")}</span>
          <input
            type="number" min={0}
            className="h-8 w-[90px] rounded-8 bg-bg-white-0 px-2 text-paragraph-sm text-text-strong-950 ring-1 ring-inset ring-stroke-soft-200 outline-none focus:ring-primary-base"
            value={project.run.loops}
            onChange={(e) => setRunOpts({ loops: Math.max(0, Number(e.target.value) || 0) })}
          />
        </label>
        {project.run.loops === 0 && (
          <label className="flex flex-col gap-1">
            <span className="text-subheading-2xs text-text-soft-400">{t("projectEditor.hours")}</span>
            <input
              type="number" min={0} step={0.5}
              className="h-8 w-[90px] rounded-8 bg-bg-white-0 px-2 text-paragraph-sm text-text-strong-950 ring-1 ring-inset ring-stroke-soft-200 outline-none focus:ring-primary-base"
              value={project.run.hours}
              onChange={(e) => setRunOpts({ hours: Math.max(0, Number(e.target.value) || 0) })}
            />
          </label>
        )}
        <p className="m-0 max-w-[38ch] text-paragraph-xs text-text-soft-400">
          {t("projectEditor.loopsHelp")}
        </p>

        <div className="ml-auto flex items-center gap-2">
          {run?.running ? (
            <Button
              variant="error" mode="filled" size="small"
              leftIcon={<StopIcon className="size-4" />}
              onClick={() => automationRunStop(project.id)}
            >
              {t("projectEditor.stop")}
            </Button>
          ) : (
            <Button
              variant="primary" mode="filled" size="small"
              leftIcon={<PlayIcon className="size-4" />}
              onClick={() =>
                automationRun(project.id).catch((e) => toast.err(String(e)))
              }
            >
              {t("projectEditor.run")}
            </Button>
          )}
          <Button
            variant="neutral" mode="stroke" size="small"
            onClick={() => automationFleetWindow()}
          >
            {t("projectEditor.fleet")}
          </Button>
          <Button
            variant="neutral" mode="stroke" size="small"
            leftIcon={<DownloadIcon className="size-4" />}
            onClick={exportProject}
          >
            {t("projectEditor.export")}
          </Button>
        </div>
      </div>

      {display?.limited && (
        <div className="mb-3.5 rounded-12 bg-warning-alpha-16 px-4 py-2.5 text-paragraph-xs text-warning-base">
          {display.note}
        </div>
      )}

      <div
        className={`grid min-h-0 flex-1 gap-3.5 ${
          showLive ? "grid-cols-[minmax(0,1fr)_300px_minmax(420px,42%)]" : "grid-cols-[1fr_300px]"
        }`}
      >
        <div className="flex min-h-0 flex-col gap-2">
          <div className="flex items-center justify-between">
            <span className="text-subheading-2xs text-text-soft-400">
              {project.blocks.length === 1
                ? t("projectEditor.stepCountOne")
                : t("projectEditor.stepCountMany", { n: project.blocks.length })}
            </span>
            <div className="flex items-center gap-1.5">
              <Button
                variant="neutral" mode="stroke" size="xsmall"
                leftIcon={<AddIcon className="size-4" />}
                onClick={() => { setPickAt(null); setPicking(true); }}
              >
                {t("projectEditor.addStep")}
              </Button>
              <Button
                variant="neutral" mode="ghost" size="xsmall"
                onClick={() => setShowLive((v) => !v)}
              >
                {showLive ? t("projectEditor.hideBrowser") : t("projectEditor.showBrowser")}
              </Button>
            </div>
          </div>
          <div className="min-h-0 flex-1">
            <BlockCanvas
              blocks={project.blocks}
              startId={project.run.start}
              selected={openStep}
              active={
                run?.workers.find((w) => w.status === "running" && w.step > 0)
                  ? project.blocks
                      .filter((b) => b.enabled)
                      .sort((a, b) => Math.round(a.x / 40) - Math.round(b.x / 40) || a.y - b.y)[
                      (run.workers.find((w) => w.status === "running")?.step ?? 1) - 1
                    ]?.id ?? null
                  : null
              }
              onSelect={(id) => { setOpenStep(id); if (id) setRightTab("step"); }}
              onMove={(id, x, y) => placeBlock(project.id, id, x, y)}
              onConnect={(from, port, to) => connect(project.id, from, port, to)}
              onDrop={onDrop}
              onSetStart={(id) => setRunOpts({ start: id })}
              onDelete={removeBlock}
              onAddAt={(x, y) => { setPickAt({ x, y }); setPicking(true); }}
            />
          </div>
        </div>

        <div className="flex min-h-0 flex-col gap-2 overflow-y-auto rounded-12 bg-bg-white-0 p-3 shadow-[var(--shadow-xs)] ring-1 ring-inset ring-stroke-soft-200">
          <div className="flex gap-1">
            {(["step", "traffic"] as const).map((tab) => (
              <button
                key={tab}
                onClick={() => setRightTab(tab)}
                className={
                  "rounded-8 px-2 py-1 text-label-sm " +
                  (rightTab === tab
                    ? "bg-bg-weak-50 text-text-strong-950"
                    : "text-text-soft-400 hover:text-text-strong-950")
                }
              >
                {tab === "step" ? t("projectEditor.tabStep") : t("projectEditor.tabTraffic")}
                {tab === "traffic" && (project.rules?.length ? ` (${project.rules.length})` : "")}
              </button>
            ))}
          </div>
          {/* Both stay mounted; we only hide the inactive one. Unmounting the
              Traffic panel on a tab switch threw away its observed requests and
              the "observing" toggle (while the core kept observing). */}
          <div hidden={rightTab !== "traffic"} className="flex min-h-0 flex-col gap-2">
            <TrafficPanel
              rules={project.rules ?? []}
              onRulesChange={setRules}
              onAddStep={(kind, params, label) => {
                const b = newBlock(kind, params, label);
                addBlock(project.id, b);
                setRightTab("step");
                setOpenStep(b.id);
              }}
              profileId={target}
              live={!!target}
            />
          </div>
          <div hidden={rightTab !== "step"} className="flex min-h-0 flex-col gap-2">
            <StepDetails
              block={project.blocks.find((b) => b.id === openStep) ?? null}
              steps={project.blocks}
              onParam={setParam}
              onSecret={toggleSecret}
              onBranch={setBranch}
              onToggle={toggleBlock}
            />
          </div>
          {run && (
            <div className="mt-auto border-t border-stroke-soft-200 pt-2">
              <div className="mb-1 flex items-center justify-between">
                <span className="text-subheading-2xs text-text-soft-400">{t("projectEditor.runLog")}</span>
                <span className="text-paragraph-xs text-text-soft-400">
                  {t("projectEditor.runningCount", {
                    n: run.workers.filter((w) => w.status === "running").length,
                  })}{" "}
                  ·{" "}
                  {t("projectEditor.failedCount", {
                    n: run.workers.filter((w) => w.status === "failed").length,
                  })}
                </span>
              </div>
              <div className="max-h-[150px] overflow-y-auto font-mono text-[10px] text-text-sub-600">
                {run.log.length === 0 ? (
                  <div className="text-text-soft-400">{t("projectEditor.logEmpty")}</div>
                ) : (
                  run.log.slice(-60).map((l, i) => <div key={i}>{l}</div>)
                )}
              </div>
            </div>
          )}
        </div>

        {showLive && (
          <div className="flex min-h-0 flex-col gap-2">
            <select
              className="h-8 w-full rounded-8 bg-bg-white-0 px-2 text-paragraph-sm text-text-strong-950 ring-1 ring-inset ring-stroke-soft-200 outline-none focus:ring-primary-base"
              value={target ?? ""}
              onChange={(e) => setTarget(e.target.value || null)}
            >
              <option value="">{t("projectEditor.chooseProfile")}</option>
              {profiles.map((p) => (
                <option key={p.id} value={p.id}>{p.name}</option>
              ))}
            </select>
            <div className="min-h-0 flex-1">
              <LiveView
                profileId={target}
                onAction={onAction}
                onRecorded={onRecorded}
                recording={recording}
                onRecording={setRecording}
              />
            </div>
            {/* Console: run JS in the page through the core's isolated world. */}
            <div className="flex flex-col gap-1 rounded-12 bg-bg-white-0 p-2 ring-1 ring-inset ring-stroke-soft-200">
              <div className="flex items-center gap-2">
                <span className="text-subheading-2xs text-text-soft-400">{t("projectEditor.console")}</span>
                <select
                  className="h-7 rounded-8 bg-bg-white-0 px-1.5 text-paragraph-xs text-text-strong-950 ring-1 ring-inset ring-stroke-soft-200 outline-none focus:ring-primary-base"
                  value={consoleWorld}
                  onChange={(e) => setConsoleWorld(e.target.value as "isolated" | "main")}
                >
                  <option value="isolated">isolated</option>
                  <option value="main">main</option>
                </select>
                <Button size="xsmall" className="ml-auto" disabled={!target} onClick={runConsole}>
                  {t("projectEditor.consoleRun")}
                </Button>
              </div>
              <input
                className="h-8 w-full rounded-8 bg-bg-white-0 px-2 font-mono text-[11px] text-text-strong-950 ring-1 ring-inset ring-stroke-soft-200 outline-none placeholder:text-text-soft-400 focus:ring-primary-base"
                placeholder="document.title"
                value={consoleSrc}
                onChange={(e) => setConsoleSrc(e.target.value)}
                onKeyDown={(e) => { if (e.key === "Enter") runConsole(); }}
              />
              {consoleOut && (
                <pre className="m-0 max-h-24 overflow-auto rounded-8 bg-bg-weak-50 p-2 font-mono text-[11px] text-text-strong-950">{consoleOut}</pre>
              )}
            </div>
          </div>
        )}
      </div>

      {picking && (
        <BlockPicker onPick={addFromLibrary} onClose={() => setPicking(false)} />
      )}
    </section>
  );
}
