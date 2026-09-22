import { Input } from "@proxyshard/shardx-ui-kit";
import { CopyIcon } from "../icons";
import { useT } from "../i18n";
import { clip } from "../lib/clipboard";
import { toast } from "../model/toast";

/// Read-only value with inline copy button — UI-kit Input + icon action.
export function CopyField({ value, secret }: { value: string; secret?: boolean }) {
  const t = useT();
  return (
    <Input
      readOnly
      inputSize="small"
      type={secret ? "password" : "text"}
      value={value}
      rightIcon={
        <button
          type="button"
          className="pointer-events-auto flex size-6 cursor-pointer items-center justify-center rounded-6 border-0 bg-transparent text-icon-soft-400 transition-colors hover:bg-bg-weak-50 hover:text-icon-strong-950"
          title={t("copyField.copyTitle")}
          onClick={async () => {
            try {
              await clip.write(value);
              toast.ok(t("copyField.copied"));
            } catch (e) {
              toast.err(String(e));
            }
          }}
        >
          <CopyIcon className="size-4" />
        </button>
      }
    />
  );
}
