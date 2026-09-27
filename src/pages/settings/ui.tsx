import type { ReactNode } from "react";

/** The page's one card: a title, an optional line of help, then rows. */
export function Section({
  title, desc, action, children,
}: { title: string; desc?: ReactNode; action?: ReactNode; children: ReactNode }) {
  return (
    <section className="overflow-hidden rounded-xl bg-bg-white-0 shadow-[var(--shadow-xs)] ring-1 ring-inset ring-stroke-soft-200">
      <header className="flex items-start justify-between gap-4 px-5 pb-3 pt-4">
        <div className="flex min-w-0 flex-col gap-1">
          <h3 className="m-0 text-label-md text-text-strong-950">{title}</h3>
          {desc && <p className="m-0 text-paragraph-xs text-text-soft-400">{desc}</p>}
        </div>
        {action && <div className="flex-none">{action}</div>}
      </header>
      <div className="flex flex-col divide-y divide-stroke-soft-200 border-t border-stroke-soft-200">{children}</div>
    </section>
  );
}

/** One setting: label and help on the left, the control on the right. */
export function Row({
  label, hint, children, stack,
}: { label: ReactNode; hint?: ReactNode; children?: ReactNode; stack?: boolean }) {
  return (
    <div className={`flex gap-4 px-5 py-3.5 ${stack ? "flex-col" : "flex-col sm:flex-row sm:items-center sm:justify-between"}`}>
      <div className="flex min-w-0 flex-col gap-0.5 sm:max-w-[55%]">
        <span className="text-label-sm text-text-strong-950">{label}</span>
        {hint && <span className="text-paragraph-xs text-text-soft-400">{hint}</span>}
      </div>
      {children !== undefined && <div className={stack ? "w-full" : "w-full flex-none sm:w-72"}>{children}</div>}
    </div>
  );
}

/** A block inside a Section that is not a label/control pair. */
export function Block({ children }: { children: ReactNode }) {
  return <div className="flex flex-col gap-3 px-5 py-4">{children}</div>;
}

export type Tone = "success" | "warning" | "neutral" | "primary";
const TONES: Record<Tone, string> = {
  success: "bg-success-alpha-10 text-success-base",
  warning: "bg-warning-alpha-10 text-warning-base",
  primary: "bg-primary-alpha-10 text-primary-base",
  neutral: "bg-bg-weak-50 text-text-sub-600",
};
export function Pill({ tone = "neutral", children }: { tone?: Tone; children: ReactNode }) {
  return <span className={`inline-flex items-center gap-1.5 rounded-full px-2 py-0.5 text-paragraph-xs font-medium ${TONES[tone]}`}>{children}</span>;
}
export const Dot = ({ on }: { on: boolean }) => (
  <span className={`inline-block size-1.5 rounded-full ${on ? "bg-success-base" : "bg-text-soft-400"}`} />
);

/** Segmented choice (2-4 options). */
export function Segmented<T extends string>({
  value, onChange, options,
}: { value: T; onChange: (v: T) => void; options: { value: T; label: string }[] }) {
  return (
    <div className="inline-flex rounded-lg bg-bg-weak-50 p-0.5 ring-1 ring-inset ring-stroke-soft-200">
      {options.map((o) => (
        <button
          key={o.value}
          type="button"
          onClick={() => onChange(o.value)}
          className={`rounded-md border-0 px-3 py-1.5 text-label-xs transition-colors ${
            value === o.value ? "bg-bg-white-0 text-text-strong-950 shadow-[var(--shadow-xs)]" : "bg-transparent text-text-sub-600 hover:text-text-strong-950"
          }`}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

export const avatar = (name: string) => (
  <span className="grid size-8 flex-none place-items-center rounded-full bg-primary-alpha-10 text-label-xs text-primary-base">
    {(name.trim()[0] ?? "?").toUpperCase()}
  </span>
);
