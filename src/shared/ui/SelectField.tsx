import { Select } from "@proxyshard/shardx-ui-kit";
import type { CSOption } from "../types";

/// Labelled dropdown over a readonly list of primitive options.
export function SelectField<T extends string | number>({
  label, value, onChange, options, format,
}: {
  label: string;
  value: T;
  onChange: (v: T) => void;
  options: readonly T[];
  format?: (v: T) => string;
}) {
  const opts: CSOption<T>[] = options.map((o) => ({
    value: o,
    label: format ? format(o) : String(o),
  }));
  const byKey = new Map(opts.map((o) => [String(o.value), o.value]));
  return (
    <Select
      size="small"
      label={label}
      value={String(value)}
      options={opts.map((o) => ({ value: String(o.value), label: o.label }))}
      onChange={(v) => {
        const next = byKey.get(v);
        if (next !== undefined) onChange(next);
      }}
    />
  );
}
