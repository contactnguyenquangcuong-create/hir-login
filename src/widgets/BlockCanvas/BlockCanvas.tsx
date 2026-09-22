import { useCallback, useEffect, useRef, useState } from "react";
import {
  branchKind,
  specFor,
  type Block,
} from "../../entities/automation";
import { t, useT } from "../../shared/i18n";

const CARD_W = 210;
// A multiple of the grid, so cards stacked flush land on it and stay docked.
const CARD_H = 60;
const GRID = 20;
const PORT_DONE = 21;
const PORT_FAIL = 43;
/// How close counts as docked.
const SNAP = 12;

type Port = "done" | "fail";

type Props = {
  blocks: Block[];
  startId: string;
  selected: string | null;
  /** Highlighted while a run is on this step. */
  active?: string | null;
  onSelect: (id: string | null) => void;
  onMove: (id: string, x: number, y: number) => void;
  onConnect: (from: string, port: Port, to: string | null) => void;
  /** A card was let go over the stack: docked under one, or put in another's
   *  place so everything below shifts down. */
  onDrop: (dragged: string, at: { under?: string; before?: string }) => void;
  onSetStart: (id: string) => void;
  onDelete: (id: string) => void;
  /** Canvas point where the operator asked for a new block. */
  onAddAt: (x: number, y: number) => void;
};

// The spec's label is a translation key; a block's own label is the name the
// operator gave it when the step was made, and stays as typed.
function title(b: Block): string {
  const spec = specFor(b.kind)?.label;
  return b.label || (spec ? t(spec) : "") || b.kind;
}

/** Where an edge leaves a card, and where it arrives. */
function portPoint(b: Block, port: Port) {
  return { x: b.x + CARD_W, y: b.y + (port === "done" ? PORT_DONE : PORT_FAIL) };
}
function inPoint(b: Block) {
  return { x: b.x, y: b.y + CARD_H / 2 };
}

/** A cubic that leaves rightwards and arrives rightwards, so an edge doubling
 *  back reads as a loop rather than a straight line through the cards. */
function edgePath(a: { x: number; y: number }, b: { x: number; y: number }) {
  const dx = Math.max(50, Math.abs(b.x - a.x) * 0.5);
  return `M ${a.x} ${a.y} C ${a.x + dx} ${a.y}, ${b.x - dx} ${b.y}, ${b.x} ${b.y}`;
}

