/// Stat card in the UI-kit card style.
export function Metric({ label, value, accent, pulse }: { label: string; value: string; accent?: boolean; pulse?: boolean }) {
  return (
    <div
      className={`relative overflow-hidden rounded-lg bg-bg-white-0 px-4 py-3.5 ring-1 ring-inset shadow-[var(--shadow-xs)] ${
        accent ? "ring-primary-alpha-24" : "ring-stroke-soft-200"
      }`}
    >
      <div className="text-subheading-2xs text-text-soft-400">{label}</div>
      <div
        className={`mt-1 text-title-h6 tabular-nums ${
          pulse ? "text-success-base" : accent ? "text-primary-base" : "text-text-strong-950"
        }`}
      >
        {value}
      </div>
    </div>
  );
}
