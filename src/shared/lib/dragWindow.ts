import { getCurrentWindow } from "@tauri-apps/api/window";

/** Drags the window unless the press landed on a control. Explicit, because
 *  `data-tauri-drag-region` is tested on the exact element hit, not ancestors. */
export function dragWindowOnMouseDown(e: React.MouseEvent): void {
  if (e.button !== 0) return;
  const target = e.target as HTMLElement | null;
  if (target?.closest("button, a, input, select, textarea, [role='button']")) {
    return;
  }
  // Otherwise the press starts a selection that smears as the window moves.
  e.preventDefault();
  void getCurrentWindow().startDragging();
}
