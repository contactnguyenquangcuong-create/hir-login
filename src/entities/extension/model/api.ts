import { invoke } from "@tauri-apps/api/core";
import type { ExtensionEntry } from "./types";

export const extensionList = () => invoke<ExtensionEntry[]>("extension_list");
/** Takes .crx, .zip or unpacked folders; returns what actually went in. */
export const extensionImport = (paths: string[]) =>
  invoke<ExtensionEntry[]>("extension_import", { paths });
/** Web Store link, a bare extension id, or a direct .crx / .zip URL. */
export const extensionImportUrl = (url: string) =>
  invoke<ExtensionEntry>("extension_import_url", { url });
export const extensionDelete = (id: string) => invoke("extension_delete", { id });
