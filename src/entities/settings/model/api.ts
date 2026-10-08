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
export type FirewallStatus = { supported: boolean; granted: boolean };
export const firewallStatus = () => invoke<FirewallStatus>("firewall_status");
/** Adds the Windows firewall rules; Windows shows one admin prompt. Rejects if declined. */
export const firewallGrant = () => invoke<void>("firewall_grant");
/** Returns true if stopping also cleared this machine's own (self-pointing) sync config. */
export const teamServerStop = () => invoke<boolean>("team_server_stop");
/** True if this machine hosts its own team *and* is separately synced to a different one. */
export const teamServerConflict = () => invoke<boolean>("team_server_conflict");
/** True if sync already points at a *different* team — hosting and joining stay mutually exclusive. */
export const teamSyncedElsewhere = () => invoke<boolean>("team_synced_elsewhere");
export const teamServerStatus = () => invoke<TeamServerStatus>("team_server_status");
export const teamInviteGenerate = (serverUrl: string, token: string) => invoke<string>("team_invite_generate", { serverUrl, token });
export const teamInviteGenerateWithAuth = (serverUrl: string, token: string, authKey: string) => invoke<string>("team_invite_generate_with_auth", { serverUrl, token, authKey });
export const teamInviteJoin = (code: string) => invoke<{ url: string; token: string }>("team_invite_join", { code });
export const teamSyncPull = () => invoke<number>("team_sync_pull");
/** The last lines of the sync log: what was pulled, made portable, restored or could not be. */
export const syncLogTail = (lines = 120) => invoke<string>("sync_log_tail", { lines });
/** Sends every profile on this machine to the team now, login included. */
export const syncPushAll = () => invoke<{ sent: number; skipped: number }>("sync_push_all");
export const tailscaleStatus = () => invoke<{ installed: boolean; connected: boolean; ip: string | null }>("tailscale_status");
export const autostartGet = () => invoke<boolean>("autostart_get");
export const autostartSet = (enabled: boolean) => invoke<void>("autostart_set", { enabled });

/** Tailscale OAuth client (scope `auth_keys`) saved once so the app can mint its own
 *  auth keys. `has_secret` says whether one is stored without ever sending it back. */
export const tailscaleOauthGet = () => invoke<{ client_id: string; has_secret: boolean; tag: string }>("tailscale_oauth_get");
export const tailscaleOauthSet = (clientId: string, clientSecret: string, tag: string) =>
  invoke<void>("tailscale_oauth_set", { clientId, clientSecret, tag });
export const tailscaleOauthClear = () => invoke<void>("tailscale_oauth_clear");
/** true = this OAuth Client's tailnet matches this machine's own; false = a
 *  mismatch (keys it mints join people to a network unreachable from here);
 *  null = this machine isn't on any tailnet right now to compare against. */
export const tailscaleOauthVerify = () => invoke<boolean | null>("tailscale_oauth_verify");
export const tailscaleCreateKey = (description: string) => invoke<string>("tailscale_create_key", { description });

export type TailscaleKey = { id: string; description: string; created: string; expires: string; revoked: string; invalid: boolean };
export const tailscaleListKeys = () => invoke<TailscaleKey[]>("tailscale_list_keys");
export const tailscaleRevokeKey = (id: string) => invoke<void>("tailscale_revoke_key", { id });
