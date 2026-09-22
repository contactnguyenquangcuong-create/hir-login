export type ExtensionEntry = {
  id: string;
  name: string;
  version: string;
  description: string;
  /** Largest icon the manifest declares, as a data: URL. Empty when it has none. */
  icon: string;
  path: string;
  size_bytes: number;
  /** "@<unix_secs>". */
  added_at: string;
};
