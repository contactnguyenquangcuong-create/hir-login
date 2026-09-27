import { memo } from "react";
import { useProfile } from "../../../entities/profile";
import { InlineEditor } from "./InlineEditor";

/// Store-wired wrapper around InlineEditor used for both the "new profile" row
/// and the per-row expanded editor. Renders nothing without an active draft.
// Takes no props and reads the store itself, so memo keeps the table's own re-renders
// (uptime clock, polling) from redrawing this large form.
export const ProfileInlineEditor = memo(function ProfileInlineEditor() {
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
});
