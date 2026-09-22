import { useProfile } from "../../../entities/profile";
import { TemplatePicker } from "./TemplatePicker";

/// Store-gated TemplatePicker. Renders nothing while closed.
export function ProfileTemplatePicker() {
  const open = useProfile((s) => s.templatePickerOpen);
  const fingerprints = useProfile((s) => s.fingerprints);
  const createFromTemplate = useProfile((s) => s.createFromTemplate);
  const setTemplatePickerOpen = useProfile((s) => s.setTemplatePickerOpen);

  if (!open) return null;

  return (
    <TemplatePicker
      fingerprints={fingerprints}
      onPick={createFromTemplate}
      onClose={() => setTemplatePickerOpen(false)}
    />
  );
}
