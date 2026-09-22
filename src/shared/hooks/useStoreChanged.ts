import { useEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";

// Reload when the backend signals an out-of-band store change — a profile or
// proxy created/edited/removed through the automation API or MCP writes
// straight to disk, so the React state never hears about it on its own.  The
// backend emits "store-changed"; without this listener the new items only show
// up after an app restart.  Bursts (e.g. MCP adding many proxies in a loop)
// are coalesced into a single reload.
export function useStoreChanged(onChange: () => void) {
  const cb = useRef(onChange);
  cb.current = onChange;
  useEffect(() => {
    let disposed = false;
    let un: (() => void) | undefined;
    let timer: ReturnType<typeof setTimeout> | undefined;
    listen("store-changed", () => {
      if (timer) clearTimeout(timer);
      timer = setTimeout(() => cb.current(), 200);
    }).then((fn) => {
      if (disposed) fn();
      else un = fn;
    });
    return () => {
      disposed = true;
      if (timer) clearTimeout(timer);
      un?.();
    };
  }, []);
}
