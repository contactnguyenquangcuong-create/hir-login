import { Input } from "@proxyshard/shardx-ui-kit";
import type { FieldProps } from "../types";

/// Labelled text field — UI-kit Input with the app's simple onChange API.
export function Field({ label, value, onChange, type = "text", placeholder, mono }: FieldProps) {
  return (
    <Input
      label={label}
      inputSize="small"
      type={type}
      value={value}
      placeholder={placeholder}
      className={mono ? "mono" : undefined}
      onChange={(e) => onChange(e.target.value)}
    />
  );
}