export function BlockCanvas({
  blocks,
  startId,
  selected,
  active,
  onSelect,
  onMove,
  onConnect,
  onDrop,
  onSetStart,
  onDelete,
  onAddAt,
}: Props) {
  const t = useT();
  const host = useRef<HTMLDivElement | null>(null);
  const [view, setView] = useState({ x: 40, y: 40, k: 1 });
  const [hint, setHint] = useState<{ under?: string; before?: string } | null>(null);
  // Cards picked out with a right-drag box, moved together. `boxed` is true once
  // a right-drag actually swept an area, so the context menu can tell a box
  // select apart from a plain right-click (which adds a step).
  const [sel, setSel] = useState<Set<string>>(new Set());
  const boxed = useRef(false);
  const [drag, setDrag] = useState<
    | { kind: "pan"; sx: number; sy: number; ox: number; oy: number }
    | { kind: "card"; id: string; dx: number; dy: number; ox: number; oy: number; freed: boolean }
    | { kind: "multi"; px: number; py: number; start: Map<string, { x: number; y: number }> }
    | { kind: "marquee"; sx: number; sy: number; x: number; y: number }
    | { kind: "wire"; from: string; port: Port; x: number; y: number }
    | null
  >(null);

  const byId = new Map(blocks.map((b) => [b.id, b]));
  // The flow runs the visual stack top-to-bottom (by column, then height), so
  // ordering — the start marker, "next" links, edges — is read off positions,
  // not the list order. Must match the runner's own sort.
  const ordered = [...blocks].sort((a, b) => {
    const ca = Math.round(a.x / 40);
    const cb = Math.round(b.x / 40);
    return ca - cb || a.y - b.y;
  });
  const orderIndex = new Map(ordered.map((b, i) => [b.id, i]));
  const entry = startId && byId.has(startId) ? startId : ordered[0]?.id;

  /** Screen point -> canvas point. */
  const toCanvas = useCallback(
    (clientX: number, clientY: number) => {
      const r = host.current?.getBoundingClientRect();
      if (!r) return { x: 0, y: 0 };
      return { x: (clientX - r.left - view.x) / view.k, y: (clientY - r.top - view.y) / view.k };
    },
    [view],
  );

  useEffect(() => {
    if (!drag) return;
    const move = (e: MouseEvent) => {
      if (drag.kind === "pan") {
        setView((v) => ({ ...v, x: drag.ox + (e.clientX - drag.sx), y: drag.oy + (e.clientY - drag.sy) }));
        return;
      }
      const p = toCanvas(e.clientX, e.clientY);
      if (drag.kind === "card") {
        let x = Math.round((p.x - drag.dx) / GRID) * GRID;
        let y = Math.round((p.y - drag.dy) / GRID) * GRID;

        // A card must come loose before it can be caught again, or a stacked
        // one snaps straight back and the chain cannot be broken at all.
        const freed =
          drag.freed || Math.hypot(x - drag.ox, y - drag.oy) > CARD_H * 0.6;
        if (freed !== drag.freed) setDrag({ ...drag, freed: true });

        let next: { under?: string; before?: string } | null = null;
        if (freed) {
          for (const other of blocks) {
            if (other.id === drag.id) continue;
            if (Math.abs(other.x - x) > SNAP * 2) continue;
            // Dropped where a card already is: take its place and push it down.
            if (Math.abs(other.y - y) <= SNAP * 1.6) {
              x = other.x;
              y = other.y;
              next = { before: other.id };
              break;
            }
            // Dropped just under one: join the bottom of that stack.
            if (Math.abs(other.y + CARD_H - y) <= SNAP * 1.6) {
              x = other.x;
              y = other.y + CARD_H;
              next = { under: other.id };
              break;
            }
          }
        }
        setHint(next);
        onMove(drag.id, x, y);
      } else if (drag.kind === "multi") {
        // Carry the whole boxed group by the same offset.
        const dx = p.x - drag.px;
        const dy = p.y - drag.py;
        for (const [id, s] of drag.start) {
          onMove(id, Math.round((s.x + dx) / GRID) * GRID, Math.round((s.y + dy) / GRID) * GRID);
        }
      } else if (drag.kind === "marquee") {
        if (Math.hypot(p.x - drag.sx, p.y - drag.sy) > 4) boxed.current = true;
        setDrag({ ...drag, x: p.x, y: p.y });
      } else {
        setDrag({ ...drag, x: p.x, y: p.y });
      }
    };
    const up = (e: MouseEvent) => {
      if (drag.kind === "card" && hint) {
        onDrop(drag.id, hint);
      }
      setHint(null);
      if (drag.kind === "marquee") {
        // Everything inside the box becomes the selection.
        const x0 = Math.min(drag.sx, drag.x);
        const x1 = Math.max(drag.sx, drag.x);
        const y0 = Math.min(drag.sy, drag.y);
        const y1 = Math.max(drag.sy, drag.y);
        const inside = blocks
          .filter((b) => b.x + CARD_W > x0 && b.x < x1 && b.y + CARD_H > y0 && b.y < y1)
          .map((b) => b.id);
        setSel(new Set(inside));
      }
      if (drag.kind === "wire") {
        const p = toCanvas(e.clientX, e.clientY);
        // Dropped on a card connects; dropped on empty space clears the link.
        const hit = blocks.find(
          (b) => p.x >= b.x && p.x <= b.x + CARD_W && p.y >= b.y && p.y <= b.y + CARD_H,
        );
        onConnect(drag.from, drag.port, hit && hit.id !== drag.from ? hit.id : null);
      }
      setDrag(null);
    };
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
    return () => {
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
    };
  }, [drag, hint, blocks, onMove, onConnect, onDrop, toCanvas]);

  const onWheel = (e: React.WheelEvent) => {
    e.preventDefault();
    const r = host.current?.getBoundingClientRect();
    if (!r) return;
    const mx = e.clientX - r.left;
    const my = e.clientY - r.top;
    setView((v) => {
      // Zoom about the cursor, so the thing under it stays under it.
      const k = Math.min(2.2, Math.max(0.25, v.k * (e.deltaY < 0 ? 1.1 : 1 / 1.1)));
      return { k, x: mx - ((mx - v.x) / v.k) * k, y: my - ((my - v.y) / v.k) * k };
    });
  };

  /** The block each one hands over to when it works — implicit (the card below)
   *  or explicit (a goto). */
  const doneTarget = (b: Block): Block | undefined => {
    const kind = branchKind(b.on_done);
    if (kind === "goto") return byId.get((b.on_done as { goto: string }).goto);
    if (kind === "next") {
      const oi = orderIndex.get(b.id) ?? -1;
      return oi >= 0 ? ordered[oi + 1] : undefined;
    }
    return undefined;
  };

  /** Cards sitting flush under the one they follow are drawn as one stack, so
   *  an arrow that would curl out and straight back is simply not drawn. */
  const dockedTo = new Map<string, string>();
  blocks.forEach((b) => {
    const t = doneTarget(b);
    if (!t) return;
    if (Math.abs(t.x - b.x) <= SNAP && Math.abs(t.y - (b.y + CARD_H)) <= SNAP) {
      dockedTo.set(b.id, t.id);
    }
  });
  const dockedUnder = new Set(dockedTo.values());

  /** Every connection, as a line to draw. */
  const edges: { from: Block; to: Block; port: Port }[] = [];
  blocks.forEach((b) => {
    // The join itself shows the hand-over; a line as well is noise.
    if (!dockedTo.has(b.id)) {
      const t = doneTarget(b);
      if (t) edges.push({ from: b, to: t, port: "done" });
    }
    const failKind = branchKind(b.on_fail);
    if (failKind === "goto") {
      const t = byId.get((b.on_fail as { goto: string }).goto);
      if (t) edges.push({ from: b, to: t, port: "fail" });
    }
  });

  return (
    <div
      ref={host}
      className="relative h-full w-full overflow-hidden rounded-12 bg-bg-weak-50 ring-1 ring-inset ring-stroke-soft-200"
      onWheel={onWheel}
      onMouseDown={(e) => {
        if (e.target !== e.currentTarget) return;
        if (e.button === 2) {
          // Right-drag draws a selection box; a right-click (no drag) still
          // opens the add-a-step menu (handled in onContextMenu).
          boxed.current = false;
          const p = toCanvas(e.clientX, e.clientY);
          setSel(new Set());
          setDrag({ kind: "marquee", sx: p.x, sy: p.y, x: p.x, y: p.y });
          return;
        }
        if (e.button !== 0) return;
        onSelect(null);
        setSel(new Set());
        setDrag({ kind: "pan", sx: e.clientX, sy: e.clientY, ox: view.x, oy: view.y });
      }}
      onContextMenu={(e) => {
        e.preventDefault();
        // A right-drag that swept a box selected cards — don't also add a step.
        if (boxed.current) { boxed.current = false; return; }
        const p = toCanvas(e.clientX, e.clientY);
        onAddAt(Math.round(p.x / GRID) * GRID, Math.round(p.y / GRID) * GRID);
      }}
      style={{
        backgroundImage:
          "radial-gradient(circle, var(--stroke-soft-200, #d1d5db) 1px, transparent 1px)",
        backgroundSize: `${GRID * view.k}px ${GRID * view.k}px`,
        backgroundPosition: `${view.x}px ${view.y}px`,
        cursor: drag?.kind === "pan" ? "grabbing" : "default",
      }}
    >
      <div
        className="pointer-events-none absolute left-0 top-0 origin-top-left"
        style={{ transform: `translate(${view.x}px, ${view.y}px) scale(${view.k})` }}
      >
        <svg className="pointer-events-none absolute overflow-visible" style={{ width: 1, height: 1 }}>
          <defs>
            <marker id="bc-done" markerWidth="8" markerHeight="8" refX="7" refY="4" orient="auto">
              <path d="M0,0 L8,4 L0,8 z" className="fill-success-base" />
            </marker>
            <marker id="bc-fail" markerWidth="8" markerHeight="8" refX="7" refY="4" orient="auto">
              <path d="M0,0 L8,4 L0,8 z" className="fill-error-base" />
            </marker>
          </defs>
          {edges.map((e, i) => {
            return (
              <path
                key={i}
                d={edgePath(portPoint(e.from, e.port), inPoint(e.to))}
                fill="none"
                strokeWidth={1.8}
                className={e.port === "done" ? "stroke-success-base" : "stroke-error-base"}
                markerEnd={`url(#bc-${e.port})`}
                opacity={0.75}
              />
            );
          })}
          {drag?.kind === "wire" && byId.get(drag.from) && (
            <path
              d={edgePath(portPoint(byId.get(drag.from)!, drag.port), { x: drag.x, y: drag.y })}
              fill="none"
              strokeWidth={1.8}
              strokeDasharray="4 3"
              className={drag.port === "done" ? "stroke-success-base" : "stroke-error-base"}
            />
          )}
          {drag?.kind === "marquee" && (
            <rect
              x={Math.min(drag.sx, drag.x)}
              y={Math.min(drag.sy, drag.y)}
              width={Math.abs(drag.x - drag.sx)}
              height={Math.abs(drag.y - drag.sy)}
              className="fill-primary-alpha-10 stroke-primary-base"
              strokeWidth={1}
            />
          )}
        </svg>

        {hint && (() => {
          const t = byId.get(hint.before ?? hint.under ?? "");
          if (!t) return null;
          const y = hint.before ? t.y : t.y + CARD_H;
          return (
            <div
              className="pointer-events-none absolute h-0.5 rounded-full bg-primary-base"
              style={{ left: t.x, top: y - 1, width: CARD_W }}
            />
          );
        })()}

        {blocks.map((b) => (
          <div
            key={b.id}
            className={`pointer-events-auto absolute bg-bg-white-0 shadow-[var(--shadow-xs)] ring-1 transition-shadow ${
              dockedUnder.has(b.id) ? "rounded-t-none" : "rounded-t-10"
            } ${dockedTo.has(b.id) ? "rounded-b-none" : "rounded-b-10"} ${
              active === b.id
                ? "z-10 ring-2 ring-success-base"
                : selected === b.id || sel.has(b.id)
                  ? "z-10 ring-2 ring-primary-base"
                  : "ring-stroke-soft-200"
            } ${b.enabled ? "" : "opacity-50"}`}
            style={{ left: b.x, top: b.y, width: CARD_W, height: CARD_H }}
            onMouseDown={(e) => {
              if (e.button !== 0) return;
              e.stopPropagation();
              const p = toCanvas(e.clientX, e.clientY);
              // Grabbing a boxed card carries the whole group; grabbing any
              // other card is a single move and drops the box selection.
              if (sel.has(b.id) && sel.size > 1) {
                const start = new Map<string, { x: number; y: number }>();
                for (const id of sel) {
                  const bb = byId.get(id);
                  if (bb) start.set(id, { x: bb.x, y: bb.y });
                }
                setDrag({ kind: "multi", px: p.x, py: p.y, start });
                return;
              }
              setSel(new Set());
              onSelect(b.id);
              setDrag({
                kind: "card",
                id: b.id,
                dx: p.x - b.x,
                dy: p.y - b.y,
                ox: b.x,
                oy: b.y,
                freed: false,
              });
            }}
          >
            <div className="flex h-full flex-col justify-center gap-0.5 px-2.5">
              <div className="flex items-center gap-1.5">
                {entry === b.id && (
                  <span className="rounded-4 bg-primary-alpha-10 px-1 text-[10px] text-primary-base">
                    {t("blockCanvas.startBadge")}
                  </span>
                )}
                <span className="truncate text-label-xs text-text-strong-950">{title(b)}</span>
              </div>
              <span className="truncate text-[10px] text-text-soft-400">
                {typeof b.params.selector === "string" && b.params.selector
                  ? b.params.selector
                  : b.params.x !== undefined
                    ? t("blockCanvas.byPosition", {
                        x: Math.round(Number(b.params.x)),
                        y: Math.round(Number(b.params.y)),
                      })
                    : b.kind}
              </span>
            </div>

            {/* Ports: drag from one onto another card to connect. */}
            {/* A hairline where two cards meet, so a stack still reads as steps. */}
            {dockedUnder.has(b.id) && (
              <div className="absolute left-2.5 right-2.5 top-0 h-px bg-stroke-soft-200" />
            )}

            {(["done", "fail"] as Port[]).filter((port) => !(port === "done" && dockedTo.has(b.id))).map((port) => (
              <button
                key={port}
                type="button"
                title={port === "done" ? t("blockCanvas.portDone") : t("blockCanvas.portFail")}
                className={`absolute size-3 rounded-full ring-2 ring-bg-white-0 ${
                  port === "done" ? "bg-success-base" : "bg-error-base"
                }`}
                style={{
                  left: CARD_W - 6,
                  top: (port === "done" ? PORT_DONE : PORT_FAIL) - 6,
                }}
                onMouseDown={(e) => {
                  e.stopPropagation();
                  const p = toCanvas(e.clientX, e.clientY);
                  setDrag({ kind: "wire", from: b.id, port, x: p.x, y: p.y });
                }}
              />
            ))}
            {!dockedUnder.has(b.id) && (
              <div
                className="absolute size-3 rounded-full bg-stroke-soft-200 ring-2 ring-bg-white-0"
                style={{ left: -6, top: CARD_H / 2 - 6 }}
              />
            )}

            {selected === b.id && (
              <div className="absolute -top-7 left-0 flex gap-1">
                <button
                  type="button"
                  className="rounded-6 bg-bg-white-0 px-1.5 py-0.5 text-[10px] text-text-sub-600 ring-1 ring-stroke-soft-200 hover:text-text-strong-950"
                  onClick={(e) => { e.stopPropagation(); onSetStart(b.id); }}
                >
                  {t("blockCanvas.startHere")}
                </button>
                <button
                  type="button"
                  className="rounded-6 bg-bg-white-0 px-1.5 py-0.5 text-[10px] text-error-base ring-1 ring-stroke-soft-200"
                  onClick={(e) => { e.stopPropagation(); onDelete(b.id); }}
                >
                  {t("blockCanvas.delete")}
                </button>
              </div>
            )}
          </div>
        ))}

        {/* A cut handle at each edge's middle — clicking it drops the link.
            (The SVG lines themselves can't be clicked reliably.) */}
        {edges.map((e, i) => {
          const a = portPoint(e.from, e.port);
          const to = inPoint(e.to);
          return (
            <button
              key={"cut" + i}
              type="button"
              title={t("blockCanvas.disconnect")}
              className="pointer-events-auto absolute z-20 flex size-4 items-center justify-center rounded-full bg-bg-white-0 text-[11px] leading-none text-error-base opacity-60 ring-1 ring-stroke-soft-200 hover:opacity-100 hover:ring-error-base"
              style={{ left: (a.x + to.x) / 2 - 8, top: (a.y + to.y) / 2 - 8 }}
              onMouseDown={(ev) => ev.stopPropagation()}
              onClick={(ev) => { ev.stopPropagation(); onConnect(e.from.id, e.port, null); }}
            >
              ×
            </button>
          );
        })}
      </div>

      <div className="pointer-events-none absolute bottom-2 left-2 rounded-8 bg-bg-white-0/80 px-2 py-1 text-[10px] text-text-soft-400">
        {t("blockCanvas.hints")}
      </div>
      <button
        type="button"
        className="absolute bottom-2 right-2 rounded-8 bg-bg-white-0 px-2 py-1 text-[10px] text-text-sub-600 ring-1 ring-stroke-soft-200 hover:text-text-strong-950"
        onClick={() => setView({ x: 40, y: 40, k: 1 })}
      >
        {t("blockCanvas.resetView")}
      </button>
    </div>
  );
}
