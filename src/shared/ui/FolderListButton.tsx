import { useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { cn } from "@proxyshard/shardx-ui-kit";

/** How many folders a page shows as buttons of its own before the rest go behind this list. */
export const INLINE_FOLDERS = 3;

/** The folders to show as buttons: the first few, and the open one wherever it is in the list —
 *  so the page never says which folder it shows only inside the list. */
export function inlineFolders(folders: string[], open: string, max = INLINE_FOLDERS): string[] {
  const first = folders.slice(0, max);
  if (open && folders.includes(open) && !first.includes(open)) return [...first.slice(0, max - 1), open];
  return first;
}

/**
 * The button that opens the whole list of folders — a thousand of them fit, because the list is a
 * box that scrolls and can be searched, not a row of buttons. Click one to pick it. Where it sits
 * and how it looks is the page's: `className` is the page's own button style.
 */
export function FolderListButton({
  folders,
  value,
  onPick,
  counts,
  className,
  extra,
}: {
  folders: string[];
  /** The open folder, or "" / "all" / anything that is not one of `folders`. */
  value: string;
  onPick: (folder: string) => void;
  counts?: Map<string, number>;
  className?: string;
  /** Items kept above the list, such as "all profiles". */
  extra?: { id: string; label: string }[];
}) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [anchor, setAnchor] = useState<{ left: number; top: number } | null>(null);
  const btnRef = useRef<HTMLButtonElement>(null);
  const panelRef = useRef<HTMLDivElement>(null);

  const matches = useMemo(() => {
    const needle = query.trim().toLowerCase();
    return needle ? folders.filter((f) => f.toLowerCase().includes(needle)) : folders;
  }, [folders, query]);

  const show = () => {
    const r = btnRef.current?.getBoundingClientRect();
    if (r) setAnchor({ left: Math.max(8, Math.min(r.left, window.innerWidth - 328)), top: r.bottom + 4 });
    setQuery("");
    setOpen(true);
  };
  const close = () => setOpen(false);
  const pick = (f: string) => { onPick(f); close(); };

  // Closes on a click anywhere else, on Escape, and when the window is resized under it.
  useEffect(() => {
    if (!open) return;
    const down = (e: MouseEvent) => {
      const el = e.target as Node;
      if (panelRef.current?.contains(el) || btnRef.current?.contains(el)) return;
      close();
    };
    const key = (e: KeyboardEvent) => { if (e.key === "Escape") close(); };
    document.addEventListener("mousedown", down);
    document.addEventListener("keydown", key);
    window.addEventListener("resize", close);
    return () => {
      document.removeEventListener("mousedown", down);
      document.removeEventListener("keydown", key);
      window.removeEventListener("resize", close);
    };
  }, [open]);

  return (
    <>
      <button
        ref={btnRef}
        type="button"
        className={className}
        onClick={() => (open ? close() : show())}
        aria-haspopup="listbox"
        aria-expanded={open}
        title="Xem toàn bộ danh sách thư mục"
      >
        Thư mục
        <span className="rounded-full bg-bg-weak-50 px-1.5 py-px text-[10px] font-semibold text-text-sub-600">{folders.length}</span>
        <span aria-hidden className="text-[9px]">▾</span>
      </button>
      {open && anchor && createPortal(
        <div
          ref={panelRef}
          role="listbox"
          style={{ position: "fixed", left: anchor.left, top: anchor.top, width: 312, zIndex: 1000 }}
          className="flex flex-col gap-1 rounded-12 bg-bg-white-0 p-2 shadow-xl ring-1 ring-stroke-soft-200"
        >
          <input
            autoFocus
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => { if (e.key === "Enter" && matches[0] !== undefined) pick(matches[0]); }}
            placeholder={`Tìm trong ${folders.length} thư mục…`}
            className="h-8 w-full rounded-8 bg-bg-white-0 px-2.5 text-paragraph-sm text-text-strong-950 ring-1 ring-inset ring-stroke-soft-200 outline-none placeholder:text-text-soft-400 focus:ring-primary-base"
          />
          <div className="flex items-center justify-between gap-2 px-1 text-paragraph-xs text-text-soft-400">
            <span>{query.trim() ? `${matches.length} khớp` : `${folders.length} thư mục`}</span>
            <span className="flex gap-3">
              {(extra ?? [{ id: "all", label: "Hiện tất cả profile" }]).map((x) => (
                <button
                  key={x.id}
                  type="button"
                  className="cursor-pointer border-0 bg-transparent p-0 text-paragraph-xs text-primary-base"
                  onClick={() => pick(x.id)}
                >
                  {x.label}
                </button>
              ))}
            </span>
          </div>
          <div className="max-h-[320px] overflow-y-auto">
            {matches.length === 0 && (
              <div className="px-2 py-3 text-center text-paragraph-xs text-text-soft-400">Không có thư mục nào khớp “{query}”.</div>
            )}
            {matches.slice(0, 300).map((f) => (
              <button
                key={f}
                type="button"
                role="option"
                aria-selected={value === f}
                className={cn(
                  "flex w-full cursor-pointer items-center justify-between gap-2 rounded-8 border-0 bg-transparent px-2.5 py-1.5 text-left text-label-xs hover:bg-bg-weak-50",
                  value === f ? "text-primary-base" : "text-text-strong-950",
                )}
                onClick={() => pick(f)}
                title={f}
              >
                <span className="truncate">{f}</span>
                {counts && (
                  <span className="rounded-full bg-bg-weak-50 px-1.5 py-px text-[10px] font-semibold text-text-sub-600">{counts.get(f) ?? 0}</span>
                )}
              </button>
            ))}
            {matches.length > 300 && (
              <div className="px-2 py-2 text-center text-paragraph-xs text-text-soft-400">
                Còn {matches.length - 300} thư mục nữa — gõ thêm để lọc.
              </div>
            )}
          </div>
        </div>,
        document.body,
      )}
    </>
  );
}
