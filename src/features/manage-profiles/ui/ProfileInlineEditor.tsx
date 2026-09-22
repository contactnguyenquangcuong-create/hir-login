import { useProfile } from "../../../entities/profile";
import { InlineEditor } from "./InlineEditor";

/// Store-wired wrapper around InlineEditor used for both the "new profile" row
/// and the per-row expanded editor. Renders nothing without an active draft.
export function ProfileInlineEditor() {
  const draft = useProfile((s) => s.draft);
  const setDraft = useProfile((s) => s.setDraft);
  const proxies = useProfile((s) => s.proxies);
  const fingerprints = useProfile((s) => s.fingerprints);
  const saveDraft = useProfile((s) => s.saveDraft);
  const cancelEdit = useProfile((s) => s.cancelEdit);

  if (!draft) return null;

  return (
    <InlineEditor
      draft={draft}
      setDraft={setDraft}
      proxies={proxies}
      fingerprints={fingerprints}
      onSave={saveDraft}
      onCancel={cancelEdit}
    />
  );
}
