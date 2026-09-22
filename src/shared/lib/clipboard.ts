import { invoke } from "@tauri-apps/api/core";

// OS clipboard via Tauri plugin (webview navigator.clipboard throws).
export const clip = {
  write: (text: string) => invoke("clipboard_write", { text }),
  read: () => invoke<string>("clipboard_read"),
};
