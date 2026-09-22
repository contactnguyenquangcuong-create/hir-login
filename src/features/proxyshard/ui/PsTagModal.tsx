import { useState } from "react";
import { DialogModal } from "@proxyshard/shardx-ui-kit";
import { EditIcon } from "../../../shared/icons";
import { Field } from "../../../shared/ui/Field";
import { toast } from "../../../shared/model/toast";
import { useT } from "../../../shared/i18n";
import type { PsOrder } from "../../../entities/proxyshard";
import { psSetTag } from "../../../entities/proxyshard";

export function PsTagModal({ order, onClose, onDone }: { order: PsOrder; onClose: () => void; onDone: () => void }) {
  const t = useT();
  const [tag, setTag] = useState(order.tag && order.tag !== "none" ? order.tag : "");
  const [busy, setBusy] = useState(false);
  const submit = async () => {
    setBusy(true);
    try {
      await psSetTag(order.order_id, tag.trim() || "none");
      toast.ok(t("psTagModal.tagUpdated", { id: order.order_id }));
      onDone();
    } catch (e) { toast.err(String(e)); }
    finally { setBusy(false); }
  };
  return (
    <DialogModal
      open
      onClose={onClose}
      icon={<EditIcon className="size-5" />}
      title={t("psTagModal.title")}
      subtitle={t("psTagModal.subtitle", { name: order.product_name, id: order.order_id })}
      confirmLabel={busy ? t("psTagModal.saving") : t("psTagModal.save")}
      onConfirm={submit}
      isLoading={busy}
      cancelLabel={t("psTagModal.cancel")}
      onCancel={onClose}
    >
      <div className="py-4">
        <Field label={t("psTagModal.tagLabel")} value={tag} onChange={setTag} placeholder={t("psTagModal.tagPlaceholder")} />
      </div>
    </DialogModal>
  );
}
