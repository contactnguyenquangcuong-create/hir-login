import { invoke } from "@tauri-apps/api/core";
import type { Settings, ApiInfo, DataRootInfo, RemoteProfileStatus } from "./types";

export const settingsGet = () => invoke<Settings>("settings_get");
export const settingsSave = (value: Settings) => invoke("settings_save", { value });
/** Why settings.json could not be read, or null when it reads fine. */
export const settingsLoadError = () => invoke<string | null>("settings_load_error");
export const apiInfo = () => invoke<ApiInfo>("api_info");
export const apiRegenerateToken = () => invoke<ApiInfo>("api_regenerate_token");
export const mcpDownload = (dir: string) => invoke<string>("mcp_download", { dir });

export const dataRootGet = () => invoke<DataRootInfo>("data_root_get");
/** Moves the data; progress arrives as `data-migration` events. */
export const dataRootMigrate = (path: string) => invoke<number>("data_root_migrate", { path });

/** "Test connection": lists what the sync server currently knows. Throws if
 *  sync isn't enabled or the server/token is wrong. */
export const teamSyncList = () => invoke<RemoteProfileStatus[]>("team_sync_list");

export type TeamServerStatus = { running: boolean; port?: number; tailscale_ip?: string | null };
export const teamServerStart = (port: number, token: string) => invoke<number>("team_server_start", { port, token });
export const teamServerStop = () => invoke<void>("team_server_stop");
export const teamServerStatus = () => invoke<TeamServerStatus>("team_server_status");
export const teamInviteGenerate = (serverUrl: string, token: string) => invoke<string>("team_invite_generate", { serverUrl, token });
export const teamInviteGenerateWithAuth = (serverUrl: string, token: string, authKey: string) => invoke<string>("team_invite_generate_with_auth", { serverUrl, token, authKey });
export const teamInviteJoin = (code: string) => invoke<{ url: string; token: string }>("team_invite_join", { code });
export const teamSyncPull = () => invoke<number>("team_sync_pull");
export const tailscaleStatus = () => invoke<{ installed: boolean; connected: boolean; ip: string | null }>("tailscale_status");
export const autostartGet = () => invoke<boolean>("autostart_get");
export const autostartSet = (enabled: boolean) => invoke<void>("autostart_set", { enabled });

/** Tailscale OAuth client (scope `auth_keys`) saved once so the app can mint its own
 *  auth keys. `has_secret` says whether one is stored without ever sending it back. */
export const tailscaleOauthGet = () => invoke<{ client_id: string; has_secret: boolean; tag: string }>("tailscale_oauth_get");
export const tailscaleOauthSet = (clientId: string, clientSecret: string, tag: string) =>
  invoke<void>("tailscale_oauth_set", { clientId, clientSecret, tag });
export const tailscaleOauthClear = () => invoke<void>("tailscale_oauth_clear");
export const tailscaleCreateKey = (description: string) => invoke<string>("tailscale_create_key", { description });

export type TailscaleKey = { id: string; description: string; created: string; expires: string; revoked: string; invalid: boolean };
export const tailscaleListKeys = () => invoke<TailscaleKey[]>("tailscale_list_keys");
export const tailscaleRevokeKey = (id: string) => invoke<void>("tailscale_revoke_key", { id });
