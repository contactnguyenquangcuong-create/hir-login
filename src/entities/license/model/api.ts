import { invoke } from "@tauri-apps/api/core";
import type { LicenseInfo } from "./types";

export const licenseStatus = () => invoke<boolean>("license_status");
export const licenseActivate = (key: string) => invoke("license_activate", { key });
/** None before activation, or if the local record doesn't belong to this machine. */
export const licenseInfo = () => invoke<LicenseInfo | null>("license_info");
export const licenseSubmitInfo = (name: string, phone: string, email: string) =>
  invoke("license_submit_info", { name, phone, email });
