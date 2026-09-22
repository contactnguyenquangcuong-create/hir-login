import { create } from "zustand";
import type { ConfirmButton, ConfirmReq } from "../types";
import { t } from "../i18n";

/// Imperative confirm dialog backed by a zustand store; the ConfirmHost
/// widget renders the current request with the UI-kit modal.
type ConfirmState = {
  req: ConfirmReq | null;
  ask: (req: ConfirmReq) => void;
  clear: () => void;
};

export const useConfirmStore = create<ConfirmState>((set) => ({
  req: null,
  ask: (req) => set({ req }),
  clear: () => set({ req: null }),
}));

export function confirmModal(opts: {
  title?: string;
  message: string;
  buttons?: ConfirmButton[];
  danger?: boolean;
}): Promise<any> {
  return new Promise((resolve) => {
    // Callers pass text that is already translated; the default buttons are
    // the only strings this helper owns.
    const buttons = opts.buttons ?? [
      { label: t("confirm.cancel"), value: false },
      {
        label: opts.danger ? t("confirm.delete") : t("confirm.ok"),
        value: true,
        danger: opts.danger,
        primary: !opts.danger,
      },
    ];
    useConfirmStore.getState().ask({
      title: opts.title,
      message: opts.message,
      buttons,
      resolve: (v) => {
        useConfirmStore.getState().clear();
        resolve(v);
      },
    });
  });
}
