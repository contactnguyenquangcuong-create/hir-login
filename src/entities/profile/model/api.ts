import { invoke } from "@tauri-apps/api/core";
import type { ProfileMeta } from "./types";

export const profileList = () => invoke<ProfileMeta[]>("profile_list");
export const profileGet = (id: string) => invoke<any>("profile_get", { id });
export const profileSave = (payload: any) => invoke<ProfileMeta>("profile_save", { payload });
export const profileDelete = (id: string) => invoke("profile_delete", { id });
export const profileBulkAndroidToDesktop = (ids: string[], os: string) =>
  invoke<{ index: number; ok: boolean; id: string | null; error: string | null }[]>("profile_bulk_android_to_desktop", { ids, os });
export const profileClone = (id: string) => invoke<ProfileMeta>("profile_clone", { id });
export const profileSetPin = (id: string, pinned: boolean) => invoke("profile_set_pin", { id, pinned });
export const profileSetFolder = (id: string, folder: string) => invoke("profile_set_folder", { id, folder });
export const profileBindProxy = (profileId: string, proxyId: string | null) => invoke("profile_bind_proxy", { profileId, proxyId });
export const profileImport = (payloads: any[]) => invoke<number>("profile_import", { payloads });
/// Folder-per-profile bundle: pairs with profileImportFolder for machine-to-machine moves.
export const profileExportFolder = (ids: string[], dest: string) => invoke<number>("profile_export_folder", { ids, dest });
export const profileImportFolder = (src: string) => invoke<number>("profile_import_folder", { src });
export const profileCreateFromTemplate = (templateId: string) => invoke<ProfileMeta>("profile_create_from_template", { templateId });
export const processList = () => invoke<{ profile_id: string; pid: number; uptime_ms: number }[]>("process_list");
export const processKill = (profileId: string) => invoke<boolean>("process_kill", { profileId });
export const launch = (profileId: string) => invoke<number>("launch", { profileId });

/** A group of profiles that mirror each other's input. Returns the group name. */
export const syncLaunch = (profileIds: string[], group?: string) =>
  invoke<string>("sync_launch", { profileIds, group });

export type SyncMember = { profile: string; excluded: boolean; driving: boolean };
export type SyncStatus = { group: string; members: SyncMember[]; paused: boolean };
export type SyncLayout = "row" | "grid" | "cascade";

export const syncStatus = (group: string) => invoke<SyncStatus>("sync_status", { group });
export const syncSetPaused = (group: string, paused: boolean) =>
  invoke<void>("sync_set_paused", { group, paused });
export const syncArrange = (group: string, layout: SyncLayout) =>
  invoke<void>("sync_arrange", { group, layout });
export const syncStop = (group: string) => invoke<void>("sync_stop", { group });
export const syncSetExcluded = (group: string, profile: string, excluded: boolean) =>
  invoke<void>("sync_set_excluded", { group, profile, excluded });
export const syncClosePanel = () => invoke<void>("sync_close_panel");

export type HelperField = { kind: string; select: boolean; x: number; y: number };
export type HelperReport = { fields: HelperField[] } | null;

export const helperProfiles = () => invoke<string[]>("helper_profiles");
export const helperFields = (profile: string) => invoke<HelperReport>("helper_fields", { profile });
/** Returns how many windows were told to fill — the whole group, when in one. */
export const helperFill = (profile: string) => invoke<number>("helper_fill", { profile });
export const helperShow = (profile: string) => invoke<void>("helper_show", { profile });
export const helperClose = () => invoke<void>("helper_close");
/** The operator closed the panel — a refusal about this page only. */
export const helperDismiss = (profile: string) => invoke<void>("helper_dismiss", { profile });
export const folderDelete = (folder: string, deleteProfiles: boolean) => invoke<number>("folder_delete", { folder, deleteProfiles });
export const cookiesExportToFile = (profileId: string, path: string) => invoke<number>("cookies_export_to_file", { profileId, path });
export const cookiesImport = (profileId: string, cookies: any[]) => invoke<number>("cookies_import", { profileId, cookies });
/** The text of a cookie file: JSON (array or {cookies}), Netscape cookies.txt, or a Facebook cookie string. */
export const cookiesImportText = (profileId: string, text: string) => invoke<number>("cookies_import_text", { profileId, text });
export const enrichPicksForPreset = (presetId: string) => invoke<{ hardware_concurrency?: number; device_memory?: number; platform_version?: string }>("enrich_picks_for_preset", { presetId });
export const hostPlatform = () => invoke<string>("host_platform");

/// Profiles being pulled/pushed right now, plus a counter that moves whenever a
/// background pull changed a local profile (so the list should reload).
export const syncActivity = () => invoke<{ busy: string[]; generation: number }>("sync_activity");
/// A profile was saved or created here: tell the team now instead of waiting.
export const syncKick = () => invoke<void>("sync_kick");
