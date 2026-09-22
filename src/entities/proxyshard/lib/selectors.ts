import { usePsAccount } from "./usePsAccount";

/// True once `ps_me` validated the saved key.
export function usePsConnected() {
    return usePsAccount((s) => s.status === "ok");
}
