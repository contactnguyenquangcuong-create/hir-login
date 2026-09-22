import { useEffect, useRef, useState, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Alert, ProgressBar } from "@proxyshard/shardx-ui-kit";
import { useT } from "../../shared/i18n";
import type { RtStatus, RtProgress } from "../../shared/types";

export function FirstRunGate({ children }: { children: ReactNode }) {
  const t = useT();
  // null = querying backend; true = reveal; false = show overlay.
  const [installed, setInstalled] = useState<boolean | null>(null);
  const [prog, setProg] = useState<RtProgress | null>(null);
  const [err, setErr] = useState<string | null>(null);
  // Single in-flight install at a time.
  const installing = useRef(false);

  const fmt = (b: number) =>
    b < 1024 * 1024 ? `${(b / 1024).toFixed(0)} KB` : `${(b / (1024 * 1024)).toFixed(1)} MB`;

  useEffect(() => {
    let cancelled = false;
    let unProg: (() => void) | undefined;
    let unDone: (() => void) | undefined;

    // Plain-browser dev (vite without Tauri): no IPC — skip the gate so the
    // UI can be previewed; launch attempts will surface their own errors.
    if (!("__TAURI_INTERNALS__" in window)) {
      setInstalled(true);
      return;
    }

    (async () => {
      // Subscribe BEFORE invoking so we don't miss the first event.
      unProg = await listen<RtProgress>("runtime:progress", (e) => {
        if (!cancelled) setProg(e.payload);
      });
      unDone = await listen("runtime:done", () => {
        if (!cancelled) { setProg(null); setInstalled(true); }
      });

      let status: RtStatus;
      try {
        status = await invoke<RtStatus>("runtime_status");
      } catch (e: any) {
        if (!cancelled) setErr(String(e));
        return;
      }
      if (cancelled) return;

      // Unsupported platform: let the user in; launch will error if attempted.
      if (!status.spec) {
        setInstalled(true);
        return;
      }
      // Reveal only when the engine + fingerprints are installed AND up to
      // date. An available engine update (chromium version bump) falls through
      // to the install path below, which re-downloads the changed archives.
      if (status.installed && status.fingerprints_installed && !status.update_available) {
        setInstalled(true);
        return;
      }

      setInstalled(false);
      if (installing.current) return;
      installing.current = true;
      try {
        await invoke<RtStatus>("runtime_install", { force: false });
        if (!cancelled) setInstalled(true);
      } catch (e: any) {
        if (!cancelled) setErr(typeof e === "string" ? e : (e?.message ?? String(e)));
      } finally {
        installing.current = false;
      }
    })();

    return () => {
      cancelled = true;
      unProg?.();
      unDone?.();
    };
  }, []);

  if (installed === null) {
    return null;
  }
  if (installed) {
    return <>{children}</>;
  }

  return (
    <div className="fixed inset-0 z-1000 flex items-center justify-center bg-bg-weak-50 text-text-strong-950">
      <div className="w-[460px] px-9 py-8 text-center">
        <div className="mb-2 text-title-h6">{t("firstRunGate.title")}</div>
        <div className="mb-6 text-paragraph-xs text-text-soft-400">
          {t("firstRunGate.downloadNote", {
            size: prog?.total ? fmt(prog.total) : "150 MB",
          })}
        </div>

        {prog && (
          <>
            <div className="mb-1.5 text-left text-paragraph-xs text-text-soft-400">
              {prog.label} —{" "}
              {prog.phase === "download"
                ? `${fmt(prog.received)} / ${fmt(prog.total)}  (${prog.percent}%)`
                : t("firstRunGate.extracting")}
            </div>
            <ProgressBar value={prog.percent} color="primary" />
          </>
        )}
        {!prog && !err && (
          <div className="text-paragraph-xs text-text-soft-400">{t("firstRunGate.contactingCdn")}</div>
        )}
        {err && (
          <Alert status="error" variant="light" className="mt-3 text-left">
            {err}
          </Alert>
        )}
      </div>
    </div>
  );
}
