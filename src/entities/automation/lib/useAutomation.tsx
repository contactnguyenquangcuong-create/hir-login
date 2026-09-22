import { create } from "zustand";
import { toast } from "../../../shared/lib/toast";
import { confirmModal } from "../../../shared/lib/confirm";
import { t } from "../../../shared/i18n";
import {
  automationAvailable,
  automationCreate,
  automationDelete,
  automationDuplicate,
  automationList,
  automationSave,
} from "../model/api";
import type { Block, Project } from "../model/types";

export type AutomationStore = {
  status: "idle" | "loading" | "ready" | "error";
  /** False on a build compiled without the automation feature. */
  available: boolean;
  projects: Project[];
  openId: string | null;
  busy: string | null;
  /** Snapshots of a project before each change, newest last. */
  past: Project[];
  future: Project[];

  init: () => Promise<void>;
  reload: () => Promise<void>;
  open: (id: string | null) => void;
  create: (name: string) => Promise<void>;
  rename: (p: Project, name: string) => Promise<void>;
  remove: (p: Project) => Promise<void>;
  duplicate: (p: Project) => Promise<void>;
  patch: (id: string, next: Partial<Project>) => Promise<void>;
  addBlock: (id: string, block: Block, after?: string) => Promise<void>;
  undo: () => Promise<void>;
  redo: () => Promise<void>;
  canUndo: () => boolean;
  canRedo: () => boolean;
  moveBlock: (id: string, blockId: string, by: -1 | 1) => Promise<void>;
  placeBlock: (id: string, blockId: string, x: number, y: number) => void;
  connect: (id: string, from: string, port: "done" | "fail", to: string | null) => Promise<void>;
  current: () => Project | null;
};

export const useAutomation = create<AutomationStore>((set, get) => ({
  status: "idle",
  available: false,
  projects: [],
  openId: null,
  busy: null,
  past: [],
  future: [],

  init: async () => {
    if (get().status === "loading") return;
    set({ status: "loading" });
    try {
      const available = await automationAvailable();
      if (!available) {
        set({ available: false, projects: [], status: "ready" });
        return;
      }
      set({ available: true, projects: await automationList(), status: "ready" });
    } catch (e) {
      set({ status: "error" });
      toast.err(String(e));
    }
  },

  reload: async () => {
    try { set({ projects: await automationList() }); }
    catch (e) { toast.err(String(e)); }
  },

  open: (id) => set({ openId: id }),

  current: () => {
    const { projects, openId } = get();
    return projects.find((p) => p.id === openId) ?? null;
  },

  create: async (name) => {
    try {
      const p = await automationCreate(name);
      await get().reload();
      set({ openId: p.id });
    } catch (e) { toast.err(String(e)); }
  },

  rename: async (p, name) => {
    const trimmed = name.trim();
    if (!trimmed || trimmed === p.name) return;
    await get().patch(p.id, { name: trimmed });
  },

  remove: async (p) => {
    const ok = await confirmModal({
      title: t("useAutomation.deleteTitle"),
      message: p.blocks.length === 1
        ? t("useAutomation.deleteMessageOne", { name: p.name })
        : t("useAutomation.deleteMessage", { name: p.name, n: p.blocks.length }),
      danger: true,
    });
    if (ok !== true) return;
    set({ busy: p.id });
    try {
      await automationDelete(p.id);
      if (get().openId === p.id) set({ openId: null });
      await get().reload();
    } catch (e) { toast.err(String(e)); }
    finally { set({ busy: null }); }
  },

  duplicate: async (p) => {
    set({ busy: p.id });
    try {
      const copy = await automationDuplicate(p.id);
      await get().reload();
      set({ openId: copy.id });
    } catch (e) { toast.err(String(e)); }
    finally { set({ busy: null }); }
  },

  // Saves the whole project: the backend is the single writer, so a partial
  // update would need a merge on both sides.
  //
  // Each change also files the version before it, so a deletion can be taken
  // back. Bounded at 50: a long editing session should not hold every state
  // the project ever had.
  patch: async (id, next) => {
    const before = get().projects.find((p) => p.id === id);
    if (!before) return;
    const merged = { ...before, ...next };
    set({
      projects: get().projects.map((p) => (p.id === id ? merged : p)),
      past: [...get().past.slice(-49), before],
      // A new change makes the redo trail meaningless.
      future: [],
    });
    try { await automationSave(merged); }
    catch (e) { toast.err(String(e)); await get().reload(); }
  },

  canUndo: () => get().past.length > 0,
  canRedo: () => get().future.length > 0,

  undo: async () => {
    const past = get().past;
    const previous = past[past.length - 1];
    if (!previous) return;
    const current = get().projects.find((p) => p.id === previous.id);
    set({
      past: past.slice(0, -1),
      future: current ? [...get().future, current] : get().future,
      projects: get().projects.map((p) => (p.id === previous.id ? previous : p)),
    });
    try { await automationSave(previous); }
    catch (e) { toast.err(String(e)); await get().reload(); }
  },

  redo: async () => {
    const future = get().future;
    const nextOne = future[future.length - 1];
    if (!nextOne) return;
    const current = get().projects.find((p) => p.id === nextOne.id);
    set({
      future: future.slice(0, -1),
      past: current ? [...get().past, current] : get().past,
      projects: get().projects.map((p) => (p.id === nextOne.id ? nextOne : p)),
    });
    try { await automationSave(nextOne); }
    catch (e) { toast.err(String(e)); await get().reload(); }
  },

  // Order matters more here than in a flat list: a jump target is a step id,
  // so moving a step keeps every branch pointing where it did.
  moveBlock: async (id, blockId, by) => {
    const p = get().projects.find((x) => x.id === id);
    if (!p) return;
    const at = p.blocks.findIndex((b) => b.id === blockId);
    const to = at + by;
    if (at < 0 || to < 0 || to >= p.blocks.length) return;
    const blocks = [...p.blocks];
    [blocks[at], blocks[to]] = [blocks[to], blocks[at]];
    await get().patch(id, { blocks });
  },

  // Dragging is continuous, so the move is applied locally on every frame and
  // written once the pointer is released — a save per pixel would queue
  // hundreds of writes behind one drag.
  placeBlock: (id, blockId, x, y) => {
    const p = get().projects.find((x2) => x2.id === id);
    if (!p) return;
    set({
      projects: get().projects.map((x2) =>
        x2.id === id
          ? { ...x2, blocks: x2.blocks.map((b) => (b.id === blockId ? { ...b, x, y } : b)) }
          : x2,
      ),
    });
  },

  connect: async (id, from, port, to) => {
    const p = get().projects.find((x) => x.id === id);
    if (!p) return;
    const which = port === "done" ? "on_done" : "on_fail";
    // Dropping a wire on empty space means "cut this link" — end the branch
    // here. (It used to fall back to "next" for the done port, which silently
    // re-attached the step to whatever sat below it, so a link could not be
    // cut at all.)
    await get().patch(id, {
      blocks: p.blocks.map((b) =>
        b.id === from ? { ...b, [which]: to ? { goto: to } : "stop" } : b,
      ),
    });
  },

  addBlock: async (id, block, after) => {
    const p = get().projects.find((x) => x.id === id);
    if (!p) return;
    const blocks = [...p.blocks];
    const at = after ? blocks.findIndex((b) => b.id === after) : -1;
    // A new step lands after the one it was recorded against, or at the end.
    blocks.splice(at >= 0 ? at + 1 : blocks.length, 0, block);
    await get().patch(id, { blocks });
  },
}));
