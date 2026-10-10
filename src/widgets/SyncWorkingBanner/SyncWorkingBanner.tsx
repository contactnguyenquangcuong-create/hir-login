import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { syncWorking } from "../../entities/settings";

/** A banner in the corner while profiles are being saved to the team — "wait before you shut the
 *  computer down" — and a short "done, you may" afterwards. The app also refuses to quit (and asks
 *  Windows to wait) while this shows; the banner is how a person knows why. */
export function SyncWorkingBanner() {
  const [n, setN] = useState(0);
  const [done, setDone] = useState(false);
  const shownAt = useRef<number | null>(null);
  const timers = useRef<number[]>([]);

  useEffect(() => {
    let off: (() => void) | undefined;
    let gone = false;
    const apply = (count: number) => {
      // Appears only once it has lasted a moment: a pull that takes a blink should not flash it.
      timers.current.forEach((t) => window.clearTimeout(t));
      timers.current = [];
      if (count > 0) {
        setDone(false);
        if (shownAt.current !== null) { setN(count); return; }
        timers.current.push(window.setTimeout(() => { shownAt.current = Date.now(); setN(count); }, 700));
      } else {
        const wasShown = shownAt.current !== null;
        shownAt.current = null;
        setN(0);
        if (wasShown) {
          setDone(true);
          timers.current.push(window.setTimeout(() => setDone(false), 4000));
        }
      }
    };
    void listen<number>("sync:working", (e) => { if (!gone) apply(Number(e.payload) || 0); }).then((u) => { if (gone) u(); else off = u; });
    void syncWorking().then((c) => { if (!gone) apply(c); }).catch(() => {});
    return () => { gone = true; off?.(); timers.current.forEach((t) => window.clearTimeout(t)); };
  }, []);

  if (n === 0 && !done) return null;
  return (
    <div role="status" aria-live="polite" className="pointer-events-none fixed bottom-4 right-4 z-[900] flex max-w-[360px] items-center gap-3 rounded-12 bg-bg-white-0 px-4 py-3 shadow-xl ring-1 ring-stroke-soft-200">
      {n > 0 ? (
        <>
          <svg width="22" height="22" viewBox="0 0 22 22" className="flex-none animate-spin" aria-hidden>
            <circle cx="11" cy="11" r="8.5" fill="none" strokeWidth="3" className="stroke-bg-soft-200" />
            <circle cx="11" cy="11" r="8.5" fill="none" strokeWidth="3" strokeLinecap="round" strokeDasharray="13 41" className="stroke-primary-base" />
          </svg>
          <div className="flex flex-col">
            <span className="text-label-sm text-text-strong-950">Đang đồng bộ {n} profile lên nhóm</span>
            <span className="text-paragraph-xs text-text-sub-600">Đợi xong rồi hãy tắt máy hoặc thoát app, kẻo mất đăng nhập.</span>
          </div>
        </>
      ) : (
        <>
          <span aria-hidden className="flex size-5 flex-none items-center justify-center rounded-full bg-success-base text-[12px] text-white">✓</span>
          <span className="text-label-sm text-text-strong-950">Đã đồng bộ xong — có thể tắt máy</span>
        </>
      )}
    </div>
  );
}
