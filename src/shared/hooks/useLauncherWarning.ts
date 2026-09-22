import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { toast } from "../model/toast";

// Warnings the backend raises during a launch — a geo probe that failed and
// left the profile on the host's clock, say. stderr is not a place a user
// looks, so they arrive as a toast.
export function useLauncherWarning() {
  useEffect(() => {
    let disposed = false;
    let un: (() => void) | undefined;
    listen<string>("launcher-warning", (e) => toast.err(e.payload)).then((fn) => {
      if (disposed) fn();
      else un = fn;
    });
    return () => {
      disposed = true;
      un?.();
    };
  }, []);
}
