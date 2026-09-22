import { useProfile } from "../../../entities/profile";
import { QuickEditDialog } from "./QuickEditDialog";

/// Store-gated QuickEditDialog (proxy / notes inline edit). Renders nothing
/// while no profile is targeted.
export function ProfileQuickEdit() {
  const quickEdit = useProfile((s) => s.quickEdit);
  const proxies = useProfile((s) => s.proxies);
  const setQuickEdit = useProfile((s) => s.setQuickEdit);
  const reload = useProfile((s) => s.reload);

  if (!quickEdit) return null;

  return (
    <QuickEditDialog
      kind={quickEdit.kind}
      profile={quickEdit.profile}
      proxies={proxies}
      onClose={() => setQuickEdit(null)}
      onSaved={() => { setQuickEdit(null); reload(); }}
    />
  );
}
