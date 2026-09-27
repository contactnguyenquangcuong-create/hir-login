import { useState } from "react";
import { cn } from "@proxyshard/shardx-ui-kit";
import { AlertIcon, CheckCircleIcon, CloseIcon, InfoIcon } from "../../shared/icons";
import { useToastStore } from "../../shared/model/toast";
import { useT } from "../../shared/i18n";
import type { ToastItem } from "../../shared/types";

// One neutral card for every kind, with a coloured edge and icon: the same
// surface, ring and accent bar the rest of the app uses (see the inline editor),
// instead of a full-bleed pink/green block.
const TONE: Record<ToastItem["kind"], { bar: string; icon: string; chip: string }> = {
  ok: {
    bar: "bg-[var(--color-success-base)]",
    icon: "text-[var(--color-success-base)]",
    chip: "bg-[var(--color-success-background)]",
  },
  err: {
    bar: "bg-[var(--color-error-base)]",
    icon: "text-[var(--color-error-base)]",
    chip: "bg-[var(--color-error-background)]",
  },
  info: { bar: "bg-primary-base", icon: "text-primary-base", chip: "bg-primary-alpha-10" },
};

function Toast({ item, onClose }: { item: ToastItem; onClose: () => void }) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const tone = TONE[item.kind];
  const Icon = item.kind === "ok" ? CheckCircleIcon : item.kind === "err" ? AlertIcon : InfoIcon;
  const title =
    item.kind === "ok" ? t("toast.okTitle") : item.kind === "err" ? t("toast.errTitle") : t("toast.infoTitle");
  return (
    <div
      role={item.kind === "err" ? "alert" : "status"}
      className="pointer-events-auto relative flex w-[380px] max-w-[92vw] animate-[toastIn_0.2s_cubic-bezier(.2,.9,.3,1)] gap-3 overflow-hidden rounded-xl bg-bg-white-0 py-3 pl-4 pr-3 shadow-[var(--shadow-md)] ring-1 ring-inset ring-stroke-soft-200"
    >
      <span className={cn("absolute left-0 top-0 h-full w-[3px]", tone.bar)} />
      <span className={cn("mt-0.5 grid size-7 shrink-0 place-items-center rounded-full", tone.chip, tone.icon)}>
        <Icon className="size-4" />
      </span>
      <div className="min-w-0 flex-1">
        <div className="text-label-xs font-semibold text-text-strong-950">{title}</div>
        <div className="mt-0.5 break-words text-paragraph-xs text-text-sub-600">{item.text}</div>
        {item.detail && (
          <>
            <button
              type="button"
              onClick={() => setOpen((v) => !v)}
              className="mt-1.5 cursor-pointer border-0 bg-transparent p-0 text-paragraph-xs text-text-soft-400 underline decoration-dotted hover:text-text-sub-600"
            >
              {t("toast.detail")}
            </button>
            {open && (
              <div className="mono mt-1 max-h-24 select-text overflow-auto break-words rounded-lg bg-bg-weak-50 px-2 py-1.5 text-[10.5px] text-text-soft-400">
                {item.detail}
              </div>
            )}
          </>
        )}
      </div>
      <button
        type="button"
        onClick={onClose}
        aria-label="close"
        className="grid size-6 shrink-0 cursor-pointer place-items-center rounded-md border-0 bg-transparent text-icon-soft-400 transition-colors hover:bg-bg-weak-50 hover:text-text-strong-950"
      >
        <CloseIcon className="size-3.5" />
      </button>
    </div>
  );
}

/// Global toast stack fed by the zustand toast store.
export function ToastHost() {
  const items = useToastStore((s) => s.items);
  const dismiss = useToastStore((s) => s.dismiss);
  if (items.length === 0) return null;
  return (
    <div className="pointer-events-none fixed bottom-6 right-6 z-300 flex flex-col-reverse gap-3">
      {items.map((it) => (
        <Toast key={it.id} item={it} onClose={() => dismiss(it.id)} />
      ))}
    </div>
  );
}
