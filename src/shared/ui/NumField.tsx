import { Input } from "@proxyshard/shardx-ui-kit";

/// Labelled numeric field — UI-kit Input in number mode.
export function NumField({ label, value, onChange, step }: { label: string; value: number; onChange: (v: number) => void; step?: number }) {
  return (
    <Input
      label={label}
      inputSize="small"
      type="number"
      step={step ?? 1}
      value={value}
      onChange={(e) => onChange(parseFloat(e.target.value) || 0)}
    />
  );
}
