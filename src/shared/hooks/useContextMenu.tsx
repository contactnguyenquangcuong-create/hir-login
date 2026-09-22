import { useEffect, useLayoutEffect, useRef, useState } from "react";
import type { ContextItem } from "../types";

/// Right-click context menu styled after the UI-kit dropdown surface.
export function useContextMenu() {
  const [menu, setMenu] = useState<{ x: number; y: number; items: ContextItem[] } | null>(null);
  const close = () => setMenu(null);
  useEffect(() => {
    if (!menu) return;
    const dismiss = () => close();
    window.addEventListener("click", dismiss);
    window.addEventListener("scroll", dismiss, true);
    return () => {
      window.removeEventListener("click", dismiss);
      window.removeEventListener("scroll", dismiss, true);
    };
  }, [menu]);
  const open = (e: React.MouseEvent, items: ContextItem[]) => {
    e.preventDefault();
    setMenu({ x: e.clientX, y: e.clientY, items });
  };
  // Clamp menu into viewport post-layout.
  const ref = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!menu || !el) return;
    const { width, height } = el.getBoundingClientRect();
    const pad = 8;
    let left = menu.x;
    let top = menu.y;
    if (left + width > window.innerWidth - pad) {
      left = Math.max(pad, window.innerWidth - width - pad);
    }
    if (top + height > window.innerHeight - pad) {
      top = Math.max(pad, window.innerHeight - height - pad);
    }
    el.style.left = `${left}px`;
    el.style.top = `${top}px`;
  }, [menu]);
  const node = menu ? (
    <div
      ref={ref}
      className="fixed z-9000 min-w-[160px] rounded-12 bg-bg-white-0 p-1 shadow-[var(--shadow-md)] ring-1 ring-inset ring-stroke-soft-200"
      style={{ left: menu.x, top: menu.y }}
      onClick={(e) => e.stopPropagation()}
    >
      {menu.items.map((it, i) =>
        it.sep ? (
          <div key={i} className="my-1 border-t border-stroke-soft-200" />
        ) : (
          <button
            key={i}
            className={`w-full cursor-pointer rounded-8 border-0 bg-transparent px-2.5 py-2 text-left text-label-xs transition-colors hover:bg-bg-weak-50 ${
              it.danger ? "text-error-base" : "text-text-sub-600 hover:text-text-strong-950"
            }`}
            onClick={() => { it.onClick(); close(); }}
          >
            {it.label}
          </button>
        ),
      )}
    </div>
  ) : null;
  return { open, node };
}
